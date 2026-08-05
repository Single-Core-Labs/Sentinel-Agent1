//! Graceful shutdown plumbing shared by all server entry points.
//!
//! A single background task watches for Ctrl-C (SIGINT; SIGTERM on Unix) and
//! flips a `watch` channel. Server loops poll that channel and exit cleanly,
//! letting in-flight connections finish and dropping subscriptions.

use tokio::sync::watch;

/// Spawn a background task that flips the returned receiver to `true` when the
/// process receives Ctrl-C. Callers pass this receiver into the
/// `*_with_shutdown` server entry points.
pub fn install_signal_handler() -> watch::Receiver<bool> {
    let (tx, rx) = watch::channel(false);
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        tracing::info!("shutdown signal received");
        let _ = tx.send(true);
    });
    rx
}

/// Resolves when the receiver flips to `true`. If the sender is dropped
/// (i.e. no signal handler was installed) this never resolves, so the server
/// keeps running until the process is killed.
pub async fn wait_shutdown(rx: &mut watch::Receiver<bool>) {
    if *rx.borrow() {
        return;
    }
    loop {
        match rx.changed().await {
            Ok(_) if *rx.borrow() => return,
            Ok(_) => continue,
            // Sender dropped without signalling: no handler installed, so the
            // server runs until the process is killed.
            Err(_) => tokio::time::sleep(std::time::Duration::from_secs(3600)).await,
        }
    }
}
