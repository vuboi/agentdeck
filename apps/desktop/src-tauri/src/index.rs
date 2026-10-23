//! A code index of the project folder, like the one an IDE keeps: symbols (classes, functions, methods, types),
//! every identifier use, and imports, parsed with tree-sitter and stored in SQLite under `~/.agentdeck/index`.
//!
//! The agent asks it through the `code_*` tools of the built-in MCP server instead of grepping the tree: a symbol
//! search, the outline of a file, a definition with its body, where a name is used, who imports a file. The index
//! is built in the background when a chat starts and brought up to date (changed files only) before each answer.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, UNIX_EPOCH};

use ignore::WalkBuilder;
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use tree_sitter::{Language, Node, Parser};

/// Files bigger than this are skipped (bundles, generated code).
const MAX_FILE: u64 = 1_000_000;
/// Between two looks at the disk, so a burst of tool calls does one walk.
const RECHECK: Duration = Duration::from_secs(2);
const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS files (id INTEGER PRIMARY KEY, path TEXT NOT NULL UNIQUE, mtime INTEGER NOT NULL, size INTEGER NOT NULL, lang TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS symbols (id INTEGER PRIMARY KEY, file_id INTEGER NOT NULL, name TEXT NOT NULL, kind TEXT NOT NULL, line INTEGER NOT NULL, end_line INTEGER NOT NULL, parent TEXT, sig TEXT NOT NULL);
CREATE INDEX IF NOT EXISTS symbols_name ON symbols(name COLLATE NOCASE);
CREATE INDEX IF NOT EXISTS symbols_file ON symbols(file_id);
CREATE TABLE IF NOT EXISTS refs (name TEXT NOT NULL, file_id INTEGER NOT NULL, line INTEGER NOT NULL);
CREATE INDEX IF NOT EXISTS refs_name ON refs(name);
CREATE INDEX IF NOT EXISTS refs_file ON refs(file_id);
CREATE TABLE IF NOT EXISTS imports (file_id INTEGER NOT NULL, spec TEXT NOT NULL);
CREATE INDEX IF NOT EXISTS imports_file ON imports(file_id);
";

struct Index {
    conn: Connection,
    checked: Option<Instant>,
}

static INDEXES: OnceLock<Mutex<HashMap<String, Arc<Mutex<Index>>>>> = OnceLock::new();
static ROOTS: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

fn norm(root: &str) -> String {
    root.replace('\\', "/").trim_end_matches('/').to_string()
}

fn fnv(text: &str) -> u64 {
    text.bytes().fold(0xcbf29ce484222325, |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3))
}

fn open(root: &str) -> Result<Arc<Mutex<Index>>, String> {
    let mut all = INDEXES.get_or_init(Default::default).lock().map_err(|_| "index busy")?;
    if let Some(found) = all.get(root) {
        return Ok(found.clone());
    }
    let folder = crate::home_folder()?.join(".agentdeck").join("index");
    std::fs::create_dir_all(&folder).map_err(|error| error.to_string())?;
    let conn = Connection::open(folder.join(format!("{:016x}.db", fnv(&root.to_lowercase())))).map_err(|error| error.to_string())?;
    let _ = conn.query_row("PRAGMA journal_mode=WAL", [], |_| Ok(()));
    let _ = conn.execute_batch("PRAGMA synchronous=NORMAL;");
    conn.execute_batch(SCHEMA).map_err(|error| error.to_string())?;
    let index = Arc::new(Mutex::new(Index { conn, checked: None }));
    all.insert(root.to_string(), index.clone());
    Ok(index)
}

/// Tells the index which folder a chat works in, and starts reading it in the background.
pub fn set_root(chat: &str, root: &str) {
    let root = norm(root);
    if root.is_empty() || !Path::new(&root).is_dir() {
        return;
    }
    if !chat.is_empty() {
        if let Ok(mut roots) = ROOTS.get_or_init(Default::default).lock() {
            roots.insert(chat.to_string(), root.clone());
        }
    }
    std::thread::spawn(move || {
        let _ = refresh(&root);
    });
}

