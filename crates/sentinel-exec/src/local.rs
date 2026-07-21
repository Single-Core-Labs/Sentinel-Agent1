use async_trait::async_trait;
use crate::executor::{ExecOutput, ExecError, Executor};

pub struct LocalExecutor;

impl LocalExecutor {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Executor for LocalExecutor {
    async fn exec(&self, command: &str, args: &[&str], _env: Option<Vec<(String, String)>>) -> Result<ExecOutput, ExecError> {
        let result = tokio::process::Command::new(command)
            .args(args)
            .output()
            .await;

        match result {
            Ok(output) => Ok(ExecOutput {
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                exit_code: output.status.code().unwrap_or(-1),
            }),
            Err(e) => Ok(ExecOutput {
                stdout: String::new(),
                stderr: format!("Failed to execute command: {}", e),
                exit_code: -1,
            }),
        }
    }

    async fn read_file(&self, path: &str) -> Result<String, ExecError> {
        Ok(std::fs::read_to_string(path)?)
    }

    async fn write_file(&self, path: &str, content: &str) -> Result<(), ExecError> {
        Ok(std::fs::write(path, content)?)
    }

    async fn exists(&self, path: &str) -> bool {
        std::path::Path::new(path).exists()
    }
}
