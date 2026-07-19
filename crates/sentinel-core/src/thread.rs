use uuid::Uuid;
use sentinel_protocol::Message;
use crate::context::ContextManager;

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
    pub context: ContextManager,
    pub turn: u32,
    pub iterations: u32,
    pub max_turns: u32,
    pub max_iterations: u32,
    pub yolo_mode: bool,
}

impl AgentThread {
    pub fn new(max_turns: u32, max_iterations: u32, yolo_mode: bool) -> Self {
        Self {
            id: Uuid::new_v4(),
            status: ThreadStatus::Idle,
            context: ContextManager::new(128000),
            turn: 0,
            iterations: 0,
            max_turns,
            max_iterations,
            yolo_mode,
        }
    }

    pub fn add_message(&mut self, msg: Message) {
        self.context.add(msg);
    }

    pub fn is_doom_loop(&self) -> bool {
        let msgs = self.context.messages();
        let tool_call_count = msgs.iter()
            .filter(|m| m.is_tool_call())
            .count();
        tool_call_count > 20 && tool_call_count == self.iterations as usize
    }

    pub fn increment_iteration(&mut self) -> bool {
        self.iterations += 1;
        self.iterations < self.max_iterations
    }

    pub fn increment_turn(&mut self) -> bool {
        self.turn += 1;
        self.turn < self.max_turns
    }
}
