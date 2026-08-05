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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn wait_shutdown_resolves_when_signalled() {
        let (tx, mut rx) = watch::channel(false);
        let waiter = tokio::spawn(async move { wait_shutdown(&mut rx).await });
        tx.send(true).unwrap();
        tokio::time::timeout(Duration::from_secs(1), waiter)
            .await
            .expect("wait_shutdown must resolve once signalled")
            .unwrap();
    }

    #[tokio::test]
    async fn wait_shutdown_resolves_when_already_signalled() {
        let (_tx, mut rx) = watch::channel(true);
        let waiter = tokio::spawn(async move { wait_shutdown(&mut rx).await });
        tokio::time::timeout(Duration::from_secs(1), waiter)
            .await
            .expect("pre-signalled receiver must resolve immediately")
            .unwrap();
    }

    #[tokio::test]
    async fn wait_shutdown_blocks_until_signalled() {
        let (tx, mut rx) = watch::channel(false);
        let waiter = tokio::spawn(async move { wait_shutdown(&mut rx).await });
        tokio::time::timeout(Duration::from_millis(100), async {
            assert!(!waiter.is_finished(), "must not resolve before signal");
        })
        .await
        .expect("waiter must stay pending before the signal");
        tx.send(true).unwrap();
        tokio::time::timeout(Duration::from_secs(1), waiter)
            .await
            .expect("must resolve after the signal")
            .unwrap();
    }

    #[tokio::test]
    async fn wait_shutdown_ignores_false_wakeups() {
        let (tx, mut rx) = watch::channel(false);
        let waiter = tokio::spawn(async move { wait_shutdown(&mut rx).await });
        tx.send(false).unwrap();
        tokio::time::timeout(Duration::from_millis(100), async {
            assert!(!waiter.is_finished());
        })
        .await
        .expect("false wakeup must not resolve the waiter");
        tx.send(true).unwrap();
        tokio::time::timeout(Duration::from_secs(1), waiter)
            .await
            .expect("must resolve after the real signal")
            .unwrap();
    }

    #[tokio::test]
    async fn wait_shutdown_never_resolves_without_signal() {
        let (_tx, mut rx) = watch::channel(false);
        tokio::time::timeout(Duration::from_millis(100), async move {
            wait_shutdown(&mut rx).await;
        })
        .await
        .expect_err("dropped sender without signal must never resolve");
    }

    #[tokio::test]
    async fn install_signal_handler_returns_receiver() {
        let rx = install_signal_handler();
        assert!(!*rx.borrow(), "initial state must be unsignalled");
        let _ = rx;
    }
}
