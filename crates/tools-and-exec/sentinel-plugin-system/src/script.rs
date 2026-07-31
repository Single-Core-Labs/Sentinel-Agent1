use async_trait::async_trait;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::plugin::{Plugin, PluginAction, PluginEvent, PluginHook, PluginManifest, PluginId};

/// On-disk manifest (`sentinel-plugin.toml`) for a packaged plugin.
#[derive(Debug, Clone, Deserialize)]
pub struct PluginFileManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub hooks: HookMap,
}

/// Script commands to run for each plugin event.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct HookMap {
    #[serde(default)]
    pub before_tool_call: Option<String>,
    #[serde(default)]
    pub after_tool_call: Option<String>,
    #[serde(default)]
    pub before_model_request: Option<String>,
    #[serde(default)]
    pub after_model_response: Option<String>,
    #[serde(default)]
    pub session_created: Option<String>,
    #[serde(default)]
    pub session_ended: Option<String>,
}

impl PluginFileManifest {
    pub fn load(dir: &Path) -> Result<Self, String> {
        let path = dir.join("sentinel-plugin.toml");
        let content = std::fs::read_to_string(&path)
            .map_err(|e| format!("cannot read {}: {}", path.display(), e))?;
        toml::from_str(&content)
            .map_err(|e| format!("invalid sentinel-plugin.toml in {}: {}", dir.display(), e))
    }
}

/// A plugin hook backed by an external script.
///
/// Contract: the script is invoked as `command <event_type> <tool_name>` with the
/// full event JSON on stdin. For `before_tool_call`, a stdout line of
/// `veto <reason>` (or `deny <reason>`) vetoes the tool call; `allow` continues.
/// All other events and outputs are treated as `Continue`.
pub struct ScriptHook {
    command: String,
    event: String,
    timeout: std::time::Duration,
}

impl ScriptHook {
    pub fn new(command: impl Into<String>, event: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            event: event.into(),
            timeout: std::time::Duration::from_secs(15),
        }
    }

    pub fn with_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

fn event_key(event: &PluginEvent) -> &'static str {
    match event {
        PluginEvent::BeforeToolCall { .. } => "before_tool_call",
        PluginEvent::AfterToolCall { .. } => "after_tool_call",
        PluginEvent::BeforeModelRequest { .. } => "before_model_request",
        PluginEvent::AfterModelResponse { .. } => "after_model_response",
        PluginEvent::SessionCreated { .. } => "session_created",
        PluginEvent::SessionEnded { .. } => "session_ended",
        PluginEvent::Custom { .. } => "custom",
    }
}

fn event_tool_name(event: &PluginEvent) -> String {
    match event {
        PluginEvent::BeforeToolCall { tool_name, .. } => tool_name.clone(),
        PluginEvent::AfterToolCall { tool_name, .. } => tool_name.clone(),
        _ => String::new(),
    }
}

#[async_trait]
impl PluginHook for ScriptHook {
    async fn handle(&self, event: &PluginEvent) -> PluginAction {
        if event_key(event) != self.event {
            return PluginAction::Continue;
        }
        let event_json = serde_json::to_string(event).unwrap_or_else(|_| "{}".into());

        #[cfg(target_os = "windows")]
        let mut cmd = {
            let mut c = tokio::process::Command::new("cmd");
            c.arg("/C").arg(format!(
                "{} {} {}",
                self.command,
                self.event,
                event_tool_name(event)
            ));
            c
        };
        #[cfg(not(target_os = "windows"))]
        let mut cmd = {
            let mut c = tokio::process::Command::new("sh");
            c.arg("-c").arg(format!(
                "{} {} {}",
                self.command,
                self.event,
                event_tool_name(event)
            ));
            c
        };

        cmd.stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null());

        let run = async {
            let mut child = cmd.spawn().ok()?;
            use tokio::io::AsyncWriteExt;
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(event_json.as_bytes()).await;
            }
            let output = child.wait_with_output().await.ok()?;
            Some(output)
        };
        match tokio::time::timeout(self.timeout, run).await {
            Ok(Some(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let line = stdout.lines().next().unwrap_or("").trim().to_lowercase();
                if line.starts_with("veto") || line.starts_with("deny") {
                    let reason = line
                        .split_once(|c: char| c.is_whitespace())
                        .map(|(_, r)| r.trim().to_string())
                        .filter(|r| !r.is_empty())
                        .unwrap_or_else(|| "vetoed by plugin script".into());
                    PluginAction::Veto(reason)
                } else {
                    PluginAction::Continue
                }
            }
            _ => PluginAction::Continue,
        }
    }
}

/// A plugin loaded from a plugin directory (manifest + script hooks).
pub struct ScriptPlugin {
    manifest: PluginManifest,
    hooks: std::sync::Mutex<Vec<Box<dyn PluginHook>>>,
}

#[async_trait]
impl Plugin for ScriptPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    fn hooks(&self) -> Vec<Box<dyn PluginHook>> {
        self.hooks.lock().unwrap_or_else(|e| e.into_inner()).drain(..).collect()
    }
}

/// Load a plugin from a directory containing `sentinel-plugin.toml`.
pub fn load_plugin_dir(dir: &Path) -> Result<Arc<dyn Plugin>, String> {
    let file = PluginFileManifest::load(dir)?;

    let resolve = |cmd: &Option<String>| -> Option<String> {
        cmd.as_ref().map(|c| {
            let p = PathBuf::from(c);
            if p.is_absolute() || p.exists() {
                c.clone()
            } else {
                dir.join(c).to_string_lossy().into_owned()
            }
        })
    };

    let mut hooks: Vec<Box<dyn PluginHook>> = Vec::new();
    for (event, cmd) in [
        ("before_tool_call", &file.hooks.before_tool_call),
        ("after_tool_call", &file.hooks.after_tool_call),
        ("before_model_request", &file.hooks.before_model_request),
        ("after_model_response", &file.hooks.after_model_response),
        ("session_created", &file.hooks.session_created),
        ("session_ended", &file.hooks.session_ended),
    ] {
        if let Some(cmd) = resolve(cmd) {
            hooks.push(Box::new(ScriptHook::new(cmd, event)));
        }
    }

    let manifest = PluginManifest {
        id: PluginId::new(&file.id),
        name: file.name,
        version: file.version,
        description: file.description,
        author: file.author,
        homepage: file.homepage,
    };

    Ok(Arc::new(ScriptPlugin { manifest, hooks: std::sync::Mutex::new(hooks) }))
}

/// Discover all plugins inside a plugins directory (one subdir per plugin).
pub fn load_plugins_dir(dir: &Path) -> Vec<Arc<dyn Plugin>> {
    let mut plugins = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return plugins;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() && path.join("sentinel-plugin.toml").exists() {
            match load_plugin_dir(&path) {
                Ok(p) => plugins.push(p),
                Err(e) => tracing::warn!("skipping plugin in {}: {}", path.display(), e),
            }
        }
    }
    plugins
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_plugin_dir(manifest: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("sentinel-plugin-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("sentinel-plugin.toml"), manifest).unwrap();
        dir
    }

    #[tokio::test]
    async fn load_plugin_dir_parses_manifest_and_hooks() {
        let dir = temp_plugin_dir(
            "id = \"guard\"\n\
             name = \"Guard\"\n\
             version = \"0.1.0\"\n\
             description = \"test\"\n\n\
             [hooks]\n\
             before_tool_call = \"echo veto only write\"\n",
        );
        let plugin = load_plugin_dir(&dir).unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(plugin.manifest().id.to_string(), "guard");
        assert_eq!(plugin.manifest().version, "0.1.0");
        assert_eq!(plugin.hooks().len(), 1);
    }

    #[tokio::test]
    async fn script_hook_vetoes_before_tool_call() {
        let hook = ScriptHook::new("echo veto no risky writes", "before_tool_call");
        let action = hook
            .handle(&PluginEvent::BeforeToolCall {
                tool_name: "write".into(),
                args: serde_json::json!({"file_path": "x"}),
            })
            .await;
        match action {
            PluginAction::Veto(reason) => assert!(reason.contains("no risky writes")),
            other => panic!("expected Veto, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn script_hook_continues_on_allow_or_other_events() {
        let hook = ScriptHook::new("echo allow", "before_tool_call");
        let action = hook
            .handle(&PluginEvent::BeforeToolCall {
                tool_name: "read".into(),
                args: serde_json::json!({}),
            })
            .await;
        assert!(matches!(action, PluginAction::Continue));

        // A hook for another event type must not fire.
        let other = hook
            .handle(&PluginEvent::SessionCreated { session_id: "s".into() })
            .await;
        assert!(matches!(other, PluginAction::Continue));
    }

    #[tokio::test]
    async fn load_plugins_dir_skips_non_plugin_dirs() {
        let base = std::env::temp_dir().join(format!("sentinel-plugins-root-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("not-a-plugin")).unwrap();
        std::fs::write(
            base.join("not-a-plugin/readme.txt"),
            "no manifest here",
        ).unwrap();
        std::fs::create_dir_all(base.join("real")).unwrap();
        std::fs::write(
            base.join("real/sentinel-plugin.toml"),
            "id = \"real\"\nname = \"Real\"\nversion = \"1.0.0\"\ndescription = \"d\"\n",
        ).unwrap();

        let plugins = load_plugins_dir(&base);
        let _ = std::fs::remove_dir_all(&base);

        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].manifest().id.to_string(), "real");
    }
}
