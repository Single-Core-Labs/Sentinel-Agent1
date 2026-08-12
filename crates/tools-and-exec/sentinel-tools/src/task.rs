use crate::tool::{Tool, ToolContext, ToolOutput};
use async_trait::async_trait;
use serde_json::json;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};
use tokio::sync::mpsc;

/// Terminal-status tags used by the task registry (JSON-serializable strings).
pub const TASK_PENDING: &str = "pending";
pub const TASK_RUNNING: &str = "running";
pub const TASK_COMPLETED: &str = "completed";
pub const TASK_FAILED: &str = "failed";
pub const TASK_CANCELLED: &str = "cancelled";

pub fn is_terminal(status: &str) -> bool {
    matches!(status, TASK_COMPLETED | TASK_FAILED | TASK_CANCELLED)
}

/// Shared task manager: every `TaskTool` instance (and therefore every
/// `ToolRegistry` in the process, including forked sub-agents) talks to the
/// same background task pool, so a task spawned by one agent turn can be
/// polled by a later one.
static MANAGER: OnceLock<Arc<TaskManager>> = OnceLock::new();

pub fn shared_task_manager() -> Arc<TaskManager> {
    MANAGER.get_or_init(TaskManager::new).clone()
}

pub struct TaskManager {
    tasks: Mutex<HashMap<String, TaskRecord>>,
    next_id: AtomicU64,
    self_weak: std::sync::Weak<TaskManager>,
}

impl TaskManager {
    /// Create a detached manager (used by tests). Agent registries use
    /// [`shared_task_manager`] so all tools see the same task pool.
    pub fn new() -> Arc<Self> {
        Arc::new_cyclic(|weak| Self {
            tasks: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            self_weak: weak.clone(),
        })
    }
}

/// A background task. The `kill_tx` half-channel is the kill switch: sending
/// `()` through it makes the running task terminate its child and record
/// itself as cancelled. The struct is `Send` so it can live inside the
/// registry used by concurrent sub-agents.
#[derive(Debug)]
struct TaskRecord {
    id: String,
    command: String,
    workdir: String,
    status: String,
    output: String,
    exit_code: Option<i32>,
    started_at_ms: u64,
    finished_at_ms: Option<u64>,
    kill_tx: Option<mpsc::Sender<()>>,
}

impl TaskManager {
    fn next_id(&self) -> String {
        format!("task-{}", self.next_id.fetch_add(1, Ordering::SeqCst))
    }

    /// Spawn `command` in the background and return its task id.
    ///
    /// The child runs under `cmd /C` (Windows) or `sh -c` (Unix) like
    /// `run_shell_command`, but is not wrapped in a sandbox jail and is NOT
    /// time-limited by this call: it keeps running until it exits, hits
    /// `timeout_ms` (optional), or is killed via [`kill`](Self::kill).
    pub fn spawn(&self, command: &str, workdir: &str, timeout_ms: Option<u64>) -> String {
        let id = self.next_id();
        let (kill_tx, kill_rx) = mpsc::channel::<()>(1);

        {
            let mut tasks = self.tasks.lock().unwrap();
            tasks.insert(
                id.clone(),
                TaskRecord {
                    id: id.clone(),
                    command: command.to_string(),
                    workdir: workdir.to_string(),
                    status: TASK_RUNNING.into(),
                    output: String::new(),
                    exit_code: None,
                    started_at_ms: now_ms(),
                    finished_at_ms: None,
                    kill_tx: Some(kill_tx),
                },
            );
        }

        let manager = self
            .self_weak
            .upgrade()
            .unwrap_or_else(|| panic!("task manager dropped while spawning a task"));
        let command = command.to_string();
        let workdir = workdir.to_string();
        let task_id = id.clone();
        tokio::spawn(async move {
            #[cfg(target_os = "windows")]
            let (shell, shell_arg) = ("cmd", "/C");
            #[cfg(not(target_os = "windows"))]
            let (shell, shell_arg) = ("sh", "-c");

            let spawned = Command::new(shell)
                .arg(shell_arg)
                .arg(&command)
                .current_dir(&workdir)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn();

            let (status, output, exit_code) = match spawned {
                Ok(child) => {
                    run_to_terminal(child, kill_rx, timeout_ms.map(Duration::from_millis)).await
                }
                Err(e) => (TASK_FAILED.into(), format!("spawn failed: {e}"), None),
            };

            manager.finish(&task_id, &status, &output, exit_code);
        });

        id
    }