fn root_of(chat: &str, args: &Value) -> Result<String, String> {
    if let Some(root) = args["root"].as_str().filter(|root| !root.is_empty()) {
        return Ok(norm(root));
    }
    let roots = ROOTS.get_or_init(Default::default).lock().map_err(|_| "index busy")?;
    roots.get(chat).cloned().ok_or_else(|| "no project folder is known for this chat: pass root (the absolute path of the project)".to_string())
}

// --- parsing ---------------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum Family {
    Js,
    Rust,
    Python,
    Go,
    Java,
    CSharp,
}

/// (name, kind, line, end line, parent, signature)
type Symbol = (String, &'static str, u32, u32, Option<String>, String);

#[derive(Default)]
struct Parsed {
    symbols: Vec<Symbol>,
    refs: HashSet<(String, u32)>,
    imports: Vec<String>,
}

/// The grammar for a file extension, and whether the code is embedded in `<script>` tags.
fn language_of(ext: &str) -> Option<(Family, Language, bool)> {
    Some(match ext {
        "ts" | "mts" | "cts" => (Family::Js, tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(), false),
        "tsx" => (Family::Js, tree_sitter_typescript::LANGUAGE_TSX.into(), false),
        "js" | "jsx" | "mjs" | "cjs" => (Family::Js, tree_sitter_javascript::LANGUAGE.into(), false),
        "svelte" | "vue" => (Family::Js, tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(), true),
        "rs" => (Family::Rust, tree_sitter_rust::LANGUAGE.into(), false),
        "py" => (Family::Python, tree_sitter_python::LANGUAGE.into(), false),
        "go" => (Family::Go, tree_sitter_go::LANGUAGE.into(), false),
        "java" => (Family::Java, tree_sitter_java::LANGUAGE.into(), false),
        "cs" => (Family::CSharp, tree_sitter_c_sharp::LANGUAGE.into(), false),
        _ => return None,
    })
}

fn text<'a>(node: Node, src: &'a [u8]) -> &'a str {
    node.utf8_text(src).unwrap_or_default()
}

fn signature(node: Node, src: &[u8]) -> String {
    let line = text(node, src).lines().next().unwrap_or_default().trim().trim_end_matches('{').trim();
    line.chars().take(140).collect()
}

/// Name, kind and whether it holds other symbols, for a node that declares something.
fn declaration(family: Family, node: Node, src: &[u8]) -> Option<(String, &'static str, bool)> {
    let named = |field: &str| node.child_by_field_name(field).map(|name| text(name, src).to_string()).filter(|name| !name.is_empty());
    let kind = node.kind();
    let (name, label, container) = match (family, kind) {
        (Family::Js, "function_declaration" | "generator_function_declaration") => (named("name")?, "function", false),
        (Family::Js, "class_declaration" | "abstract_class_declaration") => (named("name")?, "class", true),
        (Family::Js, "interface_declaration") => (named("name")?, "interface", true),
        (Family::Js, "type_alias_declaration") => (named("name")?, "type", false),
        (Family::Js, "enum_declaration") => (named("name")?, "enum", true),
        (Family::Js, "internal_module") => (named("name")?, "namespace", true),
        (Family::Js, "method_definition" | "method_signature" | "abstract_method_signature") => (named("name")?, "method", false),
        (Family::Js, "public_field_definition" | "property_signature") => (named("name")?, "field", false),
        (Family::Js, "variable_declarator") => {
            let value = node.child_by_field_name("value").map(|value| value.kind());
            if matches!(value, Some("arrow_function" | "function_expression" | "function")) {
                (named("name")?, "function", false)
            } else {
                // Only top-level constants: a local variable is not worth an entry.
                let declaration = node.parent()?;
                let top = declaration.parent().is_some_and(|up| matches!(up.kind(), "program" | "export_statement"));
                if !top {
                    return None;
                }
                (named("name")?, "variable", false)
            }
        }
        (Family::Rust, "function_item" | "function_signature_item") => (named("name")?, "function", false),
        (Family::Rust, "struct_item") => (named("name")?, "struct", true),
        (Family::Rust, "enum_item") => (named("name")?, "enum", true),
        (Family::Rust, "trait_item") => (named("name")?, "trait", true),
        (Family::Rust, "impl_item") => (named("type")?, "impl", true),
        (Family::Rust, "mod_item") => (named("name")?, "module", true),
        (Family::Rust, "const_item" | "static_item") => (named("name")?, "const", false),
        (Family::Rust, "type_item") => (named("name")?, "type", false),
        (Family::Rust, "macro_definition") => (named("name")?, "macro", false),
        (Family::Python, "function_definition") => (named("name")?, "function", false),
        (Family::Python, "class_definition") => (named("name")?, "class", true),
        (Family::Go, "function_declaration") => (named("name")?, "function", false),
        (Family::Go, "method_declaration") => (named("name")?, "method", false),
        (Family::Go, "type_spec") => (named("name")?, "type", false),
        (Family::Java, "class_declaration") => (named("name")?, "class", true),
        (Family::Java, "interface_declaration") => (named("name")?, "interface", true),
        (Family::Java, "enum_declaration") => (named("name")?, "enum", true),
        (Family::Java, "record_declaration") => (named("name")?, "record", true),
        (Family::Java, "method_declaration" | "constructor_declaration") => (named("name")?, "method", false),
        (Family::CSharp, "class_declaration") => (named("name")?, "class", true),
        (Family::CSharp, "interface_declaration") => (named("name")?, "interface", true),
        (Family::CSharp, "struct_declaration") => (named("name")?, "struct", true),
        (Family::CSharp, "enum_declaration") => (named("name")?, "enum", true),
        (Family::CSharp, "record_declaration") => (named("name")?, "record", true),
        (Family::CSharp, "namespace_declaration") => (named("name")?, "namespace", true),
        (Family::CSharp, "method_declaration" | "constructor_declaration") => (named("name")?, "method", false),
        (Family::CSharp, "property_declaration") => (named("name")?, "property", false),
        _ => return None,
    };
    Some((name, label, container))
}

