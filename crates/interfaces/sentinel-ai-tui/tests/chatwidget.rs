use sentinel_ai_exec::ThreadEvent;
use sentinel_ai_tui::{ChatWidget, DisplayEvent};
use serde_json::json;

#[test]
fn chatwidget_append_and_scroll() {
    let mut widget = ChatWidget::new();
    let ev = ThreadEvent::new("thinking", json!({"text": "thinking..."}));
    widget.append(ev);
    assert_eq!(widget.messages.len(), 1);
    if let DisplayEvent::Message(ref msg) = widget.messages[0] {
        assert_eq!(msg.text, "thinking...");
    } else {
        panic!("expected message event");
    }
}

#[test]
fn chatwidget_visible_messages() {
    let mut widget = ChatWidget::new();
    for i in 0..10 {
        let ev = ThreadEvent::new("thinking", json!({"text": format!("msg {i}")}));
        widget.append(ev);
    }
    let visible = widget.visible_events(3);
    assert_eq!(visible.len(), 3);
    if let DisplayEvent::Message(ref msg0) = visible[0] {
        assert_eq!(msg0.text, "msg 7");
    } else {
        panic!("expected message event");
    }
    if let DisplayEvent::Message(ref msg2) = visible[2] {
        assert_eq!(msg2.text, "msg 9");
    } else {
        panic!("expected message event");
    }
}

#[test]
fn chatwidget_clear() {
    let mut widget = ChatWidget::new();
    widget.append(ThreadEvent::new("thinking", json!({"text": "a"})));
    widget.clear();
    assert!(widget.messages.is_empty());
}
