use sentinel_ai_exec::{EventProcessor, HumanProcessor, JsonlProcessor};
use sentinel_ai_exec::ThreadEvent;
use serde_json::json;

#[test]
fn human_processor_handles_events_without_error() {
    let processor = HumanProcessor::new();
    let ev = ThreadEvent::new("thinking", json!({"text": "testing"}));
    // Ensure processing does not panic and returns Ok.
    processor.process_event(&ev).expect("human processing failed");
}

#[test]
fn jsonl_processor_serializes_event() {
    let processor = JsonlProcessor::new();
    let ev = ThreadEvent::new("completed", json!({"text": "done"}));
    // Process should succeed; underlying println! writes to stdout – we only check result.
    processor.process_event(&ev).expect("jsonl processing failed");
}