/// The text of an import node, or None for a node that is not one.
fn import_of(family: Family, node: Node, src: &[u8]) -> Option<String> {
    let kind = node.kind();
    let spec = match (family, kind) {
        (Family::Js, "import_statement" | "export_statement") => text(node.child_by_field_name("source")?, src).trim_matches(|c| c == '"' || c == '\'' || c == '`').to_string(),
        (Family::Go, "import_spec") => text(node.child_by_field_name("path")?, src).trim_matches('"').to_string(),
        (Family::Rust, "use_declaration") | (Family::Python, "import_statement" | "import_from_statement") | (Family::Java, "import_declaration") | (Family::CSharp, "using_directive") => {
            text(node, src).lines().next().unwrap_or_default().trim().trim_end_matches(';').to_string()
        }
        _ => return None,
    };
    (!spec.is_empty()).then_some(spec)
}

fn visit(family: Family, node: Node, src: &[u8], parent: Option<&str>, base: u32, depth: usize, out: &mut Parsed) {
    if depth > 180 {
        return;
    }
    let mut inner = parent.map(str::to_string);
    if let Some((name, kind, container)) = declaration(family, node, src) {
        let line = node.start_position().row as u32 + 1 + base;
        let end = node.end_position().row as u32 + 1 + base;
        out.symbols.push((name.clone(), kind, line, end, parent.map(str::to_string), signature(node, src)));
        if container {
            inner = Some(name);
        }
    } else if matches!(node.kind(), "identifier" | "type_identifier" | "property_identifier" | "field_identifier") {
        let name = text(node, src);
        if name.len() >= 2 {
            out.refs.insert((name.to_string(), node.start_position().row as u32 + 1 + base));
        }
    }
    if let Some(spec) = import_of(family, node, src) {
        out.imports.push(spec);
    }
    for index in 0..node.child_count() {
        if let Some(child) = node.child(index) {
            visit(family, child, src, inner.as_deref(), base, depth + 1, out);
        }
    }
}

/// The `<script>` blocks of a Svelte or Vue file: (code, lines before it).
fn script_blocks(source: &str) -> Vec<(&str, u32)> {
    let mut blocks = Vec::new();
    let mut from = 0;
    while let Some(open) = source[from..].find("<script") {
        let open = from + open;
        let Some(tag_end) = source[open..].find('>') else { break };
        let start = open + tag_end + 1;
        let Some(close) = source[start..].find("</script>") else { break };
        blocks.push((&source[start..start + close], source[..start].matches('\n').count() as u32));
        from = start + close + 9;
    }
    blocks
}

