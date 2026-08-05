//! LSP client lifecycle management.
//!
//! Language servers from the `[lsp_servers]` config section are started
//! asynchronously so they never block application startup. Each client:
//!
//! 1. spawns `command args` as a stdio subprocess;
//! 2. completes the LSP `initialize` / `initialized` handshake (Content-Length
//!    framed JSON-RPC), registering workspace/didChangeWatchedFiles capability
//!    interest;
//! 3. is watched for the rest of the app's life — an unexpected exit is
//!    treated as a crash and the client is restarted with exponential backoff
//!    (bounded by `max_restarts`);
//! 4. is terminated gracefully on application shutdown (`shutdown` request +
//!    `exit` notification, then a hard kill if the server does not comply).
//!
//! The manager is created synchronously from config ([`LspManager::from_config`])
//! but does no I/O until [`LspManager::start`] is called, which returns
//! immediately and drives everything on background tasks.

use sentinel_config::SentinelConfig;
use std::io;
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

/// LSP protocol version advertised to language servers.
pub const LSP_VERSION: &str = "3.17.0";

/// Default cap on consecutive crash-restarts before a client is given up on.
const DEFAULT_MAX_RESTARTS: u64 = 5;

/// Time allowed for the `initialize` handshake.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// Base backoff between crash restarts (doubled per attempt).
const BACKOFF_BASE: Duration = Duration::from_millis(250);

/// A client that stayed alive this long is considered stable; the restart
/// counter is reset so a single later crash does not burn the whole budget.
const STABLE_UPTIME: Duration = Duration::from_secs(30);

/// Per-client lifecycle state, shared between the watcher task and shutdown.
struct LspClientHandle {
    id: String,
    command: String,
    args: Vec<String>,
    languages: Vec<String>,
    max_restarts: u64,
    child: Option<Child>,
    restarts: u64,
    failed: bool,
    shutdown_requested: bool,
    last_error: Option<String>,
    started_at: Instant,
}

impl LspClientHandle {
    fn from_def(def: &sentinel_config::LspServerDef) -> Self {
        Self {
            id: def.id.clone(),
            command: def.command.clone(),
            args: def.args.clone(),
            languages: def.languages.clone(),
            max_restarts: DEFAULT_MAX_RESTARTS,
            child: None,
            restarts: 0,
            failed: false,
            shutdown_requested: false,
            last_error: None,
            started_at: Instant::now(),
        }
    }
}

/// Owns the LSP clients for the whole application. Cheap to construct, drives
/// no I/O until [`start`](Self::start).
pub struct LspManager {
    clients: Vec<Arc<Mutex<LspClientHandle>>>,
}

impl LspManager {
    /// Collect the LSP server definitions from config. Does not spawn anything.
    pub fn from_config(config: &SentinelConfig) -> Self {
        let clients = config
            .lsp_servers
            .iter()
            .map(|def| Arc::new(Mutex::new(LspClientHandle::from_def(def))))
            .collect();
        Self { clients }
    }

    /// Start every configured LSP client on a background task and return
    /// immediately. No-op when no servers are configured.
    pub fn start(&self) {
        for handle in &self.clients {
            let handle = Arc::clone(handle);
            tokio::spawn(async move {
                run_client(handle).await;
            });
        }
    }

    /// Gracefully terminate every LSP client: send `shutdown` + `exit`, give
    /// the server a brief window, then kill. Also requests each watcher task
    /// to stop so it cannot restart a client behind a closing app.
    pub async fn shutdown(&self) {
        for handle in &self.clients {
            let mut h = handle.lock().await;
            h.shutdown_requested = true;
            if let Some(child) = h.child.as_mut() {
                let mut flushed = false;
                if let Some(stdin) = child.stdin.as_mut() {
                    flushed = stdin
                        .write_all(&encode_message(&serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": u64::MAX,
                            "method": "shutdown",
                            "params": null,
                        })))
                        .await
                        .is_ok()
                        && stdin
                            .write_all(&encode_message(&serde_json::json!({
                                "jsonrpc": "2.0",
                                "method": "exit",
                            })))
                            .await
                        .is_ok()
                        && stdin.flush().await.is_ok();
                }
                if !flushed {
                    tracing::warn!("LSP client '{}' stdin closed — killing", h.id);
                }
            }
            if let Some(mut child) = h.child.take() {
                // Give the server up to 500 ms to exit on its own after `exit`.
                let _ = tokio::time::timeout(Duration::from_millis(500), child.wait()).await;
                let _ = child.kill().await;
            }
        }
    }

    /// Number of configured clients (exposed for diagnostics).
    pub fn len(&self) -> usize {
        self.clients.len()
    }

    pub fn is_empty(&self) -> bool {
        self.clients.is_empty()
    }
}

