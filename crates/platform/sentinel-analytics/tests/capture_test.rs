use sentinel_analytics::capture::AnalyticsDestination;
use sentinel_analytics::events::TrackEventRequest;

fn sample_event() -> TrackEventRequest {
    TrackEventRequest::new("test.event")
}

#[tokio::test]
async fn test_dispatch_to_null() {
    let dest = AnalyticsDestination::Null;
    assert!(dest.is_null());
    let result = dest.dispatch(&[sample_event()]).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_dispatch_to_file_creates_file() {
    let dir = std::env::temp_dir().join(format!("analytics-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let file_path = dir.join("events.jsonl");

    let dest = AnalyticsDestination::CaptureFile { path: file_path.clone() };
    assert!(!dest.is_null());

    let events = vec![
        TrackEventRequest::new("event.one"),
        TrackEventRequest::new("event.two"),
    ];
    dest.dispatch(&events).await.unwrap();

    let content = std::fs::read_to_string(&file_path).unwrap();
    let lines: Vec<&str> = content.trim().lines().collect();
    assert_eq!(lines.len(), 2);

    let parsed: Vec<serde_json::Value> = lines
        .iter()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(parsed[0]["event_type"], "event.one");
    assert_eq!(parsed[1]["event_type"], "event.two");

    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn test_dispatch_to_file_appends_to_existing() {
    let dir = std::env::temp_dir().join(format!("analytics-append-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let file_path = dir.join("append.jsonl");

    let dest = AnalyticsDestination::CaptureFile { path: file_path.clone() };
    dest.dispatch(&[TrackEventRequest::new("first")]).await.unwrap();
    dest.dispatch(&[TrackEventRequest::new("second")]).await.unwrap();

    let content = std::fs::read_to_string(&file_path).unwrap();
    assert_eq!(content.trim().lines().count(), 2);

    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn test_dispatch_to_file_creates_parent_dirs() {
    let dir = std::env::temp_dir().join(format!("analytics-nested-{}", uuid::Uuid::new_v4()));
    let file_path = dir.join("sub").join("nested").join("events.jsonl");

    let dest = AnalyticsDestination::CaptureFile { path: file_path.clone() };
    dest.dispatch(&[TrackEventRequest::new("test")]).await.unwrap();

    assert!(file_path.exists());
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn test_serialized_event_fields() {
    let event = TrackEventRequest::new("custom.event")
        .with_session("sess-1");
    let json = serde_json::to_value(&event).unwrap();
    assert_eq!(json["event_type"], "custom.event");
    assert_eq!(json["session_id"], "sess-1");
    assert!(json.get("id").is_some());
    assert!(json.get("timestamp").is_some());
    assert_eq!(json["metadata"], serde_json::Value::Null);
}

#[test]
fn test_capture_error_display() {
    let err = sentinel_analytics::capture::CaptureError::HttpError("timeout".into());
    assert!(err.to_string().contains("timeout"));
}
