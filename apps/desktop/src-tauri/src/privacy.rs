//! What must never reach a free-tier provider (one that may keep and train on what it is sent): secrets, environment
//! files, internal addresses and the files that hold them. Checked by `custom_chat::run_turn` before anything is sent
//! and by `custom_chat::decide` before a tool runs, whatever the main agent (Claude, Codex, Antigravity) or the user
//! put in the message. The frontend says the same to the main agent (`privateContentIn`, delegate-task.ts); this is the
//! gate that cannot be skipped.

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

static PATTERNS: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    [
        (r"-----BEGIN [A-Z ]*PRIVATE KEY-----", "a private key"),
        (
            r"\b(?:sk-(?:ant-|proj-)?[A-Za-z0-9_-]{20,}|gh[pousr]_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{30,}|glpat-[A-Za-z0-9_-]{20,}|xox[abposr]-[A-Za-z0-9-]{10,}|AKIA[0-9A-Z]{16}|AIza[0-9A-Za-z_-]{35}|gsk_[A-Za-z0-9]{20,}|sk_live_[A-Za-z0-9]{16,}|hf_[A-Za-z0-9]{30,})",
            "an API key or token",
        ),
        (r"\beyJ[A-Za-z0-9_-]{10,}\.eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}", "a signed token (JWT)"),
        (r"(?i)\b[a-z][a-z0-9+.-]*://[^\s:/@]+:[^\s@/]+@", "an address with a password"),
        (
            r#"(?i)(?:^|[^a-z])(?:password|passwd|pwd|passphrase|secret|api[_-]?key|access[_-]?key|private[_-]?key|access[_-]?token|auth[_-]?token|refresh[_-]?token|bearer|client[_-]?secret)["']?\s*[:=]\s*["']?[^\s"']{6,}"#,
            "a password or secret",
        ),
        (r"(?i)\bauthorization:\s*(?:bearer|basic)\s+\S{8,}", "an authorization header"),
        // An environment variable with a value: `NAME=value`, `export NAME=value`, `$env:NAME = "value"`.
        (
            r#"(?m)^\s*(?:export\s+|set\s+|\$env:)?[A-Z][A-Z0-9_]*_(?:KEY|TOKEN|SECRET|PASSWORD|PASS|PWD|CREDENTIALS?|DSN|URL)\s*=\s*\S+"#,
            "an environment variable",
        ),
        (
            r"\b(?:10(?:\.\d{1,3}){3}|127(?:\.\d{1,3}){3}|192\.168(?:\.\d{1,3}){2}|172\.(?:1[6-9]|2\d|3[01])(?:\.\d{1,3}){2}|169\.254(?:\.\d{1,3}){2})\b",
            "an internal IP address",
        ),
        (
            r"(?i)\b(?:localhost|[a-z0-9-]+(?:\.[a-z0-9-]+)*\.(?:internal|local|lan|corp|intranet|home\.arpa))(?::\d+)?\b",
            "an internal host name",
        ),
    ]
    .into_iter()
    .filter_map(|(pattern, what)| Regex::new(pattern).ok().map(|regex| (regex, what)))
    .collect()
});

/// Lines of `NAME=value` that make a text an environment file, whatever the names.
const ENV_LINES_MAX: usize = 3;

static ENV_LINE: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"(?m)^\s*(?:export\s+)?[A-Z][A-Z0-9_]{2,}\s*=\s*\S").ok());

/// What in `text` must not reach a free tier, or `None`.
pub fn private_content(text: &str) -> Option<&'static str> {
    if let Some((_, what)) = PATTERNS.iter().find(|(regex, _)| regex.is_match(text)) {
        return Some(what);
    }
    let env_lines = ENV_LINE.as_ref().map_or(0, |regex| regex.find_iter(text).count());
    (env_lines >= ENV_LINES_MAX).then_some("an environment file")
}

