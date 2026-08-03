use std::path::PathBuf;

pub fn sentinel_home_dir() -> PathBuf {
    if let Ok(home) = std::env::var("SENTINEL_HOME") {
        return PathBuf::from(home);
    }
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(|h| PathBuf::from(h).join(".sentinel"))
        .unwrap_or_else(|_| PathBuf::from(".sentinel"))
}

pub fn auth_file_path() -> PathBuf {
    sentinel_home_dir().join("auth.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    lazy_static::lazy_static! {
        static ref ENV_LOCK: Mutex<()> = Mutex::new(());
    }

    #[test]
    fn test_sentinel_home_dir_uses_sentinel_home_env() {
        let _guard = ENV_LOCK.lock().unwrap();
        let old_home = std::env::var("SENTINEL_HOME").ok();
        std::env::set_var("SENTINEL_HOME", "/custom/path");
        assert_eq!(sentinel_home_dir(), PathBuf::from("/custom/path"));
        // Restore
        if let Some(val) = old_home {
            std::env::set_var("SENTINEL_HOME", val);
        } else {
            std::env::remove_var("SENTINEL_HOME");
        }
    }

    #[test]
    fn test_auth_file_path_includes_auth_json() {
        let _guard = ENV_LOCK.lock().unwrap();
        let old_home = std::env::var("SENTINEL_HOME").ok();
        std::env::set_var("SENTINEL_HOME", "/tmp/test");
        let path = auth_file_path();
        assert!(path.ends_with("auth.json"));
        // Restore
        if let Some(val) = old_home {
            std::env::set_var("SENTINEL_HOME", val);
        } else {
            std::env::remove_var("SENTINEL_HOME");
        }
    }
}
