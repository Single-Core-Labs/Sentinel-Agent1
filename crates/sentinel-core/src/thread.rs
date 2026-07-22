use uuid::Uuid;
use sentinel_protocol::Message;
use crate::budget::BudgetGuard;
use crate::context::ContextManager;
use crate::conversation::Conversation;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Plan,
    Act,
}

impl Phase {
    pub fn is_plan(self) -> bool { self == Phase::Plan }
    pub fn is_act(self) -> bool { self == Phase::Act }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ThreadStatus {
    Idle,
    Running,
    AwaitingApproval,
    Completed,
    Cancelled,
    Error(String),
}

#[derive(Debug)]
pub struct ApprovalRequest {
    pub tool_name: String,
    pub args: serde_json::Value,
    pub prompt: String,
}

#[derive(Debug)]
pub struct AgentThread {
    pub id: Uuid,
    pub status: ThreadStatus,
    pub phase: Phase,
    pub conversation: Conversation,
    pub context: ContextManager,
    pub turn: u32,
    pub iterations: u32,
    pub max_turns: u32,
    pub max_iterations: u32,
    pub yolo_mode: bool,
    pub parent_thread_id: Option<String>,
    pub budget: BudgetGuard,
}

impl AgentThread {
    pub fn new(max_turns: u32, max_iterations: u32, yolo_mode: bool) -> Self {
        Self {
            id: Uuid::new_v4(),
            status: ThreadStatus::Idle,
            phase: Phase::Plan,
            conversation: Conversation::new(),
            context: ContextManager::new(128000),
            turn: 0,
            iterations: 0,
            max_turns,
            max_iterations,
            yolo_mode,
            parent_thread_id: None,
            budget: BudgetGuard::new(None, yolo_mode),
        }
    }

    pub fn enter_act_phase(&mut self) {
        self.phase = Phase::Act;
    }

    pub fn add_message(&mut self, msg: Message) {
        self.context.add(msg);
    }

    pub fn is_doom_loop(&self) -> bool {
        let msgs = self.context.messages();
        let tool_call_count = msgs.iter()
            .filter(|m| m.is_tool_call())
            .count();

        let is_all_tool_calls = tool_call_count > 20 && tool_call_count == self.iterations as usize;

        let is_repeated_tool = if msgs.len() >= 6 {
            let recent: Vec<&sentinel_protocol::Message> = msgs.iter().rev().take(6).collect();
            let tool_names: Vec<&str> = recent.iter()
                .filter_map(|m| {
                    if m.is_tool_call() {
                        m.content.iter().find_map(|b| {
                            if let sentinel_protocol::ContentBlock::ToolCall { name, .. } = b {
                                Some(name.as_str())
                            } else { None }
                        })
                    } else { None }
                })
                .collect();
            tool_names.len() >= 3 && tool_names.windows(3).all(|w| w[0] == w[1] && w[1] == w[2])
        } else {
            false
        };

        let is_same_tool_result = if msgs.len() >= 4 {
            let recent: Vec<&sentinel_protocol::Message> = msgs.iter().rev().take(4).collect();
            let errors: Vec<bool> = recent.iter()
                .filter_map(|m| {
                    if m.role == sentinel_protocol::Role::Tool {
                        m.content.iter().find_map(|b| {
                            if let sentinel_protocol::ContentBlock::ToolResult { is_error, .. } = b {
                                Some(is_error.unwrap_or(false))
                            } else { None }
                        })
                    } else { None }
                })
                .collect();
            errors.len() >= 3 && errors.iter().all(|e| *e)
        } else {
            false
        };

        is_all_tool_calls || is_repeated_tool || is_same_tool_result
    }

    pub fn increment_iteration(&mut self) -> bool {
        self.iterations += 1;
        self.iterations < self.max_iterations
    }

    pub fn increment_turn(&mut self) -> bool {
        self.turn += 1;
        self.turn < self.max_turns
    }

    /// Create a thread with a specific budget cap (for YOLO mode with cost limits).
    pub fn with_budget(max_turns: u32, max_iterations: u32, yolo_mode: bool, cost_cap_usd: Option<f64>, phase: Phase) -> Self {
        Self {
            budget: BudgetGuard::new(cost_cap_usd, yolo_mode),
            phase,
            ..Self::new(max_turns, max_iterations, yolo_mode)
        }
    }

    pub fn with_phase(mut self, phase: Phase) -> Self {
        self.phase = phase;
        self
    }

    pub fn fork(&self) -> Self {
        let forked_conversation = self.conversation.clone();
        Self {
            id: Uuid::new_v4(),
            status: ThreadStatus::Idle,
            phase: self.phase,
            conversation: forked_conversation,
            context: ContextManager::new(128000),
            turn: 0,
            iterations: 0,
            max_turns: self.max_turns,
            max_iterations: self.max_iterations,
            yolo_mode: self.yolo_mode,
            parent_thread_id: Some(self.id.to_string()),
            budget: BudgetGuard::new(self.budget.cost_cap_usd, self.budget.auto_approval_enabled),
        }
    }

    pub fn fork_at_turn(&self, turn_number: u32) -> Self {
        let forked_conversation = self.conversation.fork_at_turn(turn_number);
        Self {
            id: Uuid::new_v4(),
            status: ThreadStatus::Idle,
            phase: self.phase,
            conversation: forked_conversation,
            context: ContextManager::new(128000),
            turn: 0,
            iterations: 0,
            max_turns: self.max_turns,
            max_iterations: self.max_iterations,
            yolo_mode: self.yolo_mode,
            parent_thread_id: Some(self.id.to_string()),
            budget: BudgetGuard::new(self.budget.cost_cap_usd, self.budget.auto_approval_enabled),
        }
    }
}