/// A file that holds secrets by its name: environment files, keys, certificates, credential and password stores.
pub fn sensitive_file(path: &str) -> bool {
    let name = Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let extension = Path::new(&name)
        .extension()
        .map(|ext| ext.to_string_lossy().into_owned())
        .unwrap_or_default();
    name.starts_with(".env")
        || name.ends_with(".env")
        || name.starts_with("id_rsa")
        || name.starts_with("id_ed25519")
        || name.starts_with("id_ecdsa")
        || [".npmrc", ".pypirc", ".netrc", "_netrc", ".pgpass", ".git-credentials", ".htpasswd", "known_hosts", "authorized_keys"]
            .contains(&name.as_str())
        || ["pem", "key", "p12", "pfx", "jks", "keystore", "kdbx", "ppk", "asc", "gpg", "ovpn"].contains(&extension.as_str())
        || ["credential", "secret", "password", "passwd", "token", "apikey", "api_key"]
            .iter()
            .any(|word| name.contains(word))
}

/// Whether a web address points inside this machine or its network (loopback, private, link-local, `.internal`...).
pub fn internal_url(url: &str) -> bool {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host = authority.rsplit('@').next().unwrap_or_default();
    let host = if let Some(inner) = host.strip_prefix('[') {
        inner.split(']').next().unwrap_or_default()
    } else {
        host.split(':').next().unwrap_or_default()
    }
    .to_lowercase();
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        return match ip {
            std::net::IpAddr::V4(v4) => {
                v4.is_private() || v4.is_loopback() || v4.is_link_local() || v4.is_unspecified() || v4.is_broadcast()
            }
            std::net::IpAddr::V6(v6) => {
                v6.is_loopback() || v6.is_unspecified() || (v6.segments()[0] & 0xfe00) == 0xfc00 || (v6.segments()[0] & 0xffc0) == 0xfe80
            }
        };
    }
    host.is_empty()
        || host == "localhost"
        || !host.contains('.')
        || [".localhost", ".internal", ".local", ".lan", ".corp", ".intranet", ".home.arpa"]
            .iter()
            .any(|suffix| host.ends_with(suffix))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_writing_passes() {
        assert_eq!(private_content("Write a short email to the landlord about the heater."), None);
        assert_eq!(private_content("Search the Svelte 5 docs for $state and summarise it."), None);
        assert_eq!(private_content("The password must be at least 12 characters."), None);
    }

    #[test]
    fn secrets_env_and_internal_addresses_are_caught() {
        for text in [
            "use sk-proj-abcdefghijklmnopqrstuvwx",
            "DB_PASSWORD=hunter2222",
            "export OPENAI_API_KEY=abc",
            "$env:GITHUB_TOKEN = \"x\"",
            "Authorization: Bearer abcdefghijkl",
            "postgres://admin:pw@db/app",
            "the server is at 192.168.1.20",
            "call http://billing.internal:8080/api",
            "FOO=1\nBAR=2\nBAZ=3",
            "-----BEGIN RSA PRIVATE KEY-----",
        ] {
            assert!(private_content(text).is_some(), "{text} must be caught");
        }
    }

    #[test]
    fn files_that_hold_secrets() {
        for name in [".env", ".env.local", "prod.env", "id_rsa", "server.pem", "tls.key", "aws_credentials", "passwords.txt", ".npmrc", "vault.kdbx"] {
            assert!(sensitive_file(&format!("C:/work/{name}")), "{name}");
        }
        for name in ["notes.md", "README.md", "draft-email.txt", "keyboard.png"] {
            assert!(!sensitive_file(name), "{name}");
        }
    }

    #[test]
    fn internal_urls() {
        for url in ["http://localhost:3000", "http://127.0.0.1/", "http://10.0.0.5/x", "http://169.254.169.254/latest", "http://[::1]/", "http://nas/", "https://git.corp/x", "http://user:pw@192.168.0.1"] {
            assert!(internal_url(url), "{url}");
        }
        for url in ["https://svelte.dev/docs", "https://8.8.8.8/", "https://api.github.com/repos"] {
            assert!(!internal_url(url), "{url}");
        }
    }
}
