//! Tool definitions and execution for the custom chat loop.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use ag_adapters::agent::permission::{verdict, Action, Mode, Verdict};
use ag_adapters::agent::{allow_entry, file_change};
use serde_json::{json, Value};
use tauri::AppHandle;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

use super::{Turn, OUTPUT_MAX, RESTRICTED_TOOLS};

pub(crate) struct Outcome {
    pub(crate) text: String,
    pub(crate) is_error: bool,
    pub(crate) image: Option<String>,
}

pub(crate) fn failed(text: impl Into<String>) -> Outcome {
    Outcome {
        text: text.into(),
        is_error: true,
        image: None,
    }
}

pub(crate) fn ok(text: impl Into<String>) -> Outcome {
    Outcome {
        text: text.into(),
        is_error: false,
        image: None,
    }
}

/// How much of a tool's output the model gets: all of it when it fits, else its start and its end around a marker that says
/// how much was cut (the middle is the part that matters least). Command output keeps mostly its end, where the verdict
/// is; a page or a file keeps mostly its start.
pub(crate) fn trim_output(text: &str, limit: usize, tail_percent: usize) -> String {
    ag_adapters::agent::output::trim_middle(text, limit, tail_percent)
}
/// The end of a command's output is what matters.
pub(crate) const END_HEAVY: usize = 75;
/// A file or a page is read from the top.
pub(crate) const START_HEAVY: usize = 25;
/// A reply of a tool server: no side matters more.
pub(crate) const BALANCED: usize = 50;

pub(crate) fn resolve(cwd: Option<&Path>, path: &str) -> PathBuf {
    let path = PathBuf::from(path);
    match cwd {
        Some(dir) if path.is_relative() => dir.join(path),
        _ => path,
    }
}

pub(crate) async fn run_bash(command: &str, cwd: Option<&Path>) -> Outcome {
    #[cfg(windows)]
    let mut process = {
        let mut process = Command::new("powershell");
        process.args(["-NoProfile", "-NonInteractive", "-Command", command]);
        process.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        process
    };
    #[cfg(not(windows))]
    let mut process = {
        let mut process = Command::new("sh");
        process.args(["-c", command]);
        process
    };
    process
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(dir) = cwd.filter(|dir| dir.is_dir()) {
        process.current_dir(dir);
    }
    let mut child = match process.spawn() {
        Ok(child) => child,
        Err(error) => return failed(format!("could not start the shell: {error}")),
    };
    // Ends with the app even when the app is killed (servers.rs job).
    if let Some(pid) = child.id() {
        crate::process_guard::adopt_pid(pid);
    }
    // The whole tree ends when the command times out or the turn is stopped (the task is dropped), not just the shell.
    let mut tree = BashTree(child.id());
    let (stdout, stderr) = (child.stdout.take(), child.stderr.take());
    let collect = async {
        // Both pipes are read at once: one nobody reads fills up and blocks the command.
        let (out, err) = tokio::join!(read_capped(stdout), read_capped(stderr));
        let status = child.wait().await;
        (out, err, status)
    };
    let finished = tokio::time::timeout(Duration::from_secs(120), collect).await;
    if finished.is_ok() {
        tree.disarm();
    }
    match finished {
        Ok((out, err, status)) => {
            let text = [out.trim_end(), err.trim_end()]
                .iter()
                .filter(|part| !part.is_empty())
                .copied()
                .collect::<Vec<_>>()
                .join("\n");
            let text = trim_output(&text, OUTPUT_MAX, END_HEAVY);
            match status {
                Ok(status) if status.success() => ok(if text.is_empty() {
                    "(no output)".to_string()
                } else {
                    text
                }),
                Ok(status) => failed(
                    format!("{text}\n(exit code {})", status.code().unwrap_or(-1))
                        .trim()
                        .to_string(),
                ),
                Err(error) => failed(error.to_string()),
            }
        }
        Err(_) => failed("the command ran for more than 120 seconds and was stopped"),
    }
}

/// Ends the process tree of a command that is dropped or timed out before it finished.
struct BashTree(Option<u32>);

impl BashTree {
    fn disarm(&mut self) {
        self.0 = None;
    }
}

impl Drop for BashTree {
    fn drop(&mut self) {
        if let Some(pid) = self.0 {
            crate::process_guard::kill_tree(pid);
        }
    }
}

/// Reads a pipe to its end, keeping only the last `BASH_KEEP` bytes so a command that prints for ever cannot fill memory.
async fn read_capped<R: tokio::io::AsyncRead + Unpin>(pipe: Option<R>) -> String {
    let Some(mut pipe) = pipe else {
        return String::new();
    };
    let mut kept: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 8192];
    while let Ok(read) = pipe.read(&mut chunk).await {
        if read == 0 {
            break;
        }
        kept.extend_from_slice(&chunk[..read]);
        if kept.len() > BASH_KEEP * 2 {
            let extra = kept.len() - BASH_KEEP;
            kept.drain(..extra);
        }
    }
    if kept.len() > BASH_KEEP {
        let extra = kept.len() - BASH_KEEP;
        kept.drain(..extra);
    }
    String::from_utf8_lossy(&kept).into_owned()
}

