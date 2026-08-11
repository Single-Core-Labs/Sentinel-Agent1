pub mod host;
pub mod plugin;
pub mod registry;
pub mod script;

pub use host::*;
pub use plugin::*;
pub use registry::*;
pub use script::*;

/// Resolve the default plugin directory:
/// `$SENTINEL_HOME/plugins` when set, else `~/.sentinel/plugins`.
/// Shared by the CLI entry points and the app server so every agent
/// construction path loads the same guard plugins.
pub fn default_plugins_dir() -> std::path::PathBuf {
    if let Ok(home) = std::env::var("SENTINEL_HOME") {
        return std::path::PathBuf::from(home).join("plugins");
    }
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(|h| {
            std::path::PathBuf::from(h)
                .join(".sentinel")
                .join("plugins")
        })
        .unwrap_or_else(|_| std::path::PathBuf::from("plugins"))
}

/// Load every plugin from [`default_plugins_dir`] into `registry`.
/// Returns `(loaded_count, failure_messages)`.
pub async fn load_default_plugins(registry: &PluginRegistry) -> (usize, Vec<String>) {
    let dir = default_plugins_dir();
    if !dir.exists() {
        let _ = std::fs::create_dir_all(&dir);
    }
    let mut loaded = 0usize;
    let mut failures = Vec::new();
    for plugin in load_plugins_dir(&dir) {
        match registry.register(plugin).await {
            Ok(_) => loaded += 1,
            Err(e) => failures.push(e.to_string()),
        }
    }
    (loaded, failures)
}
