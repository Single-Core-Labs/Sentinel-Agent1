use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::warn;

use crate::plugin::{Plugin, PluginId, PluginEvent, PluginAction, PluginHook};

/// Registry of loaded plugins and their hooks.
pub struct PluginRegistry {
    plugins: RwLock<HashMap<PluginId, Arc<dyn Plugin>>>,
    hooks: RwLock<Vec<(PluginId, Box<dyn PluginHook>)>>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            plugins: RwLock::new(HashMap::new()),
            hooks: RwLock::new(Vec::new()),
        }
    }

    /// Register a plugin and its hooks.
    pub async fn register(&self, plugin: Arc<dyn Plugin>) -> Result<(), anyhow::Error> {
        let manifest = plugin.manifest();
        let id = manifest.id.clone();

        plugin.init().await?;
        let hooks = plugin.hooks();

        let mut plugins = self.plugins.write().await;
        plugins.insert(id.clone(), plugin);

        let mut hooks_lock = self.hooks.write().await;
        for hook in hooks {
            hooks_lock.push((id.clone(), hook));
        }

        tracing::info!(plugin_id = %id, "plugin registered");
        Ok(())
    }

    /// Unregister a plugin by ID.
    pub async fn unregister(&self, id: &PluginId) -> Result<(), anyhow::Error> {
        let mut plugins = self.plugins.write().await;
        if let Some(plugin) = plugins.remove(id) {
            plugin.shutdown().await?;
        }

        let mut hooks_lock = self.hooks.write().await;
        hooks_lock.retain(|(pid, _)| pid != id);

        tracing::info!(plugin_id = %id, "plugin unregistered");
        Ok(())
    }

    /// Dispatch an event to all registered hooks.
    /// Returns the first veto action found, or the last modification.
    pub async fn dispatch(&self, event: &PluginEvent) -> PluginAction {
        let hooks = self.hooks.read().await;
        let mut last_action = PluginAction::Continue;

        for (_id, hook) in hooks.iter() {
            match hook.handle(event).await {
                PluginAction::Continue => {}
                PluginAction::Veto(reason) => {
                    warn!(reason = %reason, "plugin vetoed operation");
                    return PluginAction::Veto(reason);
                }
                PluginAction::Modify(value) => {
                    last_action = PluginAction::Modify(value);
                }
            }
        }

        last_action
    }

    /// List all registered plugin IDs.
    pub async fn list_plugins(&self) -> Vec<PluginId> {
        self.plugins.read().await.keys().cloned().collect()
    }

    /// Number of registered plugins.
    pub async fn plugin_count(&self) -> usize {
        self.plugins.read().await.len()
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::{PluginManifest, PluginId};
    use async_trait::async_trait;

    use std::sync::OnceLock;

    fn test_manifest() -> &'static PluginManifest {
        static MANIFEST: OnceLock<PluginManifest> = OnceLock::new();
        MANIFEST.get_or_init(|| PluginManifest {
            id: PluginId::new("test-plugin"),
            name: "Test".into(),
            version: "0.1.0".into(),
            description: "test".into(),
            author: None,
            homepage: None,
        })
    }

    struct TestPlugin;

    #[async_trait]
    impl Plugin for TestPlugin {
        fn manifest(&self) -> &PluginManifest {
            test_manifest()
        }
    }

    #[tokio::test]
    async fn test_register_and_list() {
        let registry = PluginRegistry::new();
        let plugin = Arc::new(TestPlugin);
        registry.register(plugin).await.unwrap();

        let plugins = registry.list_plugins().await;
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].to_string(), "test-plugin");
    }

    #[tokio::test]
    async fn test_dispatch_no_hooks() {
        let registry = PluginRegistry::new();
        let event = PluginEvent::SessionCreated { session_id: "test".into() };
        let action = registry.dispatch(&event).await;
        assert!(matches!(action, PluginAction::Continue));
    }

    #[tokio::test]
    async fn test_unregister() {
        let registry = PluginRegistry::new();
        let plugin = Arc::new(TestPlugin);
        registry.register(plugin).await.unwrap();
        assert_eq!(registry.plugin_count().await, 1);

        registry.unregister(&PluginId::new("test-plugin")).await.unwrap();
        assert_eq!(registry.plugin_count().await, 0);
    }
}