/// The most of one stream of a command that is kept in memory.
const BASH_KEEP: usize = 256 * 1024;

const SKIPPED_DIRS: [&str; 7] = [
    ".git",
    "node_modules",
    "target",
    "dist",
    ".svelte-kit",
    ".next",
    "__pycache__",
];

/// A name pattern as a regular expression over `/`-separated relative paths: `**` crosses folders, `*` and `?` do not.
fn glob_regex(pattern: &str) -> Option<regex::Regex> {
    let pattern = pattern.replace('\\', "/");
    let pattern = if pattern.contains('/') {
        pattern
    } else {
        format!("**/{pattern}")
    };
    let mut out = String::from("(?i)^");
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' if chars.peek() == Some(&'*') => {
                chars.next();
                if chars.peek() == Some(&'/') {
                    chars.next();
                    out.push_str("(?:.*/)?");
                } else {
                    out.push_str(".*");
                }
            }
            '*' => out.push_str("[^/]*"),
            '?' => out.push_str("[^/]"),
            other => out.push_str(&regex::escape(&other.to_string())),
        }
    }
    out.push('$');
    regex::Regex::new(&out).ok()
}

/// The files under `root` (skipping build and dependency folders) with their paths relative to it.
fn files_under(root: &Path) -> impl Iterator<Item = (PathBuf, String)> {
    let base = root.to_path_buf();
    walkdir::WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| {
            !(entry.file_type().is_dir()
                && entry.depth() > 0
                && SKIPPED_DIRS.contains(&entry.file_name().to_string_lossy().as_ref()))
        })
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .map(move |entry| {
            let relative = entry
                .path()
                .strip_prefix(&base)
                .unwrap_or(entry.path())
                .to_string_lossy()
                .replace('\\', "/");
            (entry.into_path(), relative)
        })
}

pub(crate) fn glob_files(args: &Value, cwd: Option<&Path>) -> Outcome {
    let Some(pattern) = args["pattern"].as_str() else {
        return failed("pattern is required");
    };
    let Some(matcher) = glob_regex(pattern) else {
        return failed("that pattern is not valid");
    };
    let root = resolve(cwd, args["path"].as_str().unwrap_or("."));
    let mut found: Vec<String> = files_under(&root)
        .filter(|(_, relative)| matcher.is_match(relative))
        .map(|(_, relative)| relative)
        .take(500)
        .collect();
    found.sort();
    ok(if found.is_empty() {
        "no file matches".to_string()
    } else {
        found.join("\n")
    })
}

pub(crate) fn grep_files(args: &Value, cwd: Option<&Path>) -> Outcome {
    let Some(pattern) = args["pattern"].as_str() else {
        return failed("pattern is required");
    };
    let ignore = args["ignore_case"].as_bool().unwrap_or(false);
    let Ok(matcher) = regex::RegexBuilder::new(pattern)
        .case_insensitive(ignore)
        .build()
    else {
        return failed("that is not a valid regular expression");
    };
    let only = args["glob"].as_str().and_then(glob_regex);
    let root = resolve(cwd, args["path"].as_str().unwrap_or("."));
    let candidates: Vec<(PathBuf, String)> = if root.is_file() {
        vec![(
            root.clone(),
            root.file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
        )]
    } else {
        files_under(&root).collect()
    };
    let mut lines = Vec::new();
    'files: for (path, relative) in candidates {
        if only.as_ref().is_some_and(|only| !only.is_match(&relative)) {
            continue;
        }
        if std::fs::metadata(&path)
            .map(|meta| meta.len() > 2_000_000)
            .unwrap_or(true)
        {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        if bytes.iter().take(1024).any(|byte| *byte == 0) {
            continue;
        }
        for (index, line) in String::from_utf8_lossy(&bytes).lines().enumerate() {
            if matcher.is_match(line) {
                lines.push(format!(
                    "{relative}:{}: {}",
                    index + 1,
                    line.trim().chars().take(200).collect::<String>()
                ));
                if lines.len() >= 200 {
                    break 'files;
                }
            }
        }
    }
    ok(if lines.is_empty() {
        "no match".to_string()
    } else {
        lines.join(
            "
",
        )
    })
}

