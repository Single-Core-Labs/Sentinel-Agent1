use sentinel_ai_tui::ChatWidget;
use sentinel_ai_exec::ThreadEvent;
use serde_json::json;

#[test]
fn chatwidget_append_and_scroll() {
    let mut widget = ChatWidget::new();
    let ev = ThreadEvent::new("thinking", json!({"text": "thinking..."}));
    widget.append(ev);
    assert_eq!(widget.messages.len(), 1);
    assert_eq!(widget.messages[0].text, "thinking...");
}

#[test]
fn chatwidget_visible_messages() {
    let mut widget = ChatWidget::new();
    for i in 0..10 {
        let ev = ThreadEvent::new("thinking", json!({"text": format!("msg {i}")}));
        widget.append(ev);
    }
    let visible = widget.visible_messages(3);
    assert_eq!(visible.len(), 3);
    assert_eq!(visible[0].text, "msg 7");
    assert_eq!(visible[2].text, "msg 9");
}

#[test]
fn chatwidget_clear() {
    let mut widget = ChatWidget::new();
    widget.append(ThreadEvent::new("thinking", json!({"text": "a"})));
    widget.clear();
    assert!(widget.messages.is_empty());
}
