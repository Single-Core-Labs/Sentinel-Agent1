use std::sync::Arc;
use async_trait::async_trait;

use crate::plugin::{Plugin, PluginEvent, PluginAction, PluginHook, PluginManifest, PluginId};

/// A built-in hook implementation that wraps a closure.
pub struct FnHook {
    handler: Box<dyn Fn(&PluginEvent) -> PluginAction + Send + Sync>,
}

impl FnHook {
    pub fn new(handler: Box<dyn Fn(&PluginEvent) -> PluginAction + Send + Sync>) -> Self {
        Self { handler }
    }
}

#[async_trait]
impl PluginHook for FnHook {
    async fn handle(&self, event: &PluginEvent) -> PluginAction {
        (self.handler)(event)
    }
}

/// Convenience builder for creating plugins programmatically.
pub struct PluginBuilder {
    manifest: PluginManifest,
    hooks: Vec<Box<dyn PluginHook>>,
}

impl PluginBuilder {
    pub fn new(id: impl Into<String>, name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            manifest: PluginManifest {
                id: PluginId::new(id),
                name: name.into(),
                version: version.into(),
                description: String::new(),
                author: None,
                homepage: None,
            },
            hooks: Vec::new(),
        }
    }

    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.manifest.description = desc.into();
        self
    }

    pub fn author(mut self, author: impl Into<String>) -> Self {
        self.manifest.author = Some(author.into());
        self
    }

    pub fn on(mut self, hook: Box<dyn PluginHook>) -> Self {
        self.hooks.push(hook);
        self
    }

    pub fn on_event(mut self, handler: Box<dyn Fn(&PluginEvent) -> PluginAction + Send + Sync>) -> Self {
        self.hooks.push(Box::new(FnHook::new(handler)));
        self
    }

    /// Build into a boxed plugin.
    pub fn build(self) -> Arc<dyn Plugin> {
        let manifest = self.manifest;
        Arc::new(BuiltinPlugin { manifest })
    }

    /// Return the hooks collected during building (for manual registration).
    pub fn into_hooks(self) -> Vec<Box<dyn PluginHook>> {
        self.hooks
    }
}

struct BuiltinPlugin {
    manifest: PluginManifest,
}

#[async_trait]
impl Plugin for BuiltinPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    fn hooks(&self) -> Vec<Box<dyn PluginHook>> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_plugin_builder() {
        let plugin = PluginBuilder::new("my-plugin", "My Plugin", "1.0.0")
            .description("A test plugin")
            .author("test@example.com")
            .on_event(Box::new(|event| {
                match event {
                    PluginEvent::SessionCreated { .. } => {
                        PluginAction::Veto("no new sessions".into())
                    }
                    _ => PluginAction::Continue,
                }
            }))
            .build();

        assert_eq!(plugin.manifest().id.to_string(), "my-plugin");
        assert_eq!(plugin.manifest().name, "My Plugin");
        assert_eq!(plugin.manifest().version, "1.0.0");
    }
}