/// Drive one client's full lifecycle: spawn → handshake → watch → restart.
async fn run_client(handle: Arc<Mutex<LspClientHandle>>) {
    let mut backoff = BACKOFF_BASE;
    loop {
        let guard = handle.lock().await;
        if guard.shutdown_requested || guard.failed {
            return;
        }
        drop(guard);

        match spawn_and_handshake(&handle).await {
            Ok(child) => {
                let mut guard = handle.lock().await;
                guard.child = Some(child);
                guard.started_at = Instant::now();
                guard.last_error = None;
                let id = guard.id.clone();
                let command = guard.command.clone();
                let languages = guard.languages.join(", ");
                tracing::info!(
                    "LSP client '{}' started ({}) for [{}]",
                    id,
                    command,
                    languages
                );
                println!("   LSP: {} ({})", id.cyan().bold(), command);
                drop(guard);

                wait_for_exit(&handle).await;

                let mut guard = handle.lock().await;
                if guard.shutdown_requested {
                    return;
                }
                guard.child = None;
                guard.restarts += 1;
                if guard.started_at.elapsed() >= STABLE_UPTIME {
                    guard.restarts = 0;
                }
                let (restarts, max) = (guard.restarts, guard.max_restarts);
                let id = guard.id.clone();
                if restarts > max {
                    guard.failed = true;
                    guard.last_error = Some(format!(
                        "crashed {} times; giving up",
                        restarts
                    ));
                    println!(
                        "✖ LSP {} crashed {} times — giving up.",
                        id.red().bold(),
                        restarts
                    );
                    return;
                }
                drop(guard);
                println!(
                    " W LSP {} exited unexpectedly — restarting (attempt {}/{})",
                    id.yellow().bold(),
                    restarts,
                    max
                );
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(Duration::from_secs(8));
            }
            Err(e) => {
                let mut guard = handle.lock().await;
                if guard.shutdown_requested {
                    return;
                }
                guard.restarts += 1;
                guard.last_error = Some(e.clone());
                let (restarts, max) = (guard.restarts, guard.max_restarts);
                let id = guard.id.clone();
                if restarts > max {
                    guard.failed = true;
                    drop(guard);
                    println!(
                        "✖ LSP {} failed after {} attempts: {} — giving up.",
                        id.red().bold(),
                        restarts,
                        e
                    );
                    return;
                }
                drop(guard);
                tracing::warn!("LSP client '{}' failed to start: {}", id, e);
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(Duration::from_secs(8));
            }
        }
    }
}

/// Poll the client process until it exits (or shutdown is requested). The
/// watcher must poll rather than `wait()` so shutdown can intercede.
async fn wait_for_exit(handle: &Arc<Mutex<LspClientHandle>>) {
    loop {
        {
            let mut guard = handle.lock().await;
            if guard.shutdown_requested {
                return;
            }
            match guard.child.as_mut() {
                Some(child) => match child.try_wait() {
                    Ok(Some(_)) | Err(_) => {
                        guard.child = None;
                        return;
                    }
                    Ok(None) => {}
                },
                None => return,
            }
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// Spawn the server process and complete the LSP initialize handshake.
async fn spawn_and_handshake(
    handle: &Arc<Mutex<LspClientHandle>>,
) -> Result<Child, String> {
    let (command, args) = {
        let guard = handle.lock().await;
        (guard.command.clone(), guard.args.clone())
    };

    let mut child = Command::new(&command)
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("cannot spawn '{}': {}", command, e))?;

    let mut stdin = child.stdin.take().ok_or_else(|| "no stdin".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "no stdout".to_string())?;
    let mut reader = BufReader::new(stdout);

    let initialize = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "processId": std::process::id(),
            "clientInfo": { "name": "sentinel", "version": env!("CARGO_PKG_VERSION") },
            "capabilities": {
                "workspace": {
                    "workspaceFolders": true,
                    "didChangeWatchedFiles": { "dynamicRegistration": true }
                },
                "textDocument": {
                    "hover": { "dynamicRegistration": true },
                    "completion": { "dynamicRegistration": true }
                }
            },
            "rootUri": null,
            "workspaceFolders": null
        }
    });
    stdin
        .write_all(&encode_message(&initialize))
        .await
        .map_err(|e| format!("initialize write error: {}", e))?;
    stdin
        .flush()
        .await
        .map_err(|e| format!("initialize flush error: {}", e))?;

    let response = tokio::time::timeout(HANDSHAKE_TIMEOUT, read_frame(&mut reader))
        .await
        .map_err(|_| "initialize handshake timed out".to_string())?
        .map_err(|e| format!("initialize read error: {}", e))?
        .ok_or_else(|| "LSP server closed during initialize".to_string())?;

    if let Some(err) = response.get("error") {
        return Err(format!("initialize rejected: {}", err));
    }

    let initialized = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "initialized",
        "params": {}
    });
    stdin
        .write_all(&encode_message(&initialized))
        .await
        .map_err(|e| format!("initialized write error: {}", e))?;
    stdin
        .flush()
        .await
        .map_err(|e| format!("initialized flush error: {}", e))?;

    // Return stdin ownership to the Child so shutdown() can still write.
    child.stdin = Some(stdin);
    Ok(child)
}

