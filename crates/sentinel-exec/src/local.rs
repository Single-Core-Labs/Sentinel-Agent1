use async_trait::async_trait;
use sentinel_sandbox::SandboxPolicy;
use crate::executor::{ExecOutput, ExecError, Executor};

pub struct LocalExecutor {
    policy: Option<SandboxPolicy>,
}

impl LocalExecutor {
    pub fn new(policy: Option<SandboxPolicy>) -> Self {
        Self { policy }
    }
}

#[async_trait]
impl Executor for LocalExecutor {
    async fn exec(&self, command: &str, args: &[&str], _env: Option<Vec<(String, String)>>) -> Result<ExecOutput, ExecError> {
        if let Some(ref p) = self.policy {
            if !p.can_execute(command) {
                return Err(ExecError::PermissionDenied(format!("Execution of '{}' denied by sandbox policy", command)));
            }
        }

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
        if let Some(ref p) = self.policy {
            if !p.can_read(path) {
                return Err(ExecError::PermissionDenied(format!("Read of '{}' denied by sandbox policy", path)));
            }
        }
        Ok(std::fs::read_to_string(path)?)
    }

    async fn write_file(&self, path: &str, content: &str) -> Result<(), ExecError> {
        if let Some(ref p) = self.policy {
            if !p.can_write(path) {
                return Err(ExecError::PermissionDenied(format!("Write to '{}' denied by sandbox policy", path)));
            }
        }
        Ok(std::fs::write(path, content)?)
    }

    async fn exists(&self, path: &str) -> bool {
        if let Some(ref p) = self.policy {
            if !p.can_read(path) {
                return false;
            }
        }
        std::path::Path::new(path).exists()
    }
}
