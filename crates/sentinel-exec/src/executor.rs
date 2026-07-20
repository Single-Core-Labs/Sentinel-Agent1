use async_trait::async_trait;

#[derive(Debug, Clone)]
pub struct ExecOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

impl ExecOutput {
    pub fn success(&self) -> bool { self.exit_code == 0 }
    pub fn text(&self) -> String {
        let mut t = self.stdout.clone();
        if !self.stderr.is_empty() {
            if !t.is_empty() { t.push('\n'); }
            t.push_str(&self.stderr);
        }
        t
    }
}

#[async_trait]
pub trait Executor: Send + Sync {
    async fn exec(&self, command: &str, args: &[&str], env: Option<Vec<(String, String)>>) -> Result<ExecOutput, ExecError>;
    async fn read_file(&self, path: &str) -> Result<String, ExecError>;
    async fn write_file(&self, path: &str, content: &str) -> Result<(), ExecError>;
    async fn exists(&self, path: &str) -> bool;
}

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ExecError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
}
