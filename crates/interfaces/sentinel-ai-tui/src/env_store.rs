use std::path::{Path, PathBuf};

/// Read the `.env` file in the current directory (or under `$SENTINEL_HOME`),
/// set/replace `key=value` idempotently, and write it back.
/// Returns the path of the file that was written, or a descriptive error.
pub fn write_env_key(key: &str, value: &str) -> Result<PathBuf, String> {
    let path = env_path();
    let mut out: Vec<String> = Vec::new();
    if path.exists() {
        if let Ok(contents) = std::fs::read_to_string(&path) {
            for line in contents.lines() {
                // Drop any previously-set value for this key (preserve blanks/comments).
                let trimmed = line.trim();
                let is_assignment = !trimmed.starts_with('#')
                    && trimmed.split_once('=').is_some_and(|(k, _)| k.trim() == key);
                if !is_assignment {
                    out.push(line.to_string());
                }
            }
        }
    }
    out.push(format!("{}={}", key, value));

    std::fs::write(&path, out.join("\n") + "\n")
        .map_err(|e| format!("Failed to write {}: {}", path.display(), e))?;
    Ok(path)
}

/// Load `key=value` pairs from the `.env` file into the process environment,
/// without overriding already-set variables. Returns the number of vars set.
pub fn load_env() -> usize {
    let path = env_path();
    if !path.exists() {
        return 0;
    }
    let Ok(contents) = std::fs::read_to_string(&path) else { return 0 };
    let mut count = 0;
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else { continue };
        let key = key.trim();
        let value = value.trim();
        if key.is_empty() || value.is_empty() {
            continue;
        }
        if std::env::var_os(key).is_none() {
            std::env::set_var(key, value);
            count += 1;
        }
    }
    count
}

/// The `.env` path used by the CLI: `$SENTINEL_HOME/.env` if set, else `./.env`.
fn env_path() -> PathBuf {
    if let Ok(home) = std::env::var("SENTINEL_HOME") {
        let home = Path::new(&home);
        if home.is_dir() {
            return home.join(".env");
        }
    }
    PathBuf::from(".env")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::Mutex;

    // Tests mutate process-wide env vars (SENTINEL_HOME), so they must run serially.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn test_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join("sentinel").join(name);
        let _ = fs::create_dir_all(&dir);
        dir
    }

    #[test]
    fn writes_and_updates_env_key() {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = test_dir(&format!("env-write-{}", std::process::id()));
        std::env::set_var("SENTINEL_HOME", &dir);
        let path = dir.join(".env");

        write_env_key("GOOGLE_API_KEY", "sk-first").expect("first write");
        assert_eq!(fs::read_to_string(&path).unwrap(), "GOOGLE_API_KEY=sk-first\n");

        write_env_key("GOOGLE_API_KEY", "sk-second").expect("update");
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "GOOGLE_API_KEY=sk-second\n",
            "old value must be replaced"
        );

        write_env_key("ANTHROPIC_API_KEY", "sk-other").expect("append");
        let contents = fs::read_to_string(&path).unwrap();
        assert!(contents.contains("GOOGLE_API_KEY=sk-second"), "keep unrelated value");
        assert!(contents.contains("ANTHROPIC_API_KEY=sk-other"), "append new key");

        std::env::remove_var("SENTINEL_HOME");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn loads_env_into_process() {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = test_dir(&format!("env-load-{}", std::process::id()));
        std::env::set_var("SENTINEL_HOME", &dir);
        std::env::remove_var("SENTINEL_LOAD_TEST_KEY");
        let path = dir.join(".env");
        fs::write(&path, "SENTINEL_LOAD_TEST_KEY=abc123\n# comment\nblank=  \n\n").unwrap();

        let count = load_env();
        assert_eq!(count, 1, "only the non-blank, non-comment key is set");
        assert_eq!(std::env::var("SENTINEL_LOAD_TEST_KEY").unwrap(), "abc123");

        std::env::remove_var("SENTINEL_LOAD_TEST_KEY");
        std::env::remove_var("SENTINEL_HOME");
        let _ = fs::remove_dir_all(&dir);
    }
}