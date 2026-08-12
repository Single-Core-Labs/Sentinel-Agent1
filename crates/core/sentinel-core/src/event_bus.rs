#[derive(Debug, Clone, PartialEq)]
pub enum PolicyDecision {
    Allow,
    Deny(String),
    PromptUser,
}

/// Policy engine for tool call decisions.
#[async_trait::async_trait]
pub trait PolicyEngine: Send + Sync {
    /// Evaluate a tool call and return a decision.
    async fn evaluate(&self, tool_name: &str, args: &serde_json::Value) -> PolicyDecision;
}

/// Policy engine that allows everything.
pub struct AllowAllPolicy;

#[async_trait::async_trait]
impl PolicyEngine for AllowAllPolicy {
    async fn evaluate(&self, _tool_name: &str, _args: &serde_json::Value) -> PolicyDecision {
        PolicyDecision::Allow
    }
}

/// Policy engine that denies mutating tools by default.
pub struct SafePolicy;

#[async_trait::async_trait]
impl PolicyEngine for SafePolicy {
    async fn evaluate(&self, tool_name: &str, _args: &serde_json::Value) -> PolicyDecision {
        match tool_name {
            "write" | "edit" | "bash" | "git_commit" | "github" => PolicyDecision::PromptUser,
            _ => PolicyDecision::Allow,
        }
    }
}

/// Policy engine that delegates every tool call decision to an external script.
///
/// Contract: the script is invoked as configured; the tool name and the JSON
/// arguments are piped to its stdin (tool name on line 1, JSON args on line 2).
/// The script must print a single verdict line on stdout:
///   - `allow`             → Allow
///   - `deny <reason>`     → Deny(reason)
///   - `ask` or `prompt`   → PromptUser (fall through to user approval)
///
/// Anything else, an error, or a timeout is treated as `Deny` (fail-closed).
/// The default timeout is 15 seconds.
pub struct ScriptPolicyEngine {
    command: String,
    timeout: std::time::Duration,
}

impl ScriptPolicyEngine {
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            timeout: std::time::Duration::from_secs(15),
        }
    }

    pub fn with_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

#[async_trait::async_trait]
impl PolicyEngine for ScriptPolicyEngine {
    async fn evaluate(&self, tool_name: &str, args: &serde_json::Value) -> PolicyDecision {
        let args_json = serde_json::to_string(args).unwrap_or_else(|_| "{}".into());

        #[cfg(target_os = "windows")]
        let mut cmd = {
            let mut c = tokio::process::Command::new("cmd");
            c.arg("/C").arg(&self.command);
            c
        };
        #[cfg(not(target_os = "windows"))]
        let mut cmd = {
            let mut c = tokio::process::Command::new("sh");
            c.arg("-c").arg(&self.command);
            c
        };

        cmd.stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null());

        let run = async {
            let mut child = cmd.spawn().ok()?;
            use tokio::io::AsyncWriteExt;
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin
                    .write_all(format!("{}\n{}\n", tool_name, args_json).as_bytes())
                    .await;
            }
            let output = child.wait_with_output().await.ok()?;
            Some(output)
        };

        let result = tokio::time::timeout(self.timeout, run).await;
        match result {
            Ok(Some(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let line = stdout.lines().next().unwrap_or("").trim().to_lowercase();
                if line == "allow" {
                    PolicyDecision::Allow
                } else if let Some(reason) = line.strip_prefix("deny") {
                    let reason = reason.trim().trim_start_matches(':').trim().to_string();
                    PolicyDecision::Deny(if reason.is_empty() {
                        "denied by policy script".into()
                    } else {
                        reason
                    })
                } else if line == "ask" || line == "prompt" {
                    PolicyDecision::PromptUser
                } else {
                    PolicyDecision::Deny(format!(
                        "policy script returned unrecognized verdict '{}'",
                        line
                    ))
                }
            }
            Ok(None) => PolicyDecision::Deny("policy script failed to run".into()),
            Err(_) => {
                PolicyDecision::Deny(format!("policy script timed out after {:?}", self.timeout))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allow_all_policy() {
        let policy = AllowAllPolicy;
        let decision = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(policy.evaluate("write", &serde_json::json!({})));
        assert_eq!(decision, PolicyDecision::Allow);
    }

    #[test]
    fn test_safe_policy() {
        let policy = SafePolicy;
        let rt = tokio::runtime::Runtime::new().unwrap();
        assert_eq!(
            rt.block_on(policy.evaluate("read", &serde_json::json!({}))),
            PolicyDecision::Allow
        );
        assert_eq!(
            rt.block_on(policy.evaluate("write", &serde_json::json!({}))),
            PolicyDecision::PromptUser
        );
    }

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Runtime::new().unwrap()
    }

    fn echo_policy(verdict: &str) -> ScriptPolicyEngine {
        ScriptPolicyEngine::new(format!("echo {}", verdict))
    }

    #[test]
    fn script_policy_allow() {
        let policy = echo_policy("allow");
        assert_eq!(
            rt().block_on(policy.evaluate("read", &serde_json::json!({"path": "x"}))),
            PolicyDecision::Allow
        );
    }

    #[test]
    fn script_policy_deny_with_reason() {
        let policy = echo_policy("deny no write access");
        match rt().block_on(policy.evaluate("write", &serde_json::json!({}))) {
            PolicyDecision::Deny(reason) => assert!(reason.contains("no write access")),
            other => panic!("expected Deny, got {:?}", other),
        }
    }

    #[test]
    fn script_policy_prompt_user() {
        let policy = echo_policy("ask");
        assert_eq!(
            rt().block_on(policy.evaluate("write", &serde_json::json!({}))),
            PolicyDecision::PromptUser
        );
    }

    #[test]
    fn script_policy_fail_closed_on_garbage() {
        let policy = echo_policy("maybe");
        match rt().block_on(policy.evaluate("read", &serde_json::json!({}))) {
            PolicyDecision::Deny(reason) => assert!(reason.contains("unrecognized")),
            other => panic!("expected fail-closed Deny, got {:?}", other),
        }
    }

    #[test]
    fn script_policy_fail_closed_on_missing_command() {
        let policy = ScriptPolicyEngine::new("definitely-not-a-real-command-xyz")
            .with_timeout(std::time::Duration::from_secs(5));
        match rt().block_on(policy.evaluate("read", &serde_json::json!({}))) {
            PolicyDecision::Deny(_) => {}
            other => panic!("expected fail-closed Deny, got {:?}", other),
        }
    }
}
