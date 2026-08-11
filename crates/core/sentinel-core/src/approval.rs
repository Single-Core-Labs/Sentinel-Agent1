use crate::agent::{ApprovalDecision, ApprovalGate};
use crate::thread::ApprovalRequest;
use sentinel_config::SentinelConfig;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub enum PermissionLevel {
    Allow,
    #[default]
    Ask,
    Deny,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRule {
    pub pattern: String,
    pub level: PermissionLevel,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PermissionRuleset {
    pub rules: Vec<PermissionRule>,
    /// Level applied when no rule matches (`Ask` unless configured otherwise).
    #[serde(default)]
    pub default_level: PermissionLevel,
}

impl PermissionRuleset {
    pub fn new(rules: Vec<PermissionRule>) -> Self {
        Self {
            rules,
            default_level: PermissionLevel::Ask,
        }
    }

    /// First rule whose pattern matches `tool_name` (rules are checked in
    /// declaration order — first match wins).
    pub fn rule_for(&self, tool_name: &str) -> Option<&PermissionRule> {
        self.rules
            .iter()
            .find(|rule| glob_match(&rule.pattern, tool_name))
    }

    pub fn evaluate(&self, tool_name: &str) -> PermissionLevel {
        match self.rule_for(tool_name) {
            Some(rule) => rule.level.clone(),
            None => self.default_level.clone(),
        }
    }

    /// Build a ruleset from `[permissions]` config settings (sentinel-config).
    /// Unknown levels map to `Ask`.
    pub fn from_config(settings: &sentinel_config::PermissionSettings) -> Self {
        let rules = settings
            .rules
            .iter()
            .map(|r| PermissionRule {
                pattern: r.pattern.clone(),
                level: match r.level.as_str() {
                    "allow" => PermissionLevel::Allow,
                    "deny" => PermissionLevel::Deny,
                    _ => PermissionLevel::Ask,
                },
                reason: r.reason.clone(),
            })
            .collect();
        let default_level = match settings.default_level.as_deref() {
            Some("allow") => PermissionLevel::Allow,
            Some("deny") => PermissionLevel::Deny,
            _ => PermissionLevel::Ask,
        };
        Self {
            rules,
            default_level,
        }
    }
}

fn glob_match(pattern: &str, name: &str) -> bool {
    if pattern == "*" || pattern == name {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix('*') {
        return name.starts_with(prefix);
    }
    if let Some(suffix) = pattern.strip_prefix('*') {
        return name.ends_with(suffix);
    }
    false
}


/// Approval gate that consults a [`PermissionRuleset`] before delegating to an
/// inner gate:
///
/// - matching `allow` rule  → `Approved` (never prompts, works in yolo mode)
/// - matching `deny` rule   → `Rejected` (always blocked, works in yolo mode)
/// - matching `ask` rule or no match → delegated to `inner` (e.g. the CLI
///   interactive prompt or `AutoApprovalGate`).
pub struct RulesetApprovalGate {
    inner: Box<dyn ApprovalGate>,
    ruleset: PermissionRuleset,
}

impl std::fmt::Debug for RulesetApprovalGate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RulesetApprovalGate")
            .field("rules", &self.ruleset.rules)
            .finish_non_exhaustive()
    }
}

impl RulesetApprovalGate {
    pub fn new(inner: Box<dyn ApprovalGate>, ruleset: PermissionRuleset) -> Self {
        Self { inner, ruleset }
    }

    pub fn ruleset(&self) -> &PermissionRuleset {
        &self.ruleset
    }
}

#[async_trait::async_trait]
impl ApprovalGate for RulesetApprovalGate {
    async fn request_approval(&self, req: &ApprovalRequest) -> ApprovalDecision {
        match self.ruleset.evaluate(&req.tool_name) {
            PermissionLevel::Allow => ApprovalDecision::Approved,
            PermissionLevel::Deny => {
                let reason = self
                    .ruleset
                    .rule_for(&req.tool_name)
                    .and_then(|r| r.reason.clone())
                    .unwrap_or_else(|| {
                        format!("Tool '{}' is denied by permission ruleset", req.tool_name)
                    });
                ApprovalDecision::Rejected(reason)
            }
            PermissionLevel::Ask => self.inner.request_approval(req).await,
        }
    }
}

/// Build the permission gate for a config: wraps `inner` in a
/// [`RulesetApprovalGate`] when permission rules or a default level are
/// configured, otherwise returns `inner` unchanged (previous behavior).
pub fn permissions_gate_for(
    config: &SentinelConfig,
    inner: Box<dyn ApprovalGate>,
) -> Box<dyn ApprovalGate> {
    let settings = &config.permissions;
    if settings.rules.is_empty() && settings.default_level.is_none() {
        return inner;
    }
    Box::new(RulesetApprovalGate::new(
        inner,
        PermissionRuleset::from_config(settings),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_glob_match_exact() {
        assert!(glob_match("read", "read"));
        assert!(!glob_match("read", "write"));
    }

    #[test]
    fn test_permission_glob_match_wildcard() {
        assert!(glob_match("*", "anything"));
        assert!(glob_match("git_*", "git_status"));
        assert!(glob_match("git_*", "git_commit"));
        assert!(!glob_match("git_*", "bash"));
    }

    #[test]
    fn test_permission_glob_match_suffix() {
        assert!(glob_match("*_tool", "bash_tool"));
        assert!(!glob_match("*_tool", "bash"));
    }

    #[test]
    fn test_ruleset_evaluate_default_is_ask() {
        let rs = PermissionRuleset::default();
        assert!(matches!(rs.evaluate("read"), PermissionLevel::Ask));
    }

    #[test]
    fn test_ruleset_evaluate_allow() {
        let rs = PermissionRuleset::new(vec![
            PermissionRule {
                pattern: "read".into(),
                level: PermissionLevel::Allow,
                reason: None,
            },
            PermissionRule {
                pattern: "write".into(),
                level: PermissionLevel::Deny,
                reason: Some("dangerous".into()),
            },
        ]);
        assert!(matches!(rs.evaluate("read"), PermissionLevel::Allow));
        assert!(matches!(rs.evaluate("write"), PermissionLevel::Deny));
    }

    #[test]
    fn test_ruleset_first_match_wins() {
        let rs = PermissionRuleset::new(vec![
            PermissionRule {
                pattern: "*".into(),
                level: PermissionLevel::Deny,
                reason: None,
            },
            PermissionRule {
                pattern: "read".into(),
                level: PermissionLevel::Allow,
                reason: None,
            },
        ]);
        assert!(
            matches!(rs.evaluate("read"), PermissionLevel::Deny),
            "first matching rule must win"
        );
    }

    #[test]
    fn test_ruleset_from_config() {
        let settings = sentinel_config::PermissionSettings {
            default_level: Some("deny".into()),
            rules: vec![
                sentinel_config::PermissionRuleConfig {
                    pattern: "read".into(),
                    level: "allow".into(),
                    reason: None,
                },
                sentinel_config::PermissionRuleConfig {
                    pattern: "bash".into(),
                    level: "deny".into(),
                    reason: Some("no shell".into()),
                },
                sentinel_config::PermissionRuleConfig {
                    pattern: "web_*".into(),
                    level: "unknown-level".into(),
                    reason: None,
                },
            ],
        };
        let rs = PermissionRuleset::from_config(&settings);
        assert!(matches!(rs.evaluate("read"), PermissionLevel::Allow));
        assert!(matches!(rs.evaluate("bash"), PermissionLevel::Deny));
        assert!(
            matches!(rs.evaluate("web_fetch"), PermissionLevel::Ask),
            "unknown level must map to Ask"
        );
        assert!(
            matches!(rs.evaluate("write"), PermissionLevel::Deny),
            "default_level must apply to unmatched tools"
        );
    }

    #[tokio::test]
    async fn test_permissions_gate_for_passthrough_when_unconfigured() {
        let config = SentinelConfig::default();
        let gate = permissions_gate_for(&config, Box::new(AutoApprovalGateStub));
        let decision = gate
            .request_approval(&ApprovalRequest {
                tool_name: "write".into(),
                args: serde_json::json!({}),
                prompt: "run".into(),
                diff: None,
                estimated_cost: None,
            })
            .await;
        assert!(
            matches!(decision, ApprovalDecision::Approved),
            "no rules configured -> inner gate behavior unchanged"
        );
    }

    #[tokio::test]
    async fn test_permissions_gate_for_wraps_when_configured() {
        let mut config = SentinelConfig::default();
        config.permissions.default_level = Some("deny".into());
        let gate = permissions_gate_for(&config, Box::new(AutoApprovalGateStub));
        let decision = gate
            .request_approval(&ApprovalRequest {
                tool_name: "write".into(),
                args: serde_json::json!({}),
                prompt: "run".into(),
                diff: None,
                estimated_cost: None,
            })
            .await;
        assert!(
            matches!(decision, ApprovalDecision::Rejected(_)),
            "configured default deny must block even with auto-approving inner gate"
        );
    }

    struct AutoApprovalGateStub;

    #[async_trait::async_trait]
    impl ApprovalGate for AutoApprovalGateStub {
        async fn request_approval(&self, _: &ApprovalRequest) -> ApprovalDecision {
            ApprovalDecision::Approved
        }
    }

}
