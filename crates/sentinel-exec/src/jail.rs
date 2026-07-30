use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use crate::executor::{ExecOutput, ExecError};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum JailMode {
    Auto,
    Disabled,
    JobObject,
    Bubblewrap,
    Seatbelt,
}

#[derive(Debug, Clone)]
pub struct OSJailSandbox {
    pub mode: JailMode,
    pub workdir: PathBuf,
}

impl OSJailSandbox {
    pub fn new(workdir: impl Into<PathBuf>) -> Self {
        Self {
            mode: JailMode::Auto,
            workdir: workdir.into(),
        }
    }

    pub fn with_mode(mut self, mode: JailMode) -> Self {
        self.mode = mode;
        self
    }

    pub async fn run(&self, command: &str, args: &[&str], env: Option<Vec<(String, String)>>) -> Result<ExecOutput, ExecError> {
        let os = std::env::consts::OS;
        match self.mode {
            JailMode::Disabled => self.run_raw(command, args, env).await,
            JailMode::Bubblewrap => self.run_bubblewrap(command, args, env).await,
            JailMode::Seatbelt => self.run_seatbelt(command, args, env).await,
            JailMode::JobObject => self.run_job_object(command, args, env).await,
            JailMode::Auto => {
                match os {
                    "linux" => {
                        if which_exists("bwrap") {
                            self.run_bubblewrap(command, args, env).await
                        } else {
                            self.run_raw(command, args, env).await
                        }
                    }
                    "macos" => {
                        if which_exists("sandbox-exec") {
                            self.run_seatbelt(command, args, env).await
                        } else {
                            self.run_raw(command, args, env).await
                        }
                    }
                    "windows" => self.run_job_object(command, args, env).await,
                    _ => self.run_raw(command, args, env).await,
                }
            }
        }
    }

    async fn run_raw(&self, command: &str, args: &[&str], env: Option<Vec<(String, String)>>) -> Result<ExecOutput, ExecError> {
        let mut cmd = tokio::process::Command::new(command);
        cmd.args(args);
        cmd.current_dir(&self.workdir);
        if let Some(env_vars) = env {
            for (k, v) in env_vars {
                cmd.env(k, v);
            }
        }

        let output = cmd.output().await?;
        Ok(ExecOutput {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }

    async fn run_bubblewrap(&self, command: &str, args: &[&str], env: Option<Vec<(String, String)>>) -> Result<ExecOutput, ExecError> {
        let mut bwrap = tokio::process::Command::new("bwrap");
        let workdir_str = self.workdir.to_string_lossy();
        bwrap.arg("--unshare-all")
            .arg("--proc").arg("/proc")
            .arg("--dev").arg("/dev")
            .arg("--tmpfs").arg("/tmp")
            .arg("--ro-bind").arg("/").arg("/")
            .arg("--bind").arg(&*workdir_str).arg(&*workdir_str)
            .arg("--chdir").arg(&*workdir_str)
            .arg("--").arg(command).args(args);

        if let Some(env_vars) = env {
            for (k, v) in env_vars {
                bwrap.env(k, v);
            }
        }

        let output = bwrap.output().await?;
        Ok(ExecOutput {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }

    async fn run_seatbelt(&self, command: &str, args: &[&str], env: Option<Vec<(String, String)>>) -> Result<ExecOutput, ExecError> {
        let mut sbox = tokio::process::Command::new("sandbox-exec");
        let profile = format!("(version 1)(allow default)(deny file-write* (regex #\"^(?!{})\"#))", self.workdir.to_string_lossy());
        sbox.arg("-p").arg(profile)
            .arg(command).args(args)
            .current_dir(&self.workdir);

        if let Some(env_vars) = env {
            for (k, v) in env_vars {
                sbox.env(k, v);
            }
        }

        let output = sbox.output().await?;
        Ok(ExecOutput {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }

    async fn run_job_object(&self, command: &str, args: &[&str], env: Option<Vec<(String, String)>>) -> Result<ExecOutput, ExecError> {
        // Windows execution wrapper
        self.run_raw(command, args, env).await
    }
}

fn which_exists(cmd: &str) -> bool {
    if let Ok(path) = std::env::var("PATH") {
        for p in std::env::split_paths(&path) {
            let full = p.join(cmd);
            if full.is_file() {
                return true;
            }
            if cfg!(target_os = "windows") {
                let exe = p.join(format!("{}.exe", cmd));
                if exe.is_file() {
                    return true;
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_jail_auto_exec() {
        let jail = OSJailSandbox::new(std::env::temp_dir());
        let res = jail.run(if cfg!(target_os = "windows") { "cmd" } else { "echo" },
                           if cfg!(target_os = "windows") { &["/C", "echo hello"] } else { &["hello"] },
                           None).await;
        assert!(res.is_ok());
        let out = res.unwrap();
        assert!(out.stdout.contains("hello"));
    }
}
