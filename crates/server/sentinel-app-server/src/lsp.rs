//! LSP client lifecycle and workspace management.
//!
//! Language servers from the `[lsp_servers]` config section are started
//! asynchronously so they never block application startup. Each client:
//!
//! 1. spawns `command args` as a stdio subprocess;
//! 2. completes the LSP `initialize` / `initialized` handshake
//!    (Content-Length framed JSON-RPC), negotiating workspace file watching
//!    (`workspace/didChangeWatchedFiles` via dynamic registration or a static
//!    `fileWatchers` capability);
//! 3. runs a serve loop that (a) answers serverâ†’client requests so servers
//!    never hang on an unresponsive client, (b) streams filesystem change
//!    notifications from the workspace root (`context.paths`) to the server,
//!    and (c) detects failure — watcher errors or a dead/unresponsive child
//!    trigger a graceful `shutdown` + `exit`, after which the client is
//!    restarted from its original configuration with exponential backoff
//!    (bounded by `max_restarts`);
//! 4. is terminated gracefully on application shutdown (`shutdown` request +
//!    `exit` notification, then a hard kill if the server does not comply).
//!
//! The manager is created synchronously from config ([`LspManager::from_config`])
//! but does no I/O until [`LspManager::start`] is called, which returns
//! immediately and drives everything on background tasks.

use colored::Colorize;
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use sentinel_config::SentinelConfig;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;

/// LSP protocol version advertised to language servers.
pub const LSP_VERSION: &str = "3.17.0";

/// Default cap on consecutive crash-restarts before a client is given up on.
const DEFAULT_MAX_RESTARTS: u64 = 5;

/// Time allowed for the `initialize` handshake.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// Time allowed for the `client/registerCapability` round trip.
const REGISTRATION_TIMEOUT: Duration = Duration::from_secs(5);

/// Base backoff between crash restarts (doubled per attempt).
const BACKOFF_BASE: Duration = Duration::from_millis(250);

/// A client that stayed alive this long is considered stable; the restart
/// counter is reset so a single later crash does not burn the whole budget.
const STABLE_UPTIME: Duration = Duration::from_secs(30);

/// How long a client gets to exit after `shutdown` + `exit` before a kill.
const SHUTDOWN_GRACE: Duration = Duration::from_millis(600);

/// File watching mode negotiated with the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WatchMode {
    /// Server cannot consume `workspace/didChangeWatchedFiles`.
    Unsupported,
    /// Server advertises static `fileWatchers`; notifications are sent as-is.
    Direct,
    /// Server supports dynamic registration; the client registered interest
    /// via `client/registerCapability`.
    Registered,
}

/// Per-client lifecycle state, shared between the serve task and shutdown.
struct LspClientHandle {
    id: String,
    command: String,
    args: Vec<String>,
    languages: Vec<String>,
    max_restarts: u64,
    child: Option<Child>,
    /// Extra environment for the spawned server process (empty in production;
    /// used by tests to point a fake server at its log file).
    env: Vec<(String, String)>,
    /// `ChildStdin` kept outside `Child` so both the serve task and
    /// [`LspManager::shutdown`] can write (watcher notifications, replies,
    /// `shutdown`/`exit`) without racing for one owned handle.
    stdin: Option<Arc<Mutex<ChildStdin>>>,
    workspace_root: PathBuf,
    restarts: u64,
    failed: bool,
    shutdown_requested: bool,
    /// Set when the serve loop had to take the client down (watcher error,
    /// unresponsive child) — the restart loop then reuses the original
    /// configuration.
    restart_requested: bool,
    last_error: Option<String>,
    started_at: Instant,
}

