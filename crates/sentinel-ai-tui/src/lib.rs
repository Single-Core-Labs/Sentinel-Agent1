mod app;
mod app_event;
mod app_event_sender;
mod app_server_session;
mod chatwidget;
mod model_picker;
pub mod display;
pub mod components;

#[cfg(feature = "opentui-native")]
pub mod opentui_ffi;

pub use app::App;
pub use app_event::AppEvent;
pub use app_event_sender::AppEventSender;
pub use app_server_session::AppServerSession;
pub use chatwidget::ChatWidget;
pub use model_picker::ModelPicker;
pub use components::widget::{Widget, WidgetMut};
pub use components::text::Text;
pub use components::scrollbox::ScrollBox;
pub use components::box_widget;
