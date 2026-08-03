mod app;
mod app_event;
mod app_event_sender;
mod app_server_session;
mod chatwidget;
pub mod components;
pub mod display;
mod env_store;
mod event_bridge;
pub mod local_model;
mod model_picker;
mod provider_picker;
pub mod theme;

#[cfg(feature = "opentui-native")]
pub mod opentui_ffi;

pub use app::App;
pub use app_event::AppEvent;
pub use app_event_sender::AppEventSender;
pub use app_server_session::AppServerSession;
pub use chatwidget::{ChatWidget, DisplayEvent};
pub use components::box_widget;
pub use components::scrollbox::ScrollBox;
pub use components::text::Text;
pub use components::widget::{Widget, WidgetMut};
pub use model_picker::ModelPicker;
pub use provider_picker::ProviderPicker;