impl LspClientHandle {
    fn from_def(def: &sentinel_config::LspServerDef, workspace_root: PathBuf) -> Self {
        Self {
            id: def.id.clone(),
            command: def.command.clone(),
            args: def.args.clone(),
            languages: def.languages.clone(),
            max_restarts: DEFAULT_MAX_RESTARTS,
            child: None,
            env: Vec::new(),
            stdin: None,
            workspace_root,
            restarts: 0,
            failed: false,
            shutdown_requested: false,
            restart_requested: false,
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
        let root = workspace_root(config);
        let clients = config
            .lsp_servers
            .iter()
            .map(|def| Arc::new(Mutex::new(LspClientHandle::from_def(def, root.clone()))))
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
    /// the server a brief window, then kill. Also requests each serve task to
    /// stop so it cannot restart a client behind a closing app.
    pub async fn shutdown(&self) {
        for handle in &self.clients {
            let mut h = handle.lock().await;
            h.shutdown_requested = true;
            if let Some(stdin) = h.stdin.clone() {
                let flushed = send_shutdown_and_exit(&stdin).await;
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

/// Resolve the directory whose files are forwarded to every LSP server as
/// `workspace/didChangeWatchedFiles` notifications. Uses the first
/// `context.paths` entry (default `.`), made absolute against the CWD.
fn workspace_root(config: &SentinelConfig) -> PathBuf {
    let first = config
        .context
        .paths
        .first()
        .cloned()
        .unwrap_or_else(|| ".".into());
    let root = PathBuf::from(first);
    let root = if root.is_absolute() {
        root
    } else {
        std::env::current_dir().unwrap_or_default().join(root)
    };
    std::fs::canonicalize(&root).unwrap_or(root)
}

/// Drive one client's full lifecycle: spawn â†’ handshake â†’ serve â†’ restart.
async fn run_client(handle: Arc<Mutex<LspClientHandle>>) {
    let mut backoff = BACKOFF_BASE;
    loop {
        let guard = handle.lock().await;
        if guard.shutdown_requested || guard.failed {
            return;
        }
        drop(guard);

        match spawn_and_handshake(&handle).await {
            Ok((child, reader, mode)) => {
                {
                    let mut guard = handle.lock().await;
                    guard.child = Some(child);
                    guard.restart_requested = false;
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
                }

                serve_client(&handle, reader, mode).await;

                let mut guard = handle.lock().await;
                if guard.shutdown_requested {
                    return;
                }
                guard.child = None;
                let restart_reason = guard.last_error.clone().unwrap_or_else(|| {
                    "exited unexpectedly".to_string()
                });
                guard.restarts += 1;
                if guard.started_at.elapsed() >= STABLE_UPTIME {
                    guard.restarts = 0;
                }
                let (restarts, max) = (guard.restarts, guard.max_restarts);
                let id = guard.id.clone();
                if restarts > max {
                    guard.failed = true;
                    guard.last_error = Some(format!("crashed {} times; giving up", restarts));
                    println!(
                        "âœ– LSP {} crashed {} times — giving up.",
                        id.red().bold(),
                        restarts
                    );
                    return;
                }
                drop(guard);
                println!(
                    " W LSP {} {} — restarting (attempt {}/{})",
                    id.yellow().bold(),
                    restart_reason,
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
                        "âœ– LSP {} failed after {} attempts: {} — giving up.",
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

/// Serve one live client: answer serverâ†’client requests, forward filesystem
/// changes to the server as `workspace/didChangeWatchedFiles`, and detect
/// failure (watcher errors, unresponsive child). Returns when the child
/// exits, the watcher fails fatally, or shutdown is requested.
async fn serve_client(
    handle: &Arc<Mutex<LspClientHandle>>,
    mut reader: BufReader<ChildStdout>,
    mode: WatchMode,
) {
    let workspace_root = handle.lock().await.workspace_root.clone();

    // Filesystem watcher feeding an async channel via a std bridge (the
    // notify callback runs on its own thread).
    let (std_tx, std_rx) = std::sync::mpsc::channel::<notify::Result<notify::Event>>();
    let (watch_tx, mut watch_rx) = tokio::sync::mpsc::channel::<notify::Result<notify::Event>>(256);
    tokio::task::spawn_blocking(move || {
        while let Ok(event) = std_rx.recv() {
            if watch_tx.blocking_send(event).is_err() {
                break;
            }
        }
    });

    let mut _watcher: Option<RecommendedWatcher> = None;
    if mode != WatchMode::Unsupported {
        let tx = std_tx.clone();
        match RecommendedWatcher::new(
            move |result: notify::Result<notify::Event>| {
                let _ = tx.send(result);
            },
            notify::Config::default(),
        ) {
            Ok(mut w) => match w.watch(&workspace_root, RecursiveMode::Recursive) {
                Ok(()) => {
                    tracing::info!(
                        "LSP '{}' watching {}",
                        handle.lock().await.id,
                        workspace_root.display()
                    );
                    _watcher = Some(w);
                }
                Err(e) => {
                    tracing::warn!(
                        "LSP '{}' cannot watch {}: {} — file watching disabled",
                        handle.lock().await.id,
                        workspace_root.display(),
                        e
                    );
                }
            },
            Err(e) => {
                tracing::warn!(
                    "LSP '{}' watcher creation failed: {} — file watching disabled",
                    handle.lock().await.id,
                    e
                );
            }
        }
    }

    let mut interval = tokio::time::interval(Duration::from_millis(400));
    loop {
        tokio::select! {
            frame = read_frame(&mut reader) => {
                match frame {
                    Ok(Some(message)) => {
                        if let Some(id) = message.get("id") {
                            // Server â†’ client request: reply so the server
                            // never hangs waiting on an unresponsive client.
                            let reply = serde_json::json!({
                                "jsonrpc": "2.0",
                                "id": id,
                                "result": null,
                            });
                            if !send_message(handle, &reply).await {
                                restart_gracefully(handle, "client unresponsive (reply write failed)").await;
                                break;
                            }
                        }
                    }
                    Ok(None) => break, // stdout EOF: the server process is gone
                    Err(e) => {
                        tracing::warn!("LSP '{}' protocol error: {}", handle.lock().await.id, e);
                        break;
                    }
                }
            }
            event = watch_rx.recv() => {
                match event {
                    Some(Ok(notify_event)) => {
                        if mode == WatchMode::Unsupported {
                            continue;
                        }
                        let mut changes = Vec::new();
                        for path in &notify_event.paths {
                            if let Some(kind) = change_kind(&notify_event.kind) {
                                changes.push(serde_json::json!({
                                    "uri": path_to_file_uri(path),
                                    "type": kind,
                                }));
                            }
                        }
                        if changes.is_empty() {
                            continue;
                        }
                        let payload = serde_json::json!({
                            "jsonrpc": "2.0",
                            "method": "workspace/didChangeWatchedFiles",
                            "params": { "changes": changes },
                        });
                        if !send_message(handle, &payload).await {
                            restart_gracefully(handle, "client unresponsive (watch write failed)").await;
                            break;
                        }
                    }
                    Some(Err(e)) => {
                        // The workspace watcher failed (e.g. the watched
                        // directory vanished): take the client down and let
                        // the restart loop relaunch from its config.
                        tracing::warn!("LSP '{}' workspace watcher error: {}", handle.lock().await.id, e);
                        restart_gracefully(handle, &format!("workspace watcher error: {e}")).await;
                        break;
                    }
                    None => break, // watcher bridge closed
                }
            }
            _ = interval.tick() => {
                let mut guard = handle.lock().await;
                if guard.shutdown_requested {
                    break;
                }
                let exited = match guard.child.as_mut() {
                    Some(child) => child.try_wait().map(|status| status.is_some()).unwrap_or(true),
                    None => true,
                };
                if exited {
                    break;
                }
            }
        }
    }
}

/// Gracefully take a client down (watcher error or unresponsive child):
/// send `shutdown` + `exit`, wait [`SHUTDOWN_GRACE`], then kill. Marks the
/// handle so the restart loop relaunches with the original configuration.
async fn restart_gracefully(handle: &Arc<Mutex<LspClientHandle>>, reason: &str) {
    let stdin = handle.lock().await.stdin.clone();
    if let Some(stdin) = stdin {
        send_shutdown_and_exit(&stdin).await;
    }
    let deadline = Instant::now() + SHUTDOWN_GRACE;
    loop {
        let exited = {
            let mut guard = handle.lock().await;
            match guard.child.as_mut() {
                Some(child) => child.try_wait().map(|s| s.is_some()).unwrap_or(true),
                None => true,
            }
        };
        if exited || Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let mut guard = handle.lock().await;
    if let Some(mut child) = guard.child.take() {
        let _ = child.kill().await;
    }
    guard.restart_requested = true;
    guard.last_error = Some(reason.to_string());
}

/// Spawn the server process, complete the LSP initialize handshake, negotiate
/// file watching, and hand back the live child, its stdout reader, and the
/// negotiated watch mode. `child.stdin` is `None` afterwards — the pipe lives
/// on in `handle.stdin` so the serve task and shutdown can share it.
async fn spawn_and_handshake(
    handle: &Arc<Mutex<LspClientHandle>>,
) -> Result<(Child, BufReader<ChildStdout>, WatchMode), String> {
    let (command, args, workspace_root, id) = {
        let guard = handle.lock().await;
        (
            guard.command.clone(),
            guard.args.clone(),
            guard.workspace_root.clone(),
            guard.id.clone(),
        )
    };
    let env = handle.lock().await.env.clone();

    let mut child = Command::new(&command)
        .args(&args)
        .envs(env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("cannot spawn '{}': {}", command, e))?;

    let stdin = child.stdin.take().ok_or_else(|| "no stdin".to_string())?;
    let stdin = Arc::new(Mutex::new(stdin));
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "no stdout".to_string())?;
    let mut reader = BufReader::new(stdout);

    let root_uri = path_to_file_uri(&workspace_root);
    let folder_name = workspace_root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "workspace".into());

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
            "rootUri": root_uri,
            "workspaceFolders": [{
                "uri": root_uri,
                "name": folder_name
            }]
        }
    });
    write_message(&stdin, &initialize)
        .await
        .map_err(|e| format!("initialize write error: {e}"))?;

    let response = tokio::time::timeout(HANDSHAKE_TIMEOUT, read_frame(&mut reader))
        .await
        .map_err(|_| "initialize handshake timed out".to_string())?
        .map_err(|e| format!("initialize read error: {e}"))?
        .ok_or_else(|| "LSP server closed during initialize".to_string())?;

    if let Some(err) = response.get("error") {
        return Err(format!("initialize rejected: {err}"));
    }

    let mut mode = watch_mode_from_capabilities(&response["result"]["capabilities"]);
    if mode == WatchMode::Registered {
        let registration = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "client/registerCapability",
            "params": {
                "registrations": [{
                    "id": format!("sentinel-fs-watch-{id}"),
                    "method": "workspace/didChangeWatchedFiles",
                    "registerOptions": {
                        "watchers": [{ "globPattern": "**/*" }]
                    }
                }]
            }
        });
        if write_message(&stdin, &registration).await.is_err() {
            mode = WatchMode::Unsupported;
        } else {
            match tokio::time::timeout(REGISTRATION_TIMEOUT, read_frame(&mut reader)).await {
                Ok(Ok(Some(resp))) if resp.get("error").is_none() => {
                    tracing::debug!("LSP '{}' registered file watching", id);
                }
                Ok(Ok(Some(resp))) => {
                    tracing::warn!(
                        "LSP '{}' rejected file-watch registration: {} — watching disabled",
                        id,
                        resp["error"]
                    );
                    mode = WatchMode::Unsupported;
                }
                _ => {
                    tracing::warn!(
                        "LSP '{}' did not confirm file-watch registration — watching disabled",
                        id
                    );
                    mode = WatchMode::Unsupported;
                }
            }
        }
    }

    let initialized = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "initialized",
        "params": {}
    });
    write_message(&stdin, &initialized)
        .await
        .map_err(|e| format!("initialized write error: {e}"))?;

    handle.lock().await.stdin = Some(Arc::clone(&stdin));
    Ok((child, reader, mode))
}

/// Write and flush a framed message to the shared stdin.
async fn write_message(
    stdin: &Arc<Mutex<ChildStdin>>,
    payload: &serde_json::Value,
) -> io::Result<()> {
    let mut guard = stdin.lock().await;
    guard.write_all(&encode_message(payload)).await?;
    guard.flush().await
}

/// Send a message through the handle's shared stdin. `false` means the pipe
/// is gone (server dead or unresponsive).
async fn send_message(handle: &Arc<Mutex<LspClientHandle>>, payload: &serde_json::Value) -> bool {
    let stdin = handle.lock().await.stdin.clone();
    let Some(stdin) = stdin else {
        return false;
    };
    write_message(&stdin, payload).await.is_ok()
}

/// Best-effort graceful termination: `shutdown` request, then `exit`.
async fn send_shutdown_and_exit(stdin: &Arc<Mutex<ChildStdin>>) -> bool {
    let shutdown = serde_json::json!({
        "jsonrpc": "2.0",
        "id": u64::MAX,
        "method": "shutdown",
        "params": null,
    });
    let exit = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "exit",
    });
    write_message(stdin, &shutdown).await.is_ok() && write_message(stdin, &exit).await.is_ok()
}

/// Decide how to watch the workspace from the server's initialize response.
fn watch_mode_from_capabilities(capabilities: &serde_json::Value) -> WatchMode {
    let watch = &capabilities["workspace"]["didChangeWatchedFiles"];
    match watch {
        serde_json::Value::Object(fields) => {
            if fields.get("dynamicRegistration").and_then(|v| v.as_bool()) == Some(true) {
                WatchMode::Registered
            } else if fields.get("fileWatchers").is_some() {
                WatchMode::Direct
            } else {
                WatchMode::Unsupported
            }
        }
        _ => WatchMode::Unsupported,
    }
}

/// LSP change type for a filesystem event: Create=1, Modify=2, Delete=3.
fn change_kind(kind: &EventKind) -> Option<u32> {
    match kind {
        EventKind::Create(_) => Some(1),
        EventKind::Modify(_) => Some(2),
        EventKind::Remove(_) => Some(3),
        // Access and Other events carry no indexable change.
        EventKind::Access(_) | EventKind::Other | EventKind::Any => None,
    }
}

/// Convert a filesystem path to a `file://` URI (percent-encoded, forward
/// slashes), the form LSP `workspace/didChangeWatchedFiles` expects.
fn path_to_file_uri(path: &Path) -> String {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    };
    let mut raw = absolute.to_string_lossy().replace('\\', "/");
    if !raw.starts_with('/') {
        raw.insert(0, '/');
    }
    let mut uri = String::with_capacity(raw.len() + 16);
    uri.push_str("file://");
    for byte in raw.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' | b':' => {
                uri.push(byte as char);
            }
            _ => {
                uri.push('%');
                uri.push_str(&format!("{byte:02X}"));
            }
        }
    }
    uri
}

