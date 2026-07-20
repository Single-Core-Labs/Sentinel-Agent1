use sentinel_ai_tui::ChatWidget;
use sentinel_ai_exec::ThreadEvent;
use serde_json::json;

#[test]
fn chatwidget_append_and_render() {
    let mut widget = ChatWidget::new();
    let ev = ThreadEvent::new("thinking", json!({"text": "thinking..."}));
    widget.append(ev);
    // Rendering should not panic.
    widget.render();
}