    pub fn kill(&self, id: &str) -> Result<(), String> {
        let mut tasks = self.tasks.lock().unwrap();
        let record = tasks
            .get_mut(id)
            .ok_or_else(|| format!("no such task: {id}"))?;
        if is_terminal(&record.status) {
            return Ok(());
        }
        match record.kill_tx.take() {
            Some(tx) => {
                let _ = tx.try_send(());
                Ok(())
            }
            None => Err("task has no kill switch".into()),
        }
    }

    pub fn record_json(&self, id: &str) -> Option<serde_json::Value> {
        self.tasks.lock().unwrap().get(id).map(TaskRecord::to_json)
    }

    pub fn list_json(&self) -> serde_json::Value {
        let tasks = self.tasks.lock().unwrap();
        json!({
            "tasks": tasks.values().map(TaskRecord::to_json).collect::<Vec<_>>()
        })
    }

    /// Poll a task until it reaches a terminal state or `timeout_ms` elapses.
    /// Returns the final (or latest) record as JSON.
    pub async fn wait(
        &self,
        id: &str,
        timeout_ms: Option<u64>,
    ) -> Result<serde_json::Value, String> {
        let deadline = timeout_ms
            .map(|ms| Instant::now() + Duration::from_millis(ms))
            .unwrap_or_else(|| Instant::now() + Duration::from_secs(60));
        loop {
            let snapshot = self
                .tasks
                .lock()
                .unwrap()
                .get(id)
                .map(TaskRecord::to_json)
                .ok_or_else(|| format!("no such task: {id}"))?;
            let status = snapshot["status"].as_str().unwrap_or("");
            if is_terminal(status) || Instant::now() >= deadline {
                return Ok(snapshot);
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    fn finish(&self, id: &str, status: &str, output: &str, exit_code: Option<i32>) {
        let mut tasks = self.tasks.lock().unwrap();
        if let Some(record) = tasks.get_mut(id) {
            record.status = status.to_string();
            record.output = output.to_string();
            record.exit_code = exit_code;
            record.finished_at_ms = Some(now_ms());
            record.kill_tx = None;
        }
    }
}

/// Drive a child to completion (or to timeout / kill), then drain stdout and
/// stderr. Returns (status, combined output, exit code).
///
/// Locking discipline: the child mutex is never held across an await, so the
/// kill and timeout paths can always acquire it. On Windows killing the
/// `cmd /C` wrapper orphans the real command, which keeps the stdout pipe
/// open; killed tasks therefore skip the (potentially unbounded) drain.
async fn run_to_terminal(
    child: Child,
    mut kill_rx: mpsc::Receiver<()>,
    timeout: Option<Duration>,
) -> (String, String, Option<i32>) {
    let child = tokio::sync::Mutex::new(child);
    let mut stdout = {
        let mut c = child.lock().await;
        c.stdout.take()
    };
    let mut stderr = {
        let mut c = child.lock().await;
        c.stderr.take()
    };

    let out_task = tokio::spawn({
        let mut stdout = stdout.take().expect("stdout is piped");
        async move {
            let mut buf = Vec::new();
            let _ = stdout.read_to_end(&mut buf).await;
            String::from_utf8_lossy(&buf).into_owned()
        }
    });
    let err_task = tokio::spawn({
        let mut stderr = stderr.take().expect("stderr is piped");
        async move {
            let mut buf = Vec::new();
            let _ = stderr.read_to_end(&mut buf).await;
            String::from_utf8_lossy(&buf).into_owned()
        }
    });

    let deadline = timeout.map(|d| Instant::now() + d);

    let outcome = loop {
        {
            let mut c = child.lock().await;
            if let Some(status) = c.try_wait().ok().flatten() {
                break Ok(status.code());
            }
        }
        match kill_rx.try_recv() {
            Ok(()) | Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                break Err(TaskEnd::Killed);
            }
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {}
        }
        if let Some(d) = deadline
            && Instant::now() >= d
        {
            break Err(TaskEnd::TimedOut);
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    };

    match outcome {
        Ok(code) => {
            // The child exited on its own: its stdout/stderr are at EOF, so
            // draining is bounded.
            let (out, err) = tokio::join!(out_task, err_task);
            let out = out.unwrap_or_default();
            let err = err.unwrap_or_default();
            let status = if code.is_some_and(|c| c != 0) || code.is_none() {
                TASK_FAILED.to_string()
            } else {
                TASK_COMPLETED.to_string()
            };
            let combined = if err.is_empty() {
                out
            } else if out.is_empty() {
                err
            } else {
                format!("{out}\n{err}")
            };
            (status, combined, code)
        }
        Err(TaskEnd::Killed) => {
            out_task.abort();
            err_task.abort();
            let mut c = child.lock().await;
            let _ = c.kill().await;
            let _ = c.wait().await;
            (TASK_CANCELLED.to_string(), String::new(), None)
        }
        Err(TaskEnd::TimedOut) => {
            out_task.abort();
            err_task.abort();
            let mut c = child.lock().await;
            let _ = c.kill().await;
            let _ = c.wait().await;
            ("timed_out".to_string(), String::new(), None)
        }
    }
}

enum TaskEnd {
    Killed,
    TimedOut,
}

impl TaskRecord {
    fn to_json(&self) -> serde_json::Value {
        let elapsed_ms = self
            .finished_at_ms
            .unwrap_or_else(|| {
                if self.status == TASK_RUNNING {
                    now_ms()
                } else {
                    self.started_at_ms
                }
            })
            .saturating_sub(self.started_at_ms);
        json!({
            "task_id": self.id,
            "command": self.command,
            "workdir": self.workdir,
            "status": self.status,
            "output": self.output,
            "exit_code": self.exit_code,
            "started_at_ms": self.started_at_ms,
            "finished_at_ms": self.finished_at_ms,
            "elapsed_ms": elapsed_ms,
        })
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// `task` — spawn, poll, list, wait on and kill long-running background
/// shell commands. Tasks survive individual tool calls: a task spawned in one
/// turn keeps running while the agent does something else, and its output is
/// retrievable afterwards.
pub struct TaskTool {
    manager: Arc<TaskManager>,
}

impl Default for TaskTool {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskTool {
    pub fn new() -> Self {
        Self {
            manager: shared_task_manager(),
        }
    }
}

#[async_trait]
impl Tool for TaskTool {
    fn name(&self) -> &str {
        "task"
    }

    fn description(&self) -> &str {
        "Manage long-running background shell commands. Actions: spawn (start a command in the background and return its task_id), status (current state + output of a task), list (all tasks), wait (block until a task finishes or a timeout), kill (terminate a running task). Tasks survive across agent turns and tool calls."
    }

    fn is_mutating(&self) -> bool {
        true
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["spawn", "status", "list", "wait", "kill"],
                    "description": "Operation to perform"
                },
                "command": {
                    "type": "string",
                    "description": "Shell command to run in the background (spawn only)"
                },
                "workdir": {
                    "type": "string",
                    "description": "Working directory for the command (defaults to the workspace)"
                },
                "task_id": {
                    "type": "string",
                    "description": "Task id from a previous spawn (status/wait/kill only)"
                },
                "timeout_ms": {
                    "type": "integer",
                    "description": "For spawn: kill the command after this many ms. For wait: poll for at most this many ms before returning the latest state. Default 60000."
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, args: serde_json::Value, ctx: &ToolContext) -> ToolOutput {
        let action = args["action"].as_str().unwrap_or("");
        match action {
            "spawn" => {
                let command = args["command"].as_str().unwrap_or("");
                if command.is_empty() {
                    return ToolOutput::err("task spawn: command is required");
                }
                let workdir = args["workdir"]
                    .as_str()
                    .or(ctx.sandbox_dir.as_deref())
                    .or(ctx.workspace_dir.as_deref())
                    .unwrap_or(".")
                    .to_string();
                let timeout_ms = args["timeout_ms"].as_u64();
                let id = self.manager.spawn(command, &workdir, timeout_ms);
                ToolOutput::ok(
                    serde_json::to_string_pretty(&json!({
                        "task_id": id,
                        "status": TASK_RUNNING,
                        "message": "task spawned in the background; poll with task status or wait",
                    }))
                    .unwrap_or_default(),
                )
            }
            "status" => {
                let id = args["task_id"].as_str().unwrap_or("");
                if id.is_empty() {
                    return ToolOutput::err("task status: task_id is required");
                }
                match self.manager.record_json(id) {
                    Some(v) => ToolOutput::ok(serde_json::to_string_pretty(&v).unwrap_or_default()),
                    None => ToolOutput::err(format!("no such task: {id}")),
                }
            }
            "list" => ToolOutput::ok(
                serde_json::to_string_pretty(&self.manager.list_json()).unwrap_or_default(),
            ),
            "wait" => {
                let id = args["task_id"].as_str().unwrap_or("");
                if id.is_empty() {
                    return ToolOutput::err("task wait: task_id is required");
                }
                let timeout_ms = args["timeout_ms"].as_u64();
                match self.manager.wait(id, timeout_ms).await {
                    Ok(v) => ToolOutput::ok(serde_json::to_string_pretty(&v).unwrap_or_default()),
                    Err(e) => ToolOutput::err(e),
                }
            }
            "kill" => {
                let id = args["task_id"].as_str().unwrap_or("");
                if id.is_empty() {
                    return ToolOutput::err("task kill: task_id is required");
                }
                match self.manager.kill(id) {
                    Ok(()) => ToolOutput::ok(
                        serde_json::to_string_pretty(&json!({
                            "task_id": id,
                            "status": TASK_CANCELLED,
                            "message": "kill signal sent",
                        }))
                        .unwrap_or_default(),
                    ),
                    Err(e) => ToolOutput::err(e),
                }
            }
            other => ToolOutput::err(format!(
                "unknown task action '{other}' (expected one of spawn, status, list, wait, kill)"
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn long_command() -> &'static str {
        #[cfg(target_os = "windows")]
        {
            "ping -n 30 127.0.0.1"
        }
        #[cfg(not(target_os = "windows"))]
        {
            "sleep 30"
        }
    }

    #[tokio::test]
    async fn spawn_and_complete_echo() {
        let manager = TaskManager::new();
        let id = manager.spawn("echo hello-task-world", ".", None);
        let record = manager.wait(&id, Some(10_000)).await.unwrap();
        assert_eq!(record["status"], "completed");
        assert_eq!(record["exit_code"], serde_json::json!(0));
        let output = record["output"].as_str().unwrap_or("");
        assert!(output.contains("hello-task-world"), "output was: {output}");
    }

    #[tokio::test]
    async fn kill_running_task() {
        let manager = TaskManager::new();
        let id = manager.spawn(long_command(), ".", None);
        tokio::time::sleep(Duration::from_millis(300)).await;
        manager.kill(&id).unwrap();
        let record = manager.wait(&id, Some(10_000)).await.unwrap();
        assert_eq!(record["status"], "cancelled");
    }

    #[tokio::test]
    async fn timeout_kills_task() {
        let manager = TaskManager::new();
        let id = manager.spawn(long_command(), ".", Some(500));
        let record = manager.wait(&id, Some(10_000)).await.unwrap();
        let status = record["status"].as_str().unwrap_or("");
        assert!(
            status == "timed_out" || status == "cancelled",
            "status was: {status}"
        );
    }

    #[tokio::test]
    async fn task_survives_registry_decoupling() {
        // Every TaskTool shares the same process-global manager.
        let id = shared_task_manager().spawn("echo shared-pool", ".", None);
        let record = shared_task_manager().wait(&id, Some(10_000)).await.unwrap();
        assert_eq!(record["status"], "completed");
        assert!(
            record["output"]
                .as_str()
                .unwrap_or("")
                .contains("shared-pool")
        );
    }

    #[tokio::test]
    async fn failed_command_reports_exit_code() {
        let manager = TaskManager::new();
        let id = manager.spawn("exit 3", ".", None);
        let record = manager.wait(&id, Some(10_000)).await.unwrap();
        assert_eq!(record["status"], "failed");
        assert_eq!(record["exit_code"], serde_json::json!(3));
    }
}