fn parse(ext: &str, source: &str) -> Option<Parsed> {
    let (family, language, embedded) = language_of(ext)?;
    let mut parser = Parser::new();
    parser.set_language(&language).ok()?;
    let mut out = Parsed::default();
    let parts = if embedded { script_blocks(source) } else { vec![(source, 0)] };
    for (code, base) in parts {
        let tree = parser.parse(code, None)?;
        visit(family, tree.root_node(), code.as_bytes(), None, base, 0, &mut out);
    }
    Some(out)
}

// --- keeping the index in step with the disk -----------------------------------------------------------------------

fn skipped(entry: &ignore::DirEntry) -> bool {
    entry.file_name().to_str().is_some_and(|name| matches!(name, "node_modules" | "target" | "dist" | ".git" | ".angular" | ".svelte-kit" | "build" | "out" | "bin" | "obj" | ".next" | "coverage"))
}

fn forget(tx: &rusqlite::Transaction, id: i64) -> rusqlite::Result<()> {
    for table in ["symbols", "refs", "imports"] {
        tx.execute(&format!("DELETE FROM {table} WHERE file_id = ?1"), [id])?;
    }
    tx.execute("DELETE FROM files WHERE id = ?1", [id])?;
    Ok(())
}

/// Reads again the files that changed since the last look (and forgets the deleted ones).
fn refresh(root: &str) -> Result<(), String> {
    let index = open(root)?;
    let mut guard = index.lock().map_err(|_| "index busy")?;
    if guard.checked.is_some_and(|at| at.elapsed() < RECHECK) {
        return Ok(());
    }
    let sql = |error: rusqlite::Error| error.to_string();
    let conn = &mut guard.conn;
    let mut known: HashMap<String, (i64, i64, i64)> = HashMap::new();
    {
        let mut statement = conn.prepare("SELECT path, id, mtime, size FROM files").map_err(sql)?;
        let rows = statement.query_map([], |row| Ok((row.get::<_, String>(0)?, (row.get(1)?, row.get(2)?, row.get(3)?)))).map_err(sql)?;
        known.extend(rows.flatten());
    }
    let tx = conn.transaction().map_err(sql)?;
    let mut seen = HashSet::new();
    let walker = WalkBuilder::new(root).filter_entry(|entry| !skipped(entry)).build();
    for entry in walker.flatten() {
        let path = entry.path();
        if !entry.file_type().is_some_and(|kind| kind.is_file()) {
            continue;
        }
        let Some(ext) = path.extension().and_then(|ext| ext.to_str()).map(str::to_lowercase) else { continue };
        if language_of(&ext).is_none() {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        if meta.len() > MAX_FILE {
            continue;
        }
        let relative = path.to_string_lossy().replace('\\', "/");
        let relative = relative.strip_prefix(root).unwrap_or(&relative).trim_start_matches('/').to_string();
        let mtime = meta.modified().ok().and_then(|time| time.duration_since(UNIX_EPOCH).ok()).map_or(0, |time| time.as_millis() as i64);
        let size = meta.len() as i64;
        seen.insert(relative.clone());
        if known.get(&relative).is_some_and(|old| old.1 == mtime && old.2 == size) {
            continue;
        }
        let Ok(source) = std::fs::read_to_string(path) else { continue };
        let Some(parsed) = parse(&ext, &source) else { continue };
        if let Some(old) = known.get(&relative) {
            forget(&tx, old.0).map_err(sql)?;
        }
        tx.execute("INSERT INTO files (path, mtime, size, lang) VALUES (?1, ?2, ?3, ?4)", params![relative, mtime, size, ext]).map_err(sql)?;
        let id = tx.last_insert_rowid();
        for (name, kind, line, end, parent, sig) in &parsed.symbols {
            tx.execute("INSERT INTO symbols (file_id, name, kind, line, end_line, parent, sig) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)", params![id, name, kind, line, end, parent, sig]).map_err(sql)?;
        }
        for (name, line) in &parsed.refs {
            tx.execute("INSERT INTO refs (name, file_id, line) VALUES (?1, ?2, ?3)", params![name, id, line]).map_err(sql)?;
        }
        for spec in &parsed.imports {
            tx.execute("INSERT INTO imports (file_id, spec) VALUES (?1, ?2)", params![id, spec]).map_err(sql)?;
        }
    }
    for (path, old) in &known {
        if !seen.contains(path) {
            forget(&tx, old.0).map_err(sql)?;
        }
    }
    tx.commit().map_err(sql)?;
    guard.checked = Some(Instant::now());
    Ok(())
}

// --- the tools -----------------------------------------------------------------------------------------------------

fn def(name: &str, description: &str, properties: Value, required: &[&str]) -> Value {
    json!({ "name": name, "description": description, "inputSchema": { "type": "object", "properties": properties, "required": required } })
}

/// The `code_*` tools, as MCP tool definitions.
pub fn tool_defs() -> Vec<Value> {
    let root = json!({ "type": "string", "description": "absolute path of the project folder (left out: the folder of this chat)" });
    vec![
        def("code_search", "Finds symbols (classes, functions, methods, interfaces, types, constants) by name in the indexed project, best match first. Much faster than grep and returns where each is declared. Case-insensitive; a part of a name is enough.", json!({ "query": { "type": "string", "description": "name or part of a name" }, "kind": { "type": "string", "description": "only this kind: class, function, method, interface, type, enum, struct, trait, variable, field, property, module, namespace" }, "limit": { "type": "number", "description": "default 30" }, "root": root }), &["query"]),
        def("code_outline", "The symbols of one file with their lines, like an IDE's structure view. Read this before opening a big file.", json!({ "path": { "type": "string", "description": "file path relative to the project folder" }, "root": root }), &["path"]),
        def("code_definition", "Where a symbol is declared, with its source (up to 60 lines each). Give the exact name.", json!({ "name": { "type": "string" }, "root": root }), &["name"]),
        def("code_references", "Every place an identifier is used (name based, not type resolved: two symbols with one name share results), with the line's text. Declarations are marked def.", json!({ "name": { "type": "string" }, "limit": { "type": "number", "description": "default 60" }, "root": root }), &["name"]),
        def("code_imports", "What a file imports. Relative JS/TS imports are resolved to project files; other imports are listed as written.", json!({ "path": { "type": "string", "description": "file path relative to the project folder" }, "root": root }), &["path"]),
        def("code_importers", "The files that import a file (relative JS/TS/Svelte/Vue imports): what breaks if you change it.", json!({ "path": { "type": "string", "description": "file path relative to the project folder" }, "root": root }), &["path"]),
        def("code_index_status", "How many files and symbols are indexed, and the languages (TypeScript, JavaScript, Svelte, Vue, Rust, Python, Go, Java, C#).", json!({ "root": root }), &[]),
    ]
}

pub fn is_tool(name: &str) -> bool {
    name.starts_with("code_")
}

/// Runs one `code_*` tool. Blocking work (disk, SQLite) happens off the async threads.
pub async fn call(name: &str, args: &Value, chat: &str) -> Result<String, String> {
    let (name, args, chat) = (name.to_string(), args.clone(), chat.to_string());
    tokio::task::spawn_blocking(move || run(&name, &args, &chat)).await.map_err(|error| error.to_string())?
}

fn sql(error: rusqlite::Error) -> String {
    error.to_string()
}

fn run(name: &str, args: &Value, chat: &str) -> Result<String, String> {
    let root = root_of(chat, args)?;
    refresh(&root)?;
    let index = open(&root)?;
    let guard = index.lock().map_err(|_| "index busy")?;
    let conn = &guard.conn;
    let arg = |key: &str| args[key].as_str().map(|value| value.replace('\\', "/").trim_start_matches("./").to_string()).filter(|value| !value.is_empty());
    let limit = |default: usize| args["limit"].as_u64().map_or(default, |value| value as usize).clamp(1, 200);
    match name {
        "code_search" => {
            let query = arg("query").ok_or("send query")?;
            let kind = arg("kind");
            let escaped = query.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
            let mut statement = conn
                .prepare("SELECT s.name, s.kind, f.path, s.line, s.parent, s.sig FROM symbols s JOIN files f ON f.id = s.file_id WHERE s.name LIKE ?1 ESCAPE '\\' AND (?2 IS NULL OR s.kind = ?2) LIMIT 2000")
                .map_err(sql)?;
            let mut rows: Vec<(String, String, String, i64, Option<String>, String)> = statement
                .query_map(params![format!("%{escaped}%"), kind], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)))
                .map_err(sql)?
                .flatten()
                .collect();
            let lower = query.to_lowercase();
            let rank = |name: &str| match name.to_lowercase() {
                ref found if *found == lower => 0,
                ref found if found.starts_with(&lower) => 1,
                _ => 2,
            };
            rows.sort_by(|a, b| rank(&a.0).cmp(&rank(&b.0)).then(a.0.len().cmp(&b.0.len())).then(a.2.cmp(&b.2)).then(a.3.cmp(&b.3)));
            let total = rows.len();
            let shown = limit(30);
            let mut out: Vec<String> = rows
                .iter()
                .take(shown)
                .map(|(name, kind, path, line, parent, sig)| format!("{kind} {name}  {path}:{line}{}  {sig}", parent.as_ref().map(|parent| format!("  in {parent}")).unwrap_or_default()))
                .collect();
            if out.is_empty() {
                return Ok(format!("No symbol matches \"{query}\"."));
            }
            if total > shown {
                out.push(format!("... {} more: narrow the query or pass kind", total - shown));
            }
            Ok(out.join("\n"))
        }
        "code_outline" => {
            let path = arg("path").ok_or("send path")?;
            let mut statement = conn
                .prepare("SELECT s.name, s.kind, s.line, s.end_line, s.parent, s.sig FROM symbols s JOIN files f ON f.id = s.file_id WHERE f.path = ?1 ORDER BY s.line, s.id")
                .map_err(sql)?;
            let rows: Vec<String> = statement
                .query_map([&path], |row| {
                    let parent: Option<String> = row.get(4)?;
                    let (name, kind, line, end, sig): (String, String, i64, i64, String) = (row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(5)?);
                    let _ = name;
                    Ok(format!("{}{kind} {line}-{end}  {sig}", if parent.is_some() { "  " } else { "" }))
                })
                .map_err(sql)?
                .flatten()
                .collect();
            if rows.is_empty() {
                return Ok(format!("{path} is not indexed or has no symbols (indexed: TypeScript, JavaScript, Svelte, Vue, Rust, Python, Go, Java, C#)."));
            }
            Ok(format!("{path}\n{}", rows.join("\n")))
        }
        "code_definition" => {
            let wanted = arg("name").ok_or("send name")?;
            let mut statement = conn
                .prepare("SELECT s.kind, f.path, s.line, s.end_line FROM symbols s JOIN files f ON f.id = s.file_id WHERE s.name = ?1 COLLATE NOCASE ORDER BY f.path, s.line LIMIT 6")
                .map_err(sql)?;
            let rows: Vec<(String, String, usize, usize)> =
                statement.query_map([&wanted], |row| Ok((row.get(0)?, row.get(1)?, row.get::<_, i64>(2)? as usize, row.get::<_, i64>(3)? as usize))).map_err(sql)?.flatten().collect();
            if rows.is_empty() {
                return Ok(format!("No symbol named {wanted}."));
            }
            let mut out = Vec::new();
            for (kind, path, line, end) in rows {
                let source = std::fs::read_to_string(Path::new(&root).join(&path)).unwrap_or_default();
                let last = end.min(line + 59);
                let body: Vec<&str> = source.lines().skip(line - 1).take(last + 1 - line).collect();
                let cut = if end > last { format!("\n... {} more lines", end - last) } else { String::new() };
                out.push(format!("{kind} {wanted}  {path}:{line}-{end}\n{}{cut}", body.join("\n")));
            }
            Ok(out.join("\n\n"))
        }
        "code_references" => {
            let wanted = arg("name").ok_or("send name")?;
            let mut declared: HashSet<(String, i64)> = HashSet::new();
            let mut statement = conn.prepare("SELECT f.path, s.line FROM symbols s JOIN files f ON f.id = s.file_id WHERE s.name = ?1").map_err(sql)?;
            declared.extend(statement.query_map([&wanted], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))).map_err(sql)?.flatten());
            let mut statement = conn.prepare("SELECT f.path, r.line FROM refs r JOIN files f ON f.id = r.file_id WHERE r.name = ?1 ORDER BY f.path, r.line").map_err(sql)?;
            let rows: Vec<(String, i64)> = statement.query_map([&wanted], |row| Ok((row.get(0)?, row.get(1)?))).map_err(sql)?.flatten().collect();
            if rows.is_empty() {
                return Ok(format!("No use of {wanted}."));
            }
            let shown = limit(60);
            let mut out = Vec::new();
            let mut cache: HashMap<String, Vec<String>> = HashMap::new();
            for (path, line) in rows.iter().take(shown) {
                let lines = cache.entry(path.clone()).or_insert_with(|| std::fs::read_to_string(Path::new(&root).join(path)).unwrap_or_default().lines().map(|line| line.trim().chars().take(160).collect()).collect());
                let source = lines.get((*line - 1) as usize).cloned().unwrap_or_default();
                out.push(format!("{path}:{line}: {source}{}", if declared.contains(&(path.clone(), *line)) { "  [def]" } else { "" }));
            }
            if rows.len() > shown {
                out.push(format!("... {} more", rows.len() - shown));
            }
            Ok(out.join("\n"))
        }
        "code_imports" => {
            let path = arg("path").ok_or("send path")?;
            let mut statement = conn.prepare("SELECT i.spec FROM imports i JOIN files f ON f.id = i.file_id WHERE f.path = ?1 ORDER BY i.rowid").map_err(sql)?;
            let specs: Vec<String> = statement.query_map([&path], |row| row.get(0)).map_err(sql)?.flatten().collect();
            if specs.is_empty() {
                return Ok(format!("{path} imports nothing (or is not indexed)."));
            }
            let exists = |candidate: &str| conn.query_row("SELECT 1 FROM files WHERE path = ?1", [candidate], |_| Ok(())).is_ok();
            let out: Vec<String> = specs.iter().map(|spec| match resolve(&path, spec, &exists) {
                Some(file) => format!("{spec}  ->  {file}"),
                None => spec.clone(),
            }).collect();
            Ok(out.join("\n"))
        }
        "code_importers" => {
            let path = arg("path").ok_or("send path")?;
            let stem = Path::new(&path).file_stem().and_then(|stem| stem.to_str()).unwrap_or_default().to_string();
            let folder = Path::new(&path).parent().and_then(|parent| parent.file_name()).and_then(|name| name.to_str()).unwrap_or_default().to_string();
            let mut statement = conn
                .prepare("SELECT f.path, i.spec FROM imports i JOIN files f ON f.id = i.file_id WHERE i.spec LIKE '.%' AND (i.spec LIKE ?1 ESCAPE '\\' OR i.spec LIKE ?2 ESCAPE '\\' OR i.spec LIKE ?3 ESCAPE '\\')")
                .map_err(sql)?;
            let like = |text: &str| text.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
            let rows: Vec<(String, String)> = statement
                .query_map(params![format!("%/{}", like(&stem)), format!("%/{}.%", like(&stem)), format!("%/{}", like(&folder))], |row| Ok((row.get(0)?, row.get(1)?)))
                .map_err(sql)?
                .flatten()
                .collect();
            let exists = |candidate: &str| conn.query_row("SELECT 1 FROM files WHERE path = ?1", [candidate], |_| Ok(())).is_ok();
            let mut found: Vec<String> = rows.into_iter().filter(|(from, spec)| resolve(from, spec, &exists).as_deref() == Some(path.as_str())).map(|(from, _)| from).collect();
            found.sort();
            found.dedup();
            if found.is_empty() {
                return Ok(format!("No indexed file imports {path} by a relative path."));
            }
            Ok(found.join("\n"))
        }
        "code_index_status" => {
            let count = |table: &str| conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row.get::<_, i64>(0)).unwrap_or(0);
            let mut statement = conn.prepare("SELECT lang, COUNT(*) FROM files GROUP BY lang ORDER BY 2 DESC").map_err(sql)?;
            let langs: Vec<String> = statement.query_map([], |row| Ok(format!("{} {}", row.get::<_, String>(0)?, row.get::<_, i64>(1)?))).map_err(sql)?.flatten().collect();
            Ok(format!("{root}\n{} files, {} symbols, {} references, {} imports\n{}", count("files"), count("symbols"), count("refs"), count("imports"), langs.join(", ")))
        }
        other => Err(format!("unknown tool {other}")),
    }
}

