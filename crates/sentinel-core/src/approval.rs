use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PermissionLevel {
    Allow,
    Ask,
    Deny,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRule {
    pub pattern: String,
    pub level: PermissionLevel,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRuleset {
    pub rules: Vec<PermissionRule>,
}

impl Default for PermissionRuleset {
    fn default() -> Self {
        Self { rules: Vec::new() }
    }
}

impl PermissionRuleset {
    pub fn new(rules: Vec<PermissionRule>) -> Self {
        Self { rules }
    }

    pub fn evaluate(&self, tool_name: &str) -> PermissionLevel {
        for rule in &self.rules {
            if glob_match(&rule.pattern, tool_name) {
                return rule.level.clone();
            }
        }
        PermissionLevel::Ask
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageThreshold {
    pub enabled: bool,
    pub soft_limit_usd: f64,
    pub hard_limit_usd: f64,
    pub warning_thresholds: Vec<f64>,
}

impl Default for UsageThreshold {
    fn default() -> Self {
        Self {
            enabled: false,
            soft_limit_usd: 0.5,
            hard_limit_usd: 2.0,
            warning_thresholds: vec![0.05, 0.10, 0.25, 0.50, 1.00],
        }
    }
}

impl UsageThreshold {
    pub fn check(&self, current_spend: f64, estimated_cost: f64) -> UsageCheckResult {
        if !self.enabled {
            return UsageCheckResult::Allowed;
        }
        let total = current_spend + estimated_cost;
        if total > self.hard_limit_usd {
            return UsageCheckResult::Blocked {
                reason: format!(
                    "Estimated total ${:.2} exceeds hard limit ${:.2}",
                    total, self.hard_limit_usd
                ),
            };
        }
        if total > self.soft_limit_usd {
            return UsageCheckResult::RequiresApproval {
                current_spend,
                estimated_cost,
                limit: self.soft_limit_usd,
            };
        }
        for &threshold in &self.warning_thresholds {
            if current_spend < threshold && total >= threshold {
                return UsageCheckResult::RequiresApproval {
                    current_spend,
                    estimated_cost,
                    limit: threshold,
                };
            }
        }
        UsageCheckResult::Allowed
    }
}

#[derive(Debug, Clone)]
pub enum UsageCheckResult {
    Allowed,
    RequiresApproval {
        current_spend: f64,
        estimated_cost: f64,
        limit: f64,
    },
    Blocked {
        reason: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YoloBudgetConfig {
    pub enabled: bool,
    pub max_spend_per_turn: f64,
    pub max_spend_per_session: f64,
    pub cooldown_after_pause: f64,
    pub auto_resume_delay_secs: u64,
}

impl Default for YoloBudgetConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_spend_per_turn: 0.10,
            max_spend_per_session: 1.0,
            cooldown_after_pause: 0.01,
            auto_resume_delay_secs: 5,
        }
    }
}

#[derive(Debug, Clone)]
pub struct YoloBudgetState {
    pub turn_spend: f64,
    pub session_spend: f64,
    pub paused: bool,
}

impl YoloBudgetState {
    pub fn new() -> Self {
        Self {
            turn_spend: 0.0,
            session_spend: 0.0,
            paused: false,
        }
    }
}

impl Default for YoloBudgetState {
    fn default() -> Self {
        Self::new()
    }
}

impl YoloBudgetConfig {
    pub fn check(
        &self,
        state: &YoloBudgetState,
        estimated_cost: f64,
    ) -> YoloBudgetDecision {
        if !self.enabled {
            return YoloBudgetDecision::Allowed;
        }
        if state.paused {
            return YoloBudgetDecision::Paused;
        }
        let turn_total = state.turn_spend + estimated_cost;
        let session_total = state.session_spend + estimated_cost;
        if turn_total > self.max_spend_per_turn {
            return YoloBudgetDecision::RequiresApproval {
                reason: format!(
                    "Turn spend ${:.3} exceeds limit ${:.3}",
                    turn_total, self.max_spend_per_turn
                ),
            };
        }
        if session_total > self.max_spend_per_session {
            return YoloBudgetDecision::RequiresApproval {
                reason: format!(
                    "Session spend ${:.3} exceeds limit ${:.3}",
                    session_total, self.max_spend_per_session
                ),
            };
        }
        YoloBudgetDecision::Allowed
    }
}

#[derive(Debug, Clone)]
pub enum YoloBudgetDecision {
    Allowed,
    RequiresApproval {
        reason: String,
    },
    Paused,
}

#[derive(Debug, Clone)]
pub struct ApprovalContext {
    pub tool_name: String,
    pub args: serde_json::Value,
    pub current_spend_usd: f64,
    pub estimated_cost_usd: Option<f64>,
    pub turn: u32,
}

#[derive(Debug, Clone)]
pub enum ApprovalResult {
    Approved,
    Rejected { reason: String },
    RequiresApproval { reason: String },
}

impl ApprovalResult {
    pub fn is_allowed(&self) -> bool {
        matches!(self, ApprovalResult::Approved)
    }
}

pub struct ApprovalGateV2 {
    pub permission_ruleset: PermissionRuleset,
    pub usage_threshold: UsageThreshold,
    pub yolo_budget_config: YoloBudgetConfig,
}

impl ApprovalGateV2 {
    pub fn new() -> Self {
        Self {
            permission_ruleset: PermissionRuleset::default(),
            usage_threshold: UsageThreshold::default(),
            yolo_budget_config: YoloBudgetConfig::default(),
        }
    }

    pub fn evaluate(
        &self,
        ctx: &ApprovalContext,
        budget_state: &YoloBudgetState,
    ) -> ApprovalResult {
        let perm = self.permission_ruleset.evaluate(&ctx.tool_name);
        match perm {
            PermissionLevel::Deny => {
                return ApprovalResult::Rejected {
                    reason: format!("Tool '{}' is denied by permission ruleset", ctx.tool_name),
                };
            }
            PermissionLevel::Allow => {}
            PermissionLevel::Ask => {
                return ApprovalResult::RequiresApproval {
                    reason: format!("Tool '{}' requires approval", ctx.tool_name),
                };
            }
        }

        let est = ctx.estimated_cost_usd.unwrap_or(0.0);
        let usage_check = self.usage_threshold.check(ctx.current_spend_usd, est);
        match usage_check {
            UsageCheckResult::Blocked { reason } => {
                return ApprovalResult::Rejected { reason };
            }
            UsageCheckResult::RequiresApproval { .. } => {
                return ApprovalResult::RequiresApproval {
                    reason: "Usage threshold would be exceeded".into(),
                };
            }
            UsageCheckResult::Allowed => {}
        }

        let yolo_check = self.yolo_budget_config.check(budget_state, est);
        match yolo_check {
            YoloBudgetDecision::RequiresApproval { reason } => {
                return ApprovalResult::RequiresApproval { reason };
            }
            YoloBudgetDecision::Paused => {
                return ApprovalResult::RequiresApproval {
                    reason: "YOLO budget is paused".into(),
                };
            }
            YoloBudgetDecision::Allowed => {}
        }

        ApprovalResult::Approved
    }
}

impl Default for ApprovalGateV2 {
    fn default() -> Self {
        Self::new()
    }
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
    fn test_usage_threshold_allowed_when_disabled() {
        let t = UsageThreshold { enabled: false, ..Default::default() };
        assert!(matches!(t.check(0.0, 0.01), UsageCheckResult::Allowed));
    }

    #[test]
    fn test_usage_threshold_hard_limit() {
        let t = UsageThreshold {
            enabled: true,
            soft_limit_usd: 0.5,
            hard_limit_usd: 2.0,
            ..Default::default()
        };
        match t.check(1.5, 0.6) {
            UsageCheckResult::Blocked { .. } => {}
            _ => panic!("Expected blocked"),
        }
    }

    #[test]
    fn test_usage_threshold_soft_limit() {
        let t = UsageThreshold {
            enabled: true,
            soft_limit_usd: 0.5,
            hard_limit_usd: 2.0,
            ..Default::default()
        };
        match t.check(0.3, 0.3) {
            UsageCheckResult::RequiresApproval { .. } => {}
            _ => panic!("Expected requires approval"),
        }
    }

    #[test]
    fn test_yolo_budget_allowed() {
        let cfg = YoloBudgetConfig {
            enabled: true,
            max_spend_per_turn: 0.10,
            max_spend_per_session: 1.0,
            ..Default::default()
        };
        let state = YoloBudgetState::new();
        assert!(matches!(cfg.check(&state, 0.05), YoloBudgetDecision::Allowed));
    }

    #[test]
    fn test_yolo_budget_turn_limit() {
        let cfg = YoloBudgetConfig {
            enabled: true,
            max_spend_per_turn: 0.10,
            max_spend_per_session: 1.0,
            ..Default::default()
        };
        let state = YoloBudgetState { turn_spend: 0.08, ..Default::default() };
        match cfg.check(&state, 0.05) {
            YoloBudgetDecision::RequiresApproval { .. } => {}
            _ => panic!("Expected requires approval"),
        }
    }

    #[test]
    fn test_yolo_budget_paused() {
        let cfg = YoloBudgetConfig { enabled: true, ..Default::default() };
        let state = YoloBudgetState { paused: true, ..Default::default() };
        assert!(matches!(cfg.check(&state, 0.01), YoloBudgetDecision::Paused));
    }

    #[test]
    fn test_approval_gate_v2_deny_by_permission() {
        let gate = ApprovalGateV2 {
            permission_ruleset: PermissionRuleset::new(vec![
                PermissionRule {
                    pattern: "bash".into(),
                    level: PermissionLevel::Deny,
                    reason: Some("not allowed".into()),
                },
            ]),
            ..Default::default()
        };
        let ctx = ApprovalContext {
            tool_name: "bash".into(),
            args: serde_json::json!({}),
            current_spend_usd: 0.0,
            estimated_cost_usd: None,
            turn: 1,
        };
        let result = gate.evaluate(&ctx, &YoloBudgetState::new());
        assert!(!result.is_allowed());
        match result {
            ApprovalResult::Rejected { .. } => {}
            _ => panic!("Expected rejected"),
        }
    }

    #[test]
    fn test_approval_gate_v2_allowed() {
        let gate = ApprovalGateV2 {
            permission_ruleset: PermissionRuleset::new(vec![
                PermissionRule {
                    pattern: "read".into(),
                    level: PermissionLevel::Allow,
                    reason: None,
                },
            ]),
            usage_threshold: UsageThreshold { enabled: false, ..Default::default() },
            yolo_budget_config: YoloBudgetConfig { enabled: false, ..Default::default() },
        };
        let ctx = ApprovalContext {
            tool_name: "read".into(),
            args: serde_json::json!({}),
            current_spend_usd: 0.0,
            estimated_cost_usd: None,
            turn: 1,
        };
        let result = gate.evaluate(&ctx, &YoloBudgetState::new());
        assert!(result.is_allowed(), "Expected approved, got {:?}", result);
    }
}
