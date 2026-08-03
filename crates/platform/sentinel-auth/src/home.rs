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

    #[test]
    fn test_sentinel_home_dir_uses_sentinel_home_env() {
        std::env::set_var("SENTINEL_HOME", "/custom/path");
        assert_eq!(sentinel_home_dir(), PathBuf::from("/custom/path"));
        std::env::remove_var("SENTINEL_HOME");
    }

    #[test]
    fn test_auth_file_path_includes_auth_json() {
        std::env::set_var("SENTINEL_HOME", "/tmp/test");
        let path = auth_file_path();
        assert!(path.ends_with("auth.json"));
        std::env::remove_var("SENTINEL_HOME");
    }
}
