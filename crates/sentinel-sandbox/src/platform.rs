use crate::policy::SandboxPolicy;

#[derive(Debug)]
pub struct Sandbox {
    policy: SandboxPolicy,
}

impl Sandbox {
    pub fn new(policy: SandboxPolicy) -> Self {
        Self { policy }
    }

    pub fn policy(&self) -> &SandboxPolicy {
        &self.policy
    }

    pub fn check_read(&self, path: &str) -> Result<(), SandboxViolation> {
        if self.policy.can_read(path) {
            Ok(())
        } else {
            Err(SandboxViolation::ReadDenied(path.to_string()))
        }
    }

    pub fn check_write(&self, path: &str) -> Result<(), SandboxViolation> {
        if self.policy.can_write(path) {
            Ok(())
        } else {
            Err(SandboxViolation::WriteDenied(path.to_string()))
        }
    }

    pub fn check_execute(&self, command: &str) -> Result<(), SandboxViolation> {
        if self.policy.can_execute(command) {
            Ok(())
        } else {
            Err(SandboxViolation::CommandDenied(command.to_string()))
        }
    }

    pub fn check_network(&self) -> Result<(), SandboxViolation> {
        if self.policy.network {
            Ok(())
        } else {
            Err(SandboxViolation::NetworkDenied)
        }
    }
}

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SandboxViolation {
    #[error("Read access denied: {0}")]
    ReadDenied(String),
    #[error("Write access denied: {0}")]
    WriteDenied(String),
    #[error("Command execution denied: {0}")]
    CommandDenied(String),
    #[error("Network access denied")]
    NetworkDenied,
}
