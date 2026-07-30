use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Mutex;

pub struct Daemon {
    pid_file: Option<String>,
    running: AtomicBool,
    server_handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl Daemon {
    pub fn new(pid_file: Option<String>) -> Self {
        Self {
            pid_file,
            running: AtomicBool::new(false),
            server_handle: Mutex::new(None),
        }
    }

    pub async fn start<F, E>(&self, run_server: F) -> Result<(), DaemonError>
    where
        F: std::future::Future<Output = Result<(), E>> + Send + 'static,
        E: std::error::Error + Send + 'static,
    {
        if self.running.load(Ordering::SeqCst) {
            return Err(DaemonError::AlreadyRunning);
        }

        // Write PID file
        if let Some(ref pid_path) = self.pid_file {
            if let Some(parent) = Path::new(pid_path).parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| DaemonError::IoError(e.to_string()))?;
            }
            let pid = std::process::id();
            std::fs::write(pid_path, pid.to_string())
                .map_err(|e| DaemonError::IoError(e.to_string()))?;
        }

        self.running.store(true, Ordering::SeqCst);

        let handle = tokio::spawn(async move {
            let _ = run_server.await;
        });

        *self.server_handle.lock().await = Some(handle);
        Ok(())
    }

    pub async fn stop(&self) -> Result<(), DaemonError> {
        self.running.store(false, Ordering::SeqCst);

        if let Some(handle) = self.server_handle.lock().await.take() {
            handle.abort();
        }

        // Remove PID file
        if let Some(ref pid_path) = self.pid_file {
            let _ = std::fs::remove_file(pid_path);
        }

        Ok(())
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

use thiserror::Error;

#[derive(Debug, Error)]
pub enum DaemonError {
    #[error("Already running")]
    AlreadyRunning,
    #[error("IO error: {0}")]
    IoError(String),
    #[error("Server error: {0}")]
    ServerError(String),
}