/// The text of a page: scripts and styles dropped, tags removed, blank runs collapsed.
fn page_text(html: &str) -> String {
    let strip = |text: String, pattern: &str| {
        regex::Regex::new(pattern)
            .map(|re| re.replace_all(&text, " ").into_owned())
            .unwrap_or(text)
    };
    let text = strip(
        html.to_owned(),
        r"(?is)<script[^>]*>.*?</script>|<style[^>]*>.*?</style>|<noscript[^>]*>.*?</noscript>",
    );
    let text = strip(text, r"(?s)<[^>]+>");
    let text = text
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"");
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// `text` with every run of whitespace (tag gaps included) as one space.
fn collapse_spaces(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `restricted`: a free-tier provider's fetch, which never follows a redirect into this machine or its network.
pub(crate) async fn web_fetch(args: &Value, restricted: bool) -> Outcome {
    let Some(url) = args["url"]
        .as_str()
        .filter(|url| url.starts_with("http://") || url.starts_with("https://"))
    else {
        return failed("url must start with http:// or https://");
    };
    let redirects = if restricted {
        reqwest::redirect::Policy::custom(|attempt| {
            if crate::privacy::internal_url(attempt.url().as_str()) {
                attempt.error("redirect to an internal address is blocked")
            } else if attempt.previous().len() >= 10 {
                attempt.stop()
            } else {
                attempt.follow()
            }
        })
    } else {
        reqwest::redirect::Policy::default()
    };
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent("AgentDeck")
        .redirect(redirects)
        .build()
    {
        Ok(client) => client,
        Err(error) => return failed(error.to_string()),
    };
    match client.get(url).send().await {
        Ok(response) if response.status().is_success() => {
            let html = response
                .headers()
                .get("content-type")
                .and_then(|value| value.to_str().ok())
                .is_some_and(|kind| kind.contains("html"));
            match response.text().await {
                Ok(text) => ok(trim_output(
                    &if html { page_text(&text) } else { text },
                    OUTPUT_MAX,
                    START_HEAVY,
                )),
                Err(error) => failed(error.to_string()),
            }
        }
        Ok(response) => failed(format!("the server answered {}", response.status())),
        Err(error) => failed(error.to_string()),
    }
}

pub(crate) fn list_dir(args: &Value, cwd: Option<&Path>) -> Outcome {
    let dir = resolve(cwd, args["path"].as_str().unwrap_or("."));
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) => return failed(format!("could not list {}: {error}", dir.display())),
    };
    let mut rows: Vec<String> = entries
        .filter_map(Result::ok)
        .map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            match entry.metadata() {
                Ok(meta) if meta.is_dir() => format!("{name}/"),
                Ok(meta) => format!("{name}  ({} bytes)", meta.len()),
                Err(_) => name,
            }
        })
        .collect();
    rows.sort();
    ok(if rows.is_empty() {
        "(empty folder)".to_string()
    } else {
        rows.join("\n")
    })
}

pub(crate) fn make_dir(args: &Value, cwd: Option<&Path>) -> Outcome {
    let Some(path) = args["path"].as_str() else {
        return failed("path is required");
    };
    match std::fs::create_dir_all(resolve(cwd, path)) {
        Ok(()) => ok(format!("Folder ready: {path}")),
        Err(error) => failed(format!("could not create {path}: {error}")),
    }
}

