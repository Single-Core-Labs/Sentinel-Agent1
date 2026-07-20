//! Minimal Codex‑style Terminal UI (TUI).
//!
//! This crate implements a lightweight, event‑driven TUI that mirrors the
//! architecture described in the Codex‑rs `tui` module. It uses a mock backend
//! (`codex_exec::MockClient`) for demonstration purposes.

mod app;
mod app_event;
mod app_event_sender;
mod app_server_session;
mod chatwidget;

pub use app::App;
pub use app_event::AppEvent;
pub use app_event_sender::AppEventSender;
pub use app_server_session::AppServerSession;
pub use chatwidget::ChatWidget;
