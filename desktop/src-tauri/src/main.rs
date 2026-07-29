#[cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;
use sentinel_app_server::RequestHandler;
use sentinel_config::SentinelConfig;
use sentinel_tools::ToolRegistry;
use sentinel_analytics::AnalyticsPipeline;

struct AppState {
    handler: Arc<RequestHandler>,
}

fn main() {
    let config = Arc::new(SentinelConfig::load().unwrap_or_default());
    let analytics = Arc::new(AnalyticsPipeline::new());
    let tools = {
        let mut reg = ToolRegistry::new();
        let headroom_retrieve = sentinel_headroom::integration::HeadroomRetrieveTool::new(
            Arc::new(sentinel_headroom::ccr::CcrStore::default())
        );
        reg.register(Arc::new(headroom_retrieve));
        Arc::new(reg)
    };
    let handler = Arc::new(RequestHandler::new(config, analytics, tools));

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppState { handler })
        .invoke_handler(tauri::generate_handler![
            chat,
            create_session,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[tauri::command]
async fn chat(state: tauri::State<'_, AppState>, session_id: String, message: String) -> Result<String, String> {
    let session = state.handler.get_session(&session_id).await
        .ok_or_else(|| "Session not found".to_string())?;
    session.chat(&message).await
}

#[tauri::command]
async fn create_session(state: tauri::State<'_, AppState>, model: Option<String>) -> Result<String, String> {
    let params = serde_json::json!({ "model": model });
    let result = state.handler.handle(sentinel_app_server_protocol::rpc::JsonRpcRequest {
        jsonrpc: "2.0".into(),
        id: serde_json::Value::Null,
        method: "session/create".into(),
        params: Some(params),
    }).await;

    match result.result {
        Some(val) => val["session_id"].as_str().map(String::from)
            .ok_or_else(|| "Missing session_id".to_string()),
        None => Err(result.error.map(|e| e.message).unwrap_or_else(|| "Unknown error".to_string())),
    }
}