/// Copy or move: refuses to overwrite, so nothing is lost by a slip.
pub(crate) fn transfer(args: &Value, cwd: Option<&Path>, moving: bool) -> Outcome {
    let (Some(from), Some(to)) = (args["from"].as_str(), args["to"].as_str()) else {
        return failed("from and to are required");
    };
    let (source, target) = (resolve(cwd, from), resolve(cwd, to));
    if !source.exists() {
        return failed(format!("{from} does not exist"));
    }
    if target.exists() {
        return failed(format!(
            "{to} already exists: pick another name or Delete it first"
        ));
    }
    if let Some(parent) = target.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let done = if moving {
        std::fs::rename(&source, &target)
    } else if source.is_dir() {
        copy_dir(&source, &target)
    } else {
        std::fs::copy(&source, &target).map(|_| ())
    };
    match done {
        Ok(()) => ok(format!(
            "{} {from} to {to}",
            if moving { "Moved" } else { "Copied" }
        )),
        Err(error) => failed(error.to_string()),
    }
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)?.filter_map(Result::ok) {
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// Deletes a file or folder, but only inside the project folder.
pub(crate) fn delete_path(args: &Value, cwd: Option<&Path>) -> Outcome {
    let Some(path) = args["path"].as_str() else {
        return failed("path is required");
    };
    let target = resolve(cwd, path);
    let (Ok(real), Some(Ok(root))) = (target.canonicalize(), cwd.map(Path::canonicalize)) else {
        return failed(format!(
            "{path} does not exist, or there is no project folder"
        ));
    };
    if real == root || !real.starts_with(&root) {
        return failed("only files and folders inside the project folder can be deleted");
    }
    let done = if real.is_dir() {
        std::fs::remove_dir_all(&real)
    } else {
        std::fs::remove_file(&real)
    };
    match done {
        Ok(()) => ok(format!("Deleted {path}")),
        Err(error) => failed(error.to_string()),
    }
}

/// A web search through Brave's plain HTML page: title, address and snippet of the first results.
pub(crate) async fn web_search(args: &Value) -> Outcome {
    let Some(query) = args["query"]
        .as_str()
        .filter(|query| !query.trim().is_empty())
    else {
        return failed("query is required");
    };
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent("Mozilla/5.0 AgentDeck")
        .build()
    {
        Ok(client) => client,
        Err(error) => return failed(error.to_string()),
    };
    // Prefer the authenticated Brave API when configured; HTML scraping remains the no-key fallback.
    type Parse = fn(&str) -> Vec<(String, String, String)>;
    let mut lists = Vec::new();
    let mut problems = Vec::new();
    if std::env::var("BRAVE_SEARCH_API_KEY")
        .ok()
        .filter(|key| !key.trim().is_empty())
        .is_some()
    {
        match brave_api_results(&client, query).await {
            Ok(rows) => lists.push(rows),
            Err(error) => problems.push(format!("Brave API: {error}")),
        }
    }
    let engines: Vec<(&str, &str, Parse)> = if lists.is_empty() {
        vec![(
            "Brave",
            crate::endpoints::ENDPOINTS.web.search.as_str(),
            brave_results,
        )]
    } else {
        Vec::new()
    };
    let fallback_engines: Vec<(&str, &str, Parse)> = vec![
        (
            "Bing",
            crate::endpoints::ENDPOINTS.web.bing.as_str(),
            bing_results,
        ),
        (
            "DuckDuckGo",
            crate::endpoints::ENDPOINTS.web.duckduckgo.as_str(),
            duckduckgo_results,
        ),
    ];
    let pages = futures_util::future::join_all(engines.iter().chain(fallback_engines.iter()).map(
        |(name, address, read)| {
            let client = &client;
            async move {
                (
                    *name,
                    engine_page(client, address, query)
                        .await
                        .map(|page| read(&page)),
                )
            }
        },
    ))
    .await;
    for (name, result) in pages {
        match result {
            Ok(rows) => lists.push(rows),
            Err(error) => problems.push(format!("{name}: {error}")),
        }
    }
    if lists.iter().all(Vec::is_empty) && !problems.is_empty() {
        let limited = problems.iter().any(|problem| problem.contains("429"));
        let advice = if limited {
            " Do not search again in this turn; answer from what you have or say what you could not check."
        } else {
            ""
        };
        return failed(format!(
            "the searches failed: {}.{advice}",
            problems.join("; ")
        ));
    }
    // Round by round, one row from each engine, the same address kept once.
    let mut seen = std::collections::HashSet::new();
    let mut merged = Vec::new();
    let longest = lists.iter().map(Vec::len).max().unwrap_or(0);
    for index in 0..longest {
        for list in &lists {
            if let Some((title, target, snippet)) = list.get(index) {
                if seen.insert(target.trim_end_matches('/').to_lowercase()) {
                    merged.push(format!(
                        "{}. {title}\n   {target}\n   {snippet}",
                        merged.len() + 1
                    ));
                }
            }
        }
    }
    merged.truncate(8);
    ok(if merged.is_empty() {
        "no results".to_string()
    } else {
        merged.join("\n")
    })
}

/// Brave's official JSON endpoint. The key is read from the environment and never included in tool output.
async fn brave_api_results(
    client: &reqwest::Client,
    query: &str,
) -> Result<Vec<(String, String, String)>, String> {
    let key =
        std::env::var("BRAVE_SEARCH_API_KEY").map_err(|_| "BRAVE_SEARCH_API_KEY is not set")?;
    let response = client
        .get(crate::endpoints::ENDPOINTS.web.search_api.as_str())
        .query(&[("q", query), ("count", "8")])
        .header("Accept", "application/json")
        .header("X-Subscription-Token", key)
        .send()
        .await
        .map_err(|error| error.to_string())?;
    let status = response.status();
    if status.as_u16() == 429 {
        return Err("429 Too Many Requests".into());
    }
    let body: Value = response.json().await.map_err(|error| error.to_string())?;
    if !status.is_success() {
        let detail = body["message"].as_str().unwrap_or("no detail");
        return Err(format!("{status}: {detail}"));
    }
    Ok(body["web"]["results"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| {
            Some((
                item["title"].as_str()?.to_string(),
                item["url"].as_str()?.to_string(),
                item["description"].as_str().unwrap_or_default().to_string(),
            ))
        })
        .collect())
}

/// The page of one search engine for a query. A refusal (429) is said as it is, so the turn can stop asking.
async fn engine_page(
    client: &reqwest::Client,
    address: &str,
    query: &str,
) -> Result<String, String> {
    let response = client
        .get(address)
        .query(&[("q", query)])
        .send()
        .await
        .map_err(|error| error.to_string())?;
    let status = response.status();
    if status.as_u16() == 429 {
        return Err("429 Too Many Requests".into());
    }
    if !status.is_success() {
        return Err(status.to_string());
    }
    response.text().await.map_err(|error| error.to_string())
}

/// The results on a Bing search page: (title, address, snippet) of each `b_algo` block.
pub(crate) fn bing_results(page: &str) -> Vec<(String, String, String)> {
    let (Ok(link), Ok(snippet)) = (
        regex::Regex::new(r#"(?s)<h2><a href="(https?://[^"]+)"[^>]*>(.*?)</a></h2>"#),
        regex::Regex::new(r#"(?s)<p[^>]*>(.*?)</p>"#),
    ) else {
        return Vec::new();
    };
    page.split(r#"class="b_algo""#)
        .skip(1)
        .filter_map(|block| {
            let found = link.captures(block)?;
            let target = found[1].replace("&amp;", "&");
            let title = collapse_spaces(&page_text(&found[2]));
            let snippet = snippet
                .captures(block)
                .map(|found| collapse_spaces(&page_text(&found[1])))
                .unwrap_or_default();
            Some((title, target, snippet))
        })
        .collect()
}

/// The results on a DuckDuckGo HTML page: (title, address, snippet) of each `result__a` link, its snippet by order.
pub(crate) fn duckduckgo_results(page: &str) -> Vec<(String, String, String)> {
    let (Ok(link), Ok(snippet)) = (
        regex::Regex::new(r#"(?s)class="result__a"[^>]*href="([^"]+)"[^>]*>(.*?)</a>"#),
        regex::Regex::new(r#"(?s)class="result__snippet"[^>]*>(.*?)</a>"#),
    ) else {
        return Vec::new();
    };
    let snippets: Vec<String> = snippet
        .captures_iter(page)
        .map(|found| page_text(&found[1]).replace('\n', " "))
        .collect();
    link.captures_iter(page)
        .enumerate()
        .filter_map(|(index, found)| {
            let target = duckduckgo_target(&found[1])?;
            let title = page_text(&found[2]).replace('\n', " ");
            Some((
                title,
                target,
                snippets.get(index).cloned().unwrap_or_default(),
            ))
        })
        .collect()
}

/// The address a DuckDuckGo result link leads to: its `uddg` parameter when the link goes through DuckDuckGo.
fn duckduckgo_target(href: &str) -> Option<String> {
    let href = href.replace("&amp;", "&");
    let full = if href.starts_with("//") {
        format!("https:{href}")
    } else {
        href
    };
    let url = reqwest::Url::parse(&full).ok()?;
    if url.host_str() == Some("duckduckgo.com") && url.path().starts_with("/l/") {
        return url
            .query_pairs()
            .find(|pair| pair.0 == "uddg")
            .map(|pair| pair.1.into_owned());
    }
    full.starts_with("http").then_some(full)
}

/// The results on a Brave search page: (title, address, snippet) of each result block that has a title and an address.
/// DuckDuckGo's HTML page answers 202 with a bot check to this client, so it returned no results.
pub(crate) fn brave_results(page: &str) -> Vec<(String, String, String)> {
    let (Ok(link), Ok(title), Ok(description)) = (
        regex::Regex::new(r#"<a href="(https?://[^"]+)""#),
        regex::Regex::new(r#"class="title search-snippet-title[^"]*" title="([^"]*)""#),
        regex::Regex::new(r#"(?s)class="description[^"]*"[^>]*>(.*?)</div>"#),
    ) else {
        return Vec::new();
    };
    page.split(r#"data-type="web""#)
        .skip(1)
        .filter_map(|block| {
            let target = link.captures(block)?[1].replace("&amp;", "&");
            let title = page_text(&title.captures(block)?[1]).replace('\n', " ");
            let snippet = description
                .captures(block)
                .map(|found| page_text(&found[1]).replace('\n', " "))
                .unwrap_or_default();
            Some((title, target, snippet))
        })
        .collect()
}

/// The file change a Write or Edit makes. A Write over an existing file shows what it replaces, so the diff is
/// the real one and not "every line added".
pub(crate) fn change_for(
    name: &str,
    args: &Value,
    cwd: Option<&Path>,
) -> Option<ag_adapters::agent::FileChange> {
    let mut change = file_change(name, args)?;
    if name == "Write" {
        if let Ok(old) =
            std::fs::read_to_string(resolve(cwd, args["file_path"].as_str().unwrap_or_default()))
        {
            change.removed = old.lines().count();
            change.before = old.chars().take(3000).collect();
        }
    }
    Some(change)
}

pub(crate) fn read_file(args: &Value, cwd: Option<&Path>) -> Outcome {
    let Some(path) = args["file_path"].as_str() else {
        return failed("file_path is required");
    };
    let text = match std::fs::read(resolve(cwd, path)) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(error) => return failed(format!("could not read {path}: {error}")),
    };
    let first = args["offset"].as_u64().unwrap_or(1).max(1) as usize;
    let limit = args["limit"].as_u64().unwrap_or(2000) as usize;
    let numbered: Vec<String> = text
        .lines()
        .enumerate()
        .skip(first - 1)
        .take(limit)
        .map(|(index, line)| format!("{:>6}\t{line}", index + 1))
        .collect();
    ok(if numbered.is_empty() {
        "(empty)".to_string()
    } else {
        trim_output(&numbered.join("\n"), OUTPUT_MAX * 3, START_HEAVY)
    })
}

pub(crate) fn write_file(args: &Value, cwd: Option<&Path>) -> Outcome {
    let (Some(path), Some(content)) = (args["file_path"].as_str(), args["content"].as_str()) else {
        return failed("file_path and content are required");
    };
    let target = resolve(cwd, path);
    if let Some(parent) = target.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let existed = target.exists();
    match std::fs::write(&target, content) {
        Ok(()) => ok(format!(
            "{} {} lines {} {path}",
            if existed { "Updated" } else { "Created" },
            content.lines().count(),
            if existed { "in" } else { "in new file" }
        )),
        Err(error) => failed(format!("could not write {path}: {error}")),
    }
}

/// A key that names one file however it was written (relative, with `./`, back slashes, other case on Windows).
pub(crate) fn path_key(cwd: Option<&Path>, path: &str) -> String {
    let full = resolve(cwd, path);
    let full = std::fs::canonicalize(&full).unwrap_or(full);
    full.to_string_lossy()
        .replace('\\', "/")
        .trim_start_matches("//?/")
        .to_lowercase()
}

/// Lines of `text` around `at` (0-based), numbered, for showing the model what is really there.
fn numbered_around(text: &str, at: usize, before: usize, after: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let from = at.saturating_sub(before);
    let to = (at + after + 1).min(lines.len());
    (from..to)
        .map(|index| format!("{:>6}\t{}", index + 1, lines[index]))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Why an Edit did not find its text: the nearest place in the file, so the next try can copy it exactly.
fn not_found(text: &str, old: &str, path: &str) -> String {
    let wanted: Vec<&str> = old
        .lines()
        .map(str::trim)
        .filter(|line| line.len() > 3)
        .collect();
    let lines: Vec<&str> = text.lines().collect();
    let hit = wanted
        .iter()
        .find_map(|line| lines.iter().position(|candidate| candidate.trim() == *line))
        .or_else(|| {
            wanted
                .iter()
                .find_map(|line| lines.iter().position(|candidate| candidate.contains(*line)))
        });
    match hit {
        Some(at) => format!("old_string was not found in {path}: it must match the file exactly, spaces, tabs and line breaks included. The closest place is at line {}:\n{}\nCopy the text from here (Read the lines if you need more) and try again.", at + 1, numbered_around(text, at, 2, 8)),
        None => format!("old_string was not found in {path}, and none of its lines is in the file. Read the file (or code_outline it), then copy the exact text you want to replace."),
    }
}

/// The Plan tool: what will change, and where. Returns the plan as the model should see it, and the files it names.
pub(crate) fn plan_tool(
    args: &Value,
    cwd: Option<&Path>,
) -> (Outcome, Option<std::collections::HashSet<String>>) {
    let list = |name: &str| {
        args[name]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let (files, steps) = (list("files"), list("steps"));
    if files.is_empty() || steps.is_empty() {
        return (failed("Give the files you will change (find them with code_search, Grep or Glob first) and the steps."), None);
    }
    let mut keys = std::collections::HashSet::new();
    let mut out = format!(
        "Plan recorded: {}\nFiles:",
        args["goal"].as_str().unwrap_or_default()
    );
    for file in &files {
        keys.insert(path_key(cwd, file));
        out.push_str(&format!(
            "\n- {file} ({})",
            if resolve(cwd, file).exists() {
                "exists: Read it before you edit it"
            } else {
                "will be created"
            }
        ));
    }
    out.push_str("\nSteps:");
    for (index, step) in steps.iter().enumerate() {
        out.push_str(&format!("\n{}. {step}", index + 1));
    }
    if let Some(checks) = args["checks"]
        .as_str()
        .filter(|text| !text.trim().is_empty())
    {
        out.push_str(&format!("\nCheck with: {checks}"));
    }
    out.push_str("\nDo step 1 now. After the last step, run the check and look at the result before you answer.");
    (ok(out), Some(keys))
}

/// The latest version of a package, from its registry.
pub(crate) async fn package_info(args: &Value) -> Outcome {
    let Some(name) = args["name"]
        .as_str()
        .map(str::trim)
        .filter(|name| !name.is_empty() && name.len() < 120 && !name.contains(char::is_whitespace))
    else {
        return failed("name is required");
    };
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent("AgentDeck")
        .build()
    {
        Ok(client) => client,
        Err(error) => return failed(error.to_string()),
    };
    let get = |url: String| {
        let client = client.clone();
        async move {
            client
                .get(url)
                .send()
                .await
                .map_err(|error| error.to_string())?
                .error_for_status()
                .map_err(|error| error.to_string())?
                .json::<Value>()
                .await
                .map_err(|error| error.to_string())
        }
    };
    let text = |value: &Value| value.as_str().unwrap_or_default().to_owned();
    match args["ecosystem"].as_str().unwrap_or("npm") {
        "crates" | "cargo" | "rust" => {
            match get(format!("https://crates.io/api/v1/crates/{name}")).await {
                Ok(json) => {
                    let krate = &json["crate"];
                    ok(format!(
                        "{name} (crates.io)\nlatest stable: {}\nnewest: {}\n{}\ndocs: {}\nhome: {}",
                        text(&krate["max_stable_version"]),
                        text(&krate["newest_version"]),
                        text(&krate["description"]),
                        text(&krate["documentation"]),
                        text(&krate["homepage"])
                    ))
                }
                Err(error) => failed(format!("no answer for {name}: {error}")),
            }
        }
        "pypi" | "python" => match get(format!("https://pypi.org/pypi/{name}/json")).await {
            Ok(json) => ok(format!(
                "{name} (PyPI)\nlatest: {}\n{}\nhome: {}\nrequires python: {}",
                text(&json["info"]["version"]),
                text(&json["info"]["summary"]),
                text(&json["info"]["home_page"]),
                text(&json["info"]["requires_python"])
            )),
            Err(error) => failed(format!("no answer for {name}: {error}")),
        },
        _ => match get(format!(
            "https://registry.npmjs.org/{}/latest",
            name.replace('/', "%2F")
        ))
        .await
        {
            Ok(json) => {
                let tags = get(format!(
                    "https://registry.npmjs.org/-/package/{}/dist-tags",
                    name.replace('/', "%2F")
                ))
                .await
                .map(|tags| tags.to_string())
                .unwrap_or_default();
                let deprecated = json["deprecated"]
                    .as_str()
                    .map(|why| format!("\nDEPRECATED: {why}"))
                    .unwrap_or_default();
                ok(format!(
                    "{name} (npm)\nlatest: {}\ntags: {tags}\n{}\nhome: {}{deprecated}",
                    text(&json["version"]),
                    text(&json["description"]),
                    text(&json["homepage"])
                ))
            }
            Err(error) => failed(format!("no answer for {name}: {error}")),
        },
    }
}

/// What the project says about itself, for the model: the versions of its main libraries (so it writes for those, not for
/// the ones it remembers), its scripts, and its own rules file.
pub(crate) fn project_brief(cwd: Option<&Path>) -> String {
    let Some(dir) = cwd else { return String::new() };
    let mut out = String::new();
    if let Ok(text) = std::fs::read_to_string(dir.join("package.json")) {
        if let Ok(json) = serde_json::from_str::<Value>(&text) {
            const MAIN: [&str; 24] = [
                "@angular/core",
                "@angular/material",
                "svelte",
                "@sveltejs/kit",
                "react",
                "react-dom",
                "next",
                "vue",
                "nuxt",
                "solid-js",
                "tailwindcss",
                "vite",
                "typescript",
                "rxjs",
                "zod",
                "@tauri-apps/api",
                "@ngrx/store",
                "@tanstack/query-core",
                "pinia",
                "express",
                "nestjs",
                "@nestjs/core",
                "prisma",
                "drizzle-orm",
            ];
            let mut found = Vec::new();
            for section in ["dependencies", "devDependencies"] {
                for (name, version) in json[section].as_object().into_iter().flatten() {
                    if MAIN.contains(&name.as_str()) || name.starts_with("@shival99/") {
                        found.push(format!("{name}@{}", version.as_str().unwrap_or_default()));
                    }
                }
            }
            if !found.is_empty() {
                out.push_str(&format!("\n\nThe project's own versions (write code for these, not for older ones you may remember): {}.", found.join(", ")));
            }
            let scripts: Vec<&str> = json["scripts"]
                .as_object()
                .into_iter()
                .flat_map(|map| map.keys().map(String::as_str))
                .take(16)
                .collect();
            if !scripts.is_empty() {
                out.push_str(&format!("\npackage.json scripts: {}.", scripts.join(", ")));
            }
        }
    }
    for file in ["AGENTS.md", "CLAUDE.md"] {
        if let Ok(text) = std::fs::read_to_string(dir.join(file)) {
            let rules: String = text.chars().take(4500).collect();
            out.push_str(&format!(
                "\n\nThe project's rules ({file}); follow them:\n{rules}"
            ));
            break;
        }
    }
    out
}

pub(crate) fn edit_file(args: &Value, cwd: Option<&Path>) -> Outcome {
    let (Some(path), Some(old), Some(new)) = (
        args["file_path"].as_str(),
        args["old_string"].as_str(),
        args["new_string"].as_str(),
    ) else {
        return failed("file_path, old_string and new_string are required");
    };
    let target = resolve(cwd, path);
    let text = match std::fs::read_to_string(&target) {
        Ok(text) => text,
        Err(error) => return failed(format!("could not read {path}: {error}")),
    };
    // A file with Windows line ends, and a model that wrote its text with plain ones: the same text.
    let crlf = text.contains("\r\n");
    let fit = |piece: &str| {
        if crlf && !piece.contains("\r\n") {
            piece.replace('\n', "\r\n")
        } else {
            piece.to_owned()
        }
    };
    let (old, new) = (fit(old), fit(new));
    let (old, new) = (old.as_str(), new.as_str());
    match text.matches(old).count() {
        0 => failed(not_found(&text, old, path)),
        1 => {
            let line = text[..text.find(old).unwrap_or(0)].matches('\n').count();
            let updated = text.replacen(old, new, 1);
            match std::fs::write(&target, &updated) {
                // The result is shown back, so the model sees where its text landed and what is around it.
                Ok(()) => ok(format!(
                    "Edited {path} at line {}. It now reads:\n{}",
                    line + 1,
                    numbered_around(&updated, line, 3, new.lines().count() + 2)
                )),
                Err(error) => failed(format!("could not write {path}: {error}")),
            }
        }
        count if args["replace_all"].as_bool() == Some(true) => {
            match std::fs::write(&target, text.replace(old, new)) {
                Ok(()) => ok(format!("Edited {path}: {count} replacements")),
                Err(error) => failed(format!("could not write {path}: {error}")),
            }
        }
        count => failed(format!(
            "old_string appears {count} times: add more surrounding text, or set replace_all"
        )),
    }
}

/// Calls one AgentDeck browser tool through the MCP server the app runs for itself.
pub(crate) async fn browser_call(app: &AppHandle, chat: &str, name: &str, args: &Value) -> Outcome {
    let request = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": args } }).to_string();
    let Some(answer) = crate::mcp::handle(app, &request, chat).await else {
        return failed("the browser did not answer");
    };
    let Ok(answer) = serde_json::from_str::<Value>(&answer) else {
        return failed("the browser answered with something unreadable");
    };
    let result = &answer["result"];
    let mut text = Vec::new();
    let mut image = None;
    for part in result["content"].as_array().into_iter().flatten() {
        match part["type"].as_str() {
            Some("text") => text.push(part["text"].as_str().unwrap_or_default().to_owned()),
            Some("image") => {
                image = Some(format!(
                    "data:{};base64,{}",
                    part["mimeType"].as_str().unwrap_or("image/png"),
                    part["data"].as_str().unwrap_or_default()
                ));
            }
            _ => {}
        }
    }
    let mut text = text.join("\n");
    if text.is_empty() && image.is_some() {
        text = "Screenshot taken.".into();
    }
    Outcome {
        text: trim_output(&text, OUTPUT_MAX, BALANCED),
        is_error: result["isError"].as_bool().unwrap_or(false),
        image,
    }
}

pub(crate) enum Decision {
    Allow,
    /// Asked every time, whatever the mode and whatever was allowed before: a command that is risky but has real uses
    /// (see `ag_adapters::agent::guard`). The text says what to look at.
    Careful(&'static str),
    Deny(&'static str),
    Ask,
}

/// Whether a tool call may run now, under the permission mode and what the user approved before.
/// `external`: the tool comes from an MCP server the user set up (AgentDeck's own browser tools are not external).
pub(crate) fn decide(turn: &Turn, name: &str, args: &Value, external: bool) -> Decision {
    // Whatever the model asks for, a restricted provider runs nothing that touches this machine.
    if turn.restricted && (external || !RESTRICTED_TOOLS.contains(&name)) {
        return Decision::Deny("this provider has no access to the project or the machine: say what is needed and another agent will do it.");
    }
    if turn.restricted {
        // Its web tools must not carry a secret out in a query, nor reach this machine or its network.
        if name == "WebFetch"
            && args["url"]
                .as_str()
                .is_some_and(crate::privacy::internal_url)
        {
            return Decision::Deny("internal addresses (localhost, private IPs, .internal hosts) are blocked for this provider.");
        }
        if crate::privacy::private_content(&args.to_string()).is_some() {
            return Decision::Deny("the call holds a secret, an environment variable or an internal address: blocked for this provider.");
        }
    }
    let mode = Mode::parse(turn.permission_mode.as_deref());
    // The guard only makes a call stricter and runs before the mode and before any earlier "always allow": a heavy command
    // is refused in every mode, a risky one is asked about each time (plan mode still refuses every command).
    if name == "Bash" {
        match ag_adapters::agent::guard::assess(args["command"].as_str().unwrap_or_default()) {
            Some(ag_adapters::agent::guard::Risk::Block(reason)) => return Decision::Deny(reason),
            Some(ag_adapters::agent::guard::Risk::Confirm(reason)) if mode != Mode::Plan => {
                return Decision::Careful(reason)
            }
            _ => {}
        }
    }
    let approved = turn
        .allowed
        .iter()
        .any(|entry| entry == name || *entry == allow_entry(name, args));
    let (action, refusal) = match name {
        "Read" | "Glob" | "Grep" | "WebFetch" | "WebSearch" | "Skill" | "LS" => {
            return Decision::Allow
        }
        "Write" | "Edit" | "MkDir" | "Move" | "Copy" => (
            Action::Write,
            "plan mode: nothing may be changed. Describe the plan instead.",
        ),
        "Bash" | "Delete" => (
            Action::Shell,
            "plan mode: commands may not run. Describe the plan instead.",
        ),
        // External tools from the user's MCP servers.
        _ if external => (Action::Other, "plan mode: external tools may not run."),
        // The browser tools of AgentDeck itself.
        _ => return Decision::Allow,
    };
    match verdict(mode, action) {
        Verdict::Approve => Decision::Allow,
        Verdict::Refuse => Decision::Deny(refusal),
        Verdict::Ask if approved => Decision::Allow,
        Verdict::Ask => Decision::Ask,
    }
}
