use sentinel_analytics::capture::AnalyticsDestination;
use sentinel_analytics::client::AnalyticsEventsClient;
use sentinel_analytics::queue::AnalyticsQueueConfig;
use sentinel_analytics::fact::{AnalyticsFact, FactKind};

fn null_client() -> AnalyticsEventsClient {
    AnalyticsEventsClient::new(
        AnalyticsDestination::Null,
        AnalyticsQueueConfig::default(),
    )
}

#[tokio::test]
async fn test_null_client_does_not_panic() {
    let client = null_client();
    client.record_fact(AnalyticsFact::new(FactKind::Approval {
        action: "deploy".into(),
        granted: true,
    }));
}

#[tokio::test]
async fn test_record_skill_invocation() {
    let client = null_client();
    client.record_skill_invocation("review-pr", 1500, true);
}

#[tokio::test]
async fn test_record_plugin_usage() {
    let client = null_client();
    client.record_plugin_usage("vscode", "open_file");
}

#[tokio::test]
async fn test_record_code_change() {
    let client = null_client();
    client.record_code_change("src/main.rs", 10, 3);
}

#[tokio::test]
async fn test_record_tool_call() {
    let client = null_client();
    client.record_tool_call("bash", "t-1", 500, true);
}

#[tokio::test]
async fn test_record_turn_started_and_ended() {
    let client = null_client();
    client.record_turn_started("turn-1", "thread-1");
    client.record_turn_ended("turn-1", "thread-1", 100, 2000);
}

#[tokio::test]
async fn test_record_model_request_and_response() {
    let client = null_client();
    client.record_model_request("gpt-4", 150, 1024);
    client.record_model_response("gpt-4", 200, 3000);
}

#[tokio::test]
async fn test_record_client_request() {
    let client = null_client();
    client.record_client_request("POST", "/api/chat", 200, 500);
}

#[tokio::test]
async fn test_record_session_event() {
    let client = null_client();
    client.record_session_event("sess-1", "created");
}

#[tokio::test]
async fn test_record_approval() {
    let client = null_client();
    client.record_approval("deploy", true);
    client.record_approval("delete", false);
}

#[tokio::test]
async fn test_record_error() {
    let client = null_client();
    client.record_error("exec", "command not found");
}

#[tokio::test]
async fn test_record_server_notification() {
    let client = null_client();
    client.record_server_notification("ping", serde_json::json!({"ts": 123}));
}

#[tokio::test]
async fn test_record_turn_session() {
    let client = null_client();
    client.record_turn_session("turn-1", "thread-1", "session-1");
}