/// Encode a JSON-RPC message using LSP's Content-Length framing.
fn encode_message(payload: &serde_json::Value) -> Vec<u8> {
    let body = payload.to_string();
    let mut out = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    out.extend_from_slice(body.as_bytes());
    out
}

/// Read one Content-Length framed message. Returns `None` on clean EOF.
async fn read_frame<R: tokio::io::AsyncBufRead + Unpin>(
    reader: &mut R,
) -> io::Result<Option<serde_json::Value>> {
    let mut length: Option<usize> = None;
    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            return Ok(None);
        }
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            break;
        }
        if let Some(value) = trimmed.strip_prefix("Content-Length:") {
            length = value.trim().parse::<usize>().ok();
        }
    }
    let length = length.ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "missing Content-Length header")
    })?;
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body).await?;
    serde_json::from_slice(&body)
        .map(Some)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

use colored::Colorize;

#[cfg(test)]
mod tests {
    use super::*;

    fn quick_exit_command() -> (String, Vec<String>) {
        if cfg!(windows) {
            ("cmd".into(), vec!["/C".into(), "exit".into(), "0".into()])
        } else {
            ("sh".into(), vec!["-c".into(), "exit 0".into()])
        }
    }

    #[test]
    fn frame_round_trips() {
        let payload = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": { "a": [1, 2, 3] },
        });
        let encoded = encode_message(&payload);
        let mut reader = BufReader::new(&encoded[..]);
        let decoded = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(read_frame(&mut reader))
            .unwrap()
            .expect("must decode");
        assert_eq!(decoded, payload);
    }

    #[test]
    fn frame_rejects_missing_content_length() {
        let mut reader = BufReader::new(&b"\r\n{}"[..]);
        let err = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(read_frame(&mut reader))
            .unwrap_err();
        assert!(err.to_string().contains("Content-Length"));
    }

    #[test]
    fn empty_config_starts_and_shuts_down_as_noop() {
        let config = SentinelConfig::default();
        let manager = LspManager::from_config(&config);
        assert!(manager.is_empty());
        manager.start();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(manager.shutdown());
    }

    #[tokio::test]
    async fn crashing_client_is_restarted_then_given_up() {
        let (command, args) = quick_exit_command();
        let mut config = SentinelConfig::default();
        config.lsp_servers = vec![sentinel_config::LspServerDef {
            id: "flaky".into(),
            command,
            args,
            languages: vec!["plaintext".into()],
        }];
        let manager = LspManager::from_config(&config);
        assert_eq!(manager.len(), 1);
        manager.start();

        let handle = manager.clients[0].clone();
        // A process that exits immediately can never handshake: give the
        // restart loop room to burn the whole budget.
        tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                let guard = handle.lock().await;
                if guard.failed {
                    assert!(
                        guard.restarts > 1,
                        "expected multiple restart attempts, got {}",
                        guard.restarts
                    );
                    assert!(
                        guard.last_error.is_some(),
                        "expected a recorded failure reason"
                    );
                    break;
                }
                drop(guard);
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .expect("client must be given up after max restarts");

        manager.shutdown().await;
    }
}