/// Encode a JSON-RPC message using LSP's Content-Length framing.
fn encode_message(payload: &serde_json::Value) -> Vec<u8> {
    let body = payload.to_string();
    let mut out = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    out.extend_from_slice(body.as_bytes());
    out
}

/// Read one Content-Length framed message. Returns `None` on clean EOF
/// (no headers received before the stream closed).
async fn read_frame<R: tokio::io::AsyncBufRead + Unpin>(
    reader: &mut R,
) -> io::Result<Option<serde_json::Value>> {
    let mut length: Option<usize> = None;
    let mut saw_any = false;
    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            return match length {
                Some(_) => Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "truncated Content-Length body",
                )),
                None if saw_any => Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "missing Content-Length header",
                )),
                None => Ok(None),
            };
        }
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            // Stray blank line before the headers completed (some processes
            // print noise on stdout): skip it rather than failing framing.
            if length.is_some() {
                break;
            }
            continue;
        }
        saw_any = true;
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
    fn uri_percent_encodes_spaces_and_non_ascii() {
        #[cfg(windows)]
        let uri = path_to_file_uri(Path::new(r"C:\Temp\a b.rs"));
        #[cfg(not(windows))]
        let uri = path_to_file_uri(Path::new("/tmp/a b.rs"));
        assert!(uri.starts_with("file:///"));
        assert!(uri.contains("/a%20b.rs"), "got {uri}");
    }

    #[test]
    fn uri_handles_relative_paths() {
        let uri = path_to_file_uri(Path::new("src/lib.rs"));
        assert!(uri.starts_with("file:///"), "got {uri}");
        assert!(uri.ends_with("/src/lib.rs"), "got {uri}");
    }

    #[test]
    fn watch_mode_negotiation_matches_capabilities() {
        let dynamic = serde_json::json!({
            "workspace": { "didChangeWatchedFiles": { "dynamicRegistration": true } }
        });
        assert_eq!(
            watch_mode_from_capabilities(&dynamic),
            WatchMode::Registered
        );

        let static_watchers = serde_json::json!({
            "workspace": { "didChangeWatchedFiles": { "fileWatchers": [{}] } }
        });
        assert_eq!(
            watch_mode_from_capabilities(&static_watchers),
            WatchMode::Direct
        );

        let none = serde_json::json!({ "workspace": {} });
        assert_eq!(watch_mode_from_capabilities(&none), WatchMode::Unsupported);

        assert_eq!(
            watch_mode_from_capabilities(&serde_json::json!(null)),
            WatchMode::Unsupported
        );
    }

    #[test]
    fn change_kinds_map_to_lsp_types() {
        use notify::event::{AccessKind, CreateKind, DataChange, ModifyKind, RemoveKind};

        assert_eq!(
            change_kind(&EventKind::Create(CreateKind::File)),
            Some(1)
        );
        assert_eq!(
            change_kind(&EventKind::Modify(ModifyKind::Data(DataChange::Any))),
            Some(2)
        );
        assert_eq!(
            change_kind(&EventKind::Remove(RemoveKind::File)),
            Some(3)
        );
        assert_eq!(change_kind(&EventKind::Access(AccessKind::Read)), None);
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
        let config = SentinelConfig {
            lsp_servers: vec![sentinel_config::LspServerDef {
                id: "flaky".into(),
                command,
                args,
                languages: vec!["plaintext".into()],
            }],
            ..SentinelConfig::default()
        };
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

    /// Standalone fake LSP server. Only runs when spawned by the tests below
    /// (`SENTINEL_FAKE_LSP=1`): answers `initialize` with dynamic file-watch
    /// registration, replies to every request with `null`, and logs received
    /// messages to `FAKE_LSP_LOG` (one line each).
    #[test]
    fn fake_lsp_server_main() {
        use std::io::{BufRead, Read, Write};

        if std::env::var("SENTINEL_FAKE_LSP").as_deref() != Ok("1") {
            return;
        }
        let log_path = std::env::var("FAKE_LSP_LOG").expect("FAKE_LSP_LOG must be set");
        let log_line = |line: &str| {
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path)
                .expect("open log");
            writeln!(file, "{line}").expect("write log");
        };

        let mut stdin = std::io::stdin().lock();
        let mut stdout = std::io::stdout().lock();
        let mut buffer = Vec::new();
        loop {
            let mut length = None;
            loop {
                buffer.clear();
                if stdin.read_until(b'\n', &mut buffer).expect("read header") == 0 {
                    return;
                }
                let line = String::from_utf8_lossy(&buffer);
                let line = line.trim_end();
                if line.is_empty() {
                    break;
                }
                if let Some(value) = line.strip_prefix("Content-Length:") {
                    length = value.trim().parse::<usize>().ok();
                }
            }
            let mut body = vec![0u8; length.expect("Content-Length")];
            stdin.read_exact(&mut body).expect("read body");
            let message: serde_json::Value =
                serde_json::from_slice(&body).expect("parse body");
            let method = message
                .get("method")
                .and_then(|m| m.as_str())
                .unwrap_or("");
            let has_id = message.get("id").is_some();

            match (method, has_id) {
                ("initialize", true) => {
                    let response = serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": message["id"],
                        "result": {
                            "capabilities": {
                                "workspace": {
                                    "didChangeWatchedFiles": { "dynamicRegistration": true }
                                }
                            },
                            "serverInfo": { "name": "fake-lsp" }
                        }
                    });
                    stdout.write_all(&encode_message(&response)).expect("write");
                    stdout.flush().expect("flush");
                    log_line("initialize");
                }
                ("client/registerCapability", true) => {
                    let response = serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": message["id"],
                        "result": null,
                    });
                    stdout.write_all(&encode_message(&response)).expect("write");
                    stdout.flush().expect("flush");
                    log_line("registerCapability");
                }
                ("shutdown", true) => {
                    let response = serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": message["id"],
                        "result": null,
                    });
                    stdout.write_all(&encode_message(&response)).expect("write");
                    stdout.flush().expect("flush");
                    log_line("shutdown");
                    return;
                }
                ("exit", false) => {
                    log_line("exit");
                    return;
                }
                ("workspace/didChangeWatchedFiles", false) => {
                    let count = message["params"]["changes"]
                        .as_array()
                        .map(|a| a.len())
                        .unwrap_or(0);
                    log_line(&format!("didChangeWatchedFiles:{count}"));
                }
                (other, true) => {
                    let response = serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": message["id"],
                        "result": null,
                    });
                    stdout.write_all(&encode_message(&response)).expect("write");
                    stdout.flush().expect("flush");
                    log_line(&format!("request:{other}"));
                }
                (other, false) => {
                    log_line(&format!("notif:{other}"));
                }
            }
        }
    }

    /// End-to-end: a fake LSP server over stdio completes the handshake, the
    /// workspace watcher forwards real filesystem changes as
    /// `workspace/didChangeWatchedFiles`, and application shutdown reaches
    /// the server as a graceful `shutdown` request.
    #[tokio::test]
    async fn workspace_changes_are_forwarded_and_shutdown_is_graceful() {
        let root = std::env::temp_dir().join(format!(
            "sentinel-lsp-watch-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&root).expect("create watch root");
        let log = std::env::temp_dir().join(format!(
            "sentinel-lsp-fake-{}.log",
            uuid::Uuid::new_v4().simple()
        ));

        let config = SentinelConfig {
            context: sentinel_config::ContextSettings {
                paths: vec![root.to_string_lossy().into_owned()],
                ..sentinel_config::ContextSettings::default()
            },
            lsp_servers: vec![sentinel_config::LspServerDef {
                id: "fake".into(),
                command: std::env::current_exe()
                    .expect("current exe")
                    .to_string_lossy()
                    .into_owned(),
                args: vec![
                    "--exact".into(),
                    "lsp::tests::fake_lsp_server_main".into(),
                    "--nocapture".into(),
                ],
                languages: vec!["rust".into()],
            }],
            ..SentinelConfig::default()
        };

        let manager = LspManager::from_config(&config);
        // Point the fake server at its log and enable server mode in the
        // spawned child only — the parent test process must NOT inherit it,
        // or its own run of `fake_lsp_server_main` would block on stdin.
        manager.clients[0].lock().await.env = vec![
            ("SENTINEL_FAKE_LSP".into(), "1".into()),
            ("FAKE_LSP_LOG".into(), log.to_string_lossy().into_owned()),
        ];
        manager.start();

        // Wait for the client to come up (handshake + registration complete).
        let handle = manager.clients[0].clone();
        tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                let guard = handle.lock().await;
                let up = guard.stdin.is_some() && guard.last_error.is_none();
                drop(guard);
                if up {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .expect("fake client must come up");

        // Give the serve task a moment to attach the filesystem watcher, then
        // touch a file and wait for the server to see it.
        tokio::time::sleep(Duration::from_millis(500)).await;
        let watched_file = root.join("hello.rs");
        std::fs::write(&watched_file, "fn main() {}\n").expect("write watched file");

        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let log_content = std::fs::read_to_string(&log).unwrap_or_default();
                if log_content.contains("didChangeWatchedFiles:") {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .expect("server must receive didChangeWatchedFiles");

        manager.shutdown().await;

        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let log_content = std::fs::read_to_string(&log).unwrap_or_default();
                if log_content.contains("shutdown") {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .expect("server must receive graceful shutdown");

        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_file(&log);
    }
}