/// The project file a relative import points at, when it is one.
fn resolve(from: &str, spec: &str, exists: &dyn Fn(&str) -> bool) -> Option<String> {
    if !spec.starts_with('.') {
        return None;
    }
    let mut parts: Vec<&str> = from.split('/').collect();
    parts.pop();
    for part in spec.split('/') {
        match part {
            "." | "" => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    let base = parts.join("/");
    let mut candidates = vec![base.clone()];
    for ext in ["ts", "tsx", "js", "jsx", "mjs", "svelte", "vue", "d.ts"] {
        candidates.push(format!("{base}.{ext}"));
        candidates.push(format!("{base}/index.{ext}"));
    }
    // `./a.js` written for a `./a.ts` source.
    if let Some(stem) = base.strip_suffix(".js") {
        candidates.push(format!("{stem}.ts"));
        candidates.push(format!("{stem}.tsx"));
    }
    candidates.into_iter().find(|candidate| exists(candidate))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typescript_symbols_refs_and_imports() {
        let parsed = parse("ts", "import { A } from './a';\nexport class Foo {\n  bar(x: number) { return helper(x); }\n}\nexport const make = () => new Foo();\nconst LIMIT = 3;\nfunction helper(n: number) { const local = 1; return n + local; }\n").expect("parsed");
        let names: Vec<(&str, &str, Option<&str>)> = parsed.symbols.iter().map(|s| (s.0.as_str(), s.1, s.4.as_deref())).collect();
        assert!(names.contains(&("Foo", "class", None)));
        assert!(names.contains(&("bar", "method", Some("Foo"))));
        assert!(names.contains(&("make", "function", None)));
        assert!(names.contains(&("LIMIT", "variable", None)));
        assert!(!names.iter().any(|s| s.0 == "local"));
        assert_eq!(parsed.imports, vec!["./a".to_string()]);
        assert!(parsed.refs.iter().any(|r| r.0 == "helper" && r.1 == 3));
    }

    #[test]
    fn svelte_script_lines_are_offset() {
        let parsed = parse("svelte", "<div></div>\n<script lang=\"ts\">\n  function go() {}\n</script>\n").expect("parsed");
        assert_eq!(parsed.symbols[0].0, "go");
        assert_eq!(parsed.symbols[0].2, 3);
    }

    #[test]
    fn rust_and_resolve() {
        let parsed = parse("rs", "pub struct S;\nimpl S { fn f(&self) {} }\n").expect("parsed");
        assert!(parsed.symbols.iter().any(|s| s.0 == "S" && s.1 == "struct"));
        assert!(parsed.symbols.iter().any(|s| s.0 == "f" && s.4.as_deref() == Some("S")));
        let exists = |p: &str| p == "src/lib/a/index.ts";
        assert_eq!(resolve("src/lib/b.ts", "./a", &exists).as_deref(), Some("src/lib/a/index.ts"));
    }

    #[test]
    fn indexes_this_project() {
        let root = format!("{}/../src", env!("CARGO_MANIFEST_DIR"));
        let args = json!({ "root": root });
        let started = Instant::now();
        let status = run("code_index_status", &args, "").expect("status");
        eprintln!("first index {:?}\n{status}", started.elapsed());
        let found = run("code_search", &json!({ "root": root, "query": "openFileTab" }), "").expect("search");
        eprintln!("{found}");
        assert!(found.contains("content-tabs.svelte.ts"));
        let refs = run("code_references", &json!({ "root": root, "name": "openFileTab", "limit": 5 }), "").expect("refs");
        eprintln!("{refs}");
        let importers = run("code_importers", &json!({ "root": root, "path": "lib/content-tabs.svelte.ts" }), "").expect("importers");
        eprintln!("{importers}");
        assert!(importers.contains("ChangesView.svelte"));
        let again = Instant::now();
        let _ = run("code_outline", &json!({ "root": root, "path": "lib/content-tabs.svelte.ts" }), "").expect("outline");
        eprintln!("query {:?}", again.elapsed());
    }
}
