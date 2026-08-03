use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ActiveEditorState {
    pub active_file: Option<String>,
    pub open_tabs: Vec<String>,
    pub cursor_line: Option<u32>,
    pub cursor_column: Option<u32>,
    pub selected_text: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct EditorTracker {
    state: Arc<RwLock<ActiveEditorState>>,
}

impl EditorTracker {
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(ActiveEditorState::default())),
        }
    }

    pub async fn update(&self, new_state: ActiveEditorState) {
        let mut w = self.state.write().await;
        *w = new_state;
    }

    pub async fn get_state(&self) -> ActiveEditorState {
        self.state.read().await.clone()
    }
}
