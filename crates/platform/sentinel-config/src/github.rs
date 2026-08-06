//! GitHub token retrieval for provider discovery.
//!
//! Mirrors opencode's `LoadGitHubToken`: the `GITHUB_TOKEN` environment
//! variable is checked first, then the GitHub Copilot configuration file
//! (`hosts.json`) is consulted. This lets a user who never set an explicit
//! environment variable still unlock providers that authenticate with a
//! GitHub token (e.g. a Copilot-style endpoint).

use std::path::{Path, PathBuf};

/// Environment variable that holds the GitHub token.
pub const GITHUB_TOKEN_ENV: &str = "GITHUB_TOKEN";

/// Resolve a GitHub token: `GITHUB_TOKEN` env var first, then the Copilot
/// `hosts.json` file. Returns `None` when neither source yields a token.
pub fn load_github_token(get_env: &impl Fn(&str) -> Option<String>) -> Option<String> {
    if let Some(t) = get_env(GITHUB_TOKEN_ENV) {
        if !t.trim().is_empty() {
            return Some(t);
        }
    }
    github_hosts_token(copilot_hosts_path().as_deref())
}

/// Location of the GitHub Copilot configuration file:
/// `%APPDATA%/github-copilot/hosts.json` on Windows, else
/// `~/.config/github-copilot/hosts.json`.
pub fn copilot_hosts_path() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return Some(PathBuf::from(appdata).join("github-copilot").join("hosts.json"));
        }
    }
    #[cfg(not(windows))]
    {
        if let Some(home) = std::env::var_os("HOME") {
            return Some(PathBuf::from(home).join(".config").join("github-copilot").join("hosts.json"));
        }
    }
    None
}

/// Read the `oauth_token` for `github.com` from a Copilot `hosts.json`:
///
/// ```json
/// { "github.com": { "oauth_token": "gho_…" } }
/// ```
///
/// Returns `None` when the file is missing, malformed, or lacks the entry.
pub fn github_hosts_token(path: Option<&Path>) -> Option<String> {
    let path = path?;
    let content = std::fs::read_to_string(path).ok()?;
    let hosts: serde_json::Value = serde_json::from_str(&content).ok()?;
    hosts
        .get("github.com")?
        .get("oauth_token")?
        .as_str()
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_env(_: &str) -> Option<String> {
        None
    }

    fn env_of<'a>(k: &'a str, v: &'a str) -> impl Fn(&str) -> Option<String> + 'a {
        move |key| if key == k { Some(v.to_string()) } else { None }
    }

    fn temp_hosts(content: &str) -> String {
        let path = std::env::temp_dir().join(format!(
            "sentinel-github-test-{}-{}.json",
            std::process::id(),
            content.len()
        ));
        std::fs::write(&path, content).unwrap();
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn env_token_wins() {
        let env = env_of(GITHUB_TOKEN_ENV, "gho_env");
        assert_eq!(load_github_token(&env).as_deref(), Some("gho_env"));
    }

    #[test]
    fn hosts_file_token_when_env_missing() {
        let hosts = temp_hosts(
            r#"{"github.com":{"oauth_token":"gho_file","expires_at":0}}"#,
        );
        let token = github_hosts_token(Some(Path::new(&hosts)));
        let _ = std::fs::remove_file(&hosts);
        assert_eq!(token.as_deref(), Some("gho_file"));
    }

    #[test]
    fn missing_sources_yield_none() {
        assert_eq!(load_github_token(&empty_env), None);
        assert_eq!(github_hosts_token(None), None);
    }

    #[test]
    fn malformed_hosts_yields_none() {
        let hosts = temp_hosts("not json at all");
        let token = github_hosts_token(Some(Path::new(&hosts)));
        let _ = std::fs::remove_file(&hosts);
        assert_eq!(token, None);

        let hosts = temp_hosts(r#"{"github.com":{"oauth_token":42}}"#);
        let token = github_hosts_token(Some(Path::new(&hosts)));
        let _ = std::fs::remove_file(&hosts);
        assert_eq!(token, None);
    }
}
