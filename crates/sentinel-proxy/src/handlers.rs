use std::sync::Arc;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode, Method},
    response::{IntoResponse, Json, Response},
    routing::{get, post},
    Router,
};
use serde_json::{json, Value};
use tracing::error;

use crate::compression::ProxyCompressor;
use crate::config::ProxyConfig;
use crate::stats::SharedStats;

#[derive(Clone)]
pub struct AppState {
    pub compressor: Arc<ProxyCompressor>,
    pub stats: SharedStats,
    pub config: ProxyConfig,
    pub client: reqwest::Client,
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health_handler))
        .route("/stats", get(stats_handler))
        .route("/metrics", get(metrics_handler))
        .route("/v1/chat/completions", post(openai_chat_handler))
        .route("/v1/messages", post(anthropic_messages_handler))
        .route("/v1/compress", post(compress_handler))
        .with_state(state)
}

async fn health_handler(State(state): State<AppState>) -> Json<Value> {
    let health = state.stats.health(state.config.optimize);
    let s = state.stats.snapshot();
    Json(json!({
        "status": health.status,
        "optimize": health.optimize,
        "uptime_seconds": health.uptime_seconds,
        "stats": {
            "total_requests": s.total_requests,
            "tokens_saved": s.tokens_saved,
            "savings_percent": s.savings_percent,
        }
    }))
}

async fn stats_handler(State(state): State<AppState>) -> Json<Value> {
    let s = state.stats.snapshot();
    Json(json!({
        "session": {
            "total_requests": s.total_requests,
            "tokens_before": s.tokens_before,
            "tokens_after": s.tokens_after,
            "tokens_saved": s.tokens_saved,
            "savings_percent": s.savings_percent,
            "cache_hits": s.cache_hits,
            "cache_misses": s.cache_misses,
            "errors": s.errors,
            "started_at": s.started_at,
        },
        "persistent_savings": {
            "total_tokens_saved": s.tokens_saved,
            "total_requests": s.total_requests,
        }
    }))
}

async fn metrics_handler(State(state): State<AppState>) -> String {
    state.stats.metrics_text()
}

fn should_bypass(headers: &HeaderMap) -> bool {
    headers
        .get("x-headroom-bypass")
        .and_then(|v| v.to_str().ok())
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false)
}

fn build_proxy_headers(headers: &HeaderMap) -> HeaderMap {
    let mut out = HeaderMap::new();
    for (key, value) in headers.iter() {
        let key_str = key.as_str().to_lowercase();
        if key_str.starts_with("x-") && key_str != "x-headroom-bypass" {
            out.insert(key.clone(), value.clone());
        }
        if key_str == "authorization" {
            out.insert(key.clone(), value.clone());
        }
        if key_str == "anthropic-version" {
            out.insert(key.clone(), value.clone());
        }
        if key_str == "content-type" {
            out.insert(key.clone(), value.clone());
        }
    }
    if !out.contains_key("content-type") {
        out.insert(
            axum::http::header::CONTENT_TYPE,
            "application/json".parse().unwrap(),
        );
    }
    out
}

fn check_budget(config: &ProxyConfig, tokens_after: u64) -> Option<Response> {
    if let Some(budget_usd) = config.budget {
        let estimated_cost = tokens_after as f64 * 0.00001;
        if estimated_cost > budget_usd {
            error!("Budget exceeded: ${:.4} > ${:.2}", estimated_cost, budget_usd);
            return Some((
                StatusCode::TOO_MANY_REQUESTS,
                Json(json!({
                    "error": "Budget exceeded",
                    "estimated_cost": estimated_cost,
                    "budget": budget_usd,
                })),
            ).into_response());
        }
    }
    None
}

async fn proxy_and_inject_meta(
    state: &AppState,
    headers: &HeaderMap,
    target_url: &str,
    body: Value,
    tokens_before: u64,
    tokens_after: u64,
) -> Response {
    let proxy_headers = build_proxy_headers(headers);
    let client_req = state
        .client
        .request(Method::POST, target_url)
        .headers(proxy_headers)
        .json(&body);

    match client_req.send().await {
        Ok(resp) => {
            let status = resp.status();
            let resp_headers = resp.headers().clone();
            let resp_body = resp.bytes().await.unwrap_or_default();

            let mut response = Response::new(axum::body::Body::from(resp_body));
            *response.status_mut() = status;
            for (key, value) in resp_headers.iter() {
                response.headers_mut().insert(key.clone(), value.clone());
            }

            let proxy_meta = json!({
                "tokens_before": tokens_before,
                "tokens_after": tokens_after,
                "tokens_saved": tokens_before.saturating_sub(tokens_after),
                "compression_ratio": if tokens_before > 0 {
                    (tokens_before - tokens_after) as f64 / tokens_before as f64
                } else {
                    0.0
                },
            });

            let (parts, body) = response.into_parts();
            let body_bytes = axum::body::to_bytes(body, 1024 * 1024).await.unwrap_or_default();
            if let Ok(mut resp_json) = serde_json::from_slice::<Value>(&body_bytes) {
                resp_json["proxy_metadata"] = proxy_meta;
                let new_body = serde_json::to_vec(&resp_json).unwrap_or(body_bytes.to_vec());
                Response::from_parts(parts, axum::body::Body::from(new_body))
            } else {
                Response::from_parts(parts, axum::body::Body::from(body_bytes))
            }
        }
        Err(e) => {
            error!("Proxy request failed: {}", e);
            state.stats.record_error();
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": format!("Upstream request failed: {}", e)})),
            )
                .into_response()
        }
    }
}

async fn openai_chat_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    let bypass = should_bypass(&headers);

    let model = body
        .get("model")
        .and_then(|m| m.as_str())
        .unwrap_or("gpt-4o");
    let messages = body
        .get("messages")
        .and_then(|m| m.as_array())
        .cloned()
        .unwrap_or_default();

    let (compressed_msgs, tokens_before, tokens_after) = if !bypass && state.config.optimize {
        state
            .compressor
            .compress_json_messages(&messages, model)
            .await
    } else {
        let tokens: u64 = messages
            .iter()
            .filter_map(|m| m["content"].as_str())
            .map(|c| ProxyCompressor::estimate_tokens(c))
            .sum();
        (messages, tokens, tokens)
    };

    if let Some(budget_resp) = check_budget(&state.config, tokens_after) {
        return budget_resp;
    }

    let mut altered_body = body.clone();
    altered_body["messages"] = Value::Array(compressed_msgs);

    let target_url = format!("{}/v1/chat/completions", state.config.openai_api_url);
    proxy_and_inject_meta(&state, &headers, &target_url, altered_body, tokens_before, tokens_after).await
}

async fn anthropic_messages_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    let bypass = should_bypass(&headers);

    let model = body
        .get("model")
        .and_then(|m| m.as_str())
        .unwrap_or("claude-sonnet-4-20250514");
    let messages = body
        .get("messages")
        .and_then(|m| m.as_array())
        .cloned()
        .unwrap_or_default();
    let system = body.get("system").and_then(|s| s.as_str());

    let all_msgs: Vec<Value> = if let Some(sys) = system {
        let mut msgs = vec![json!({"role": "system", "content": sys})];
        msgs.extend(messages);
        msgs
    } else {
        messages
    };

    let (compressed_msgs, tokens_before, tokens_after) = if !bypass && state.config.optimize {
        state
            .compressor
            .compress_json_messages(&all_msgs, model)
            .await
    } else {
        let tokens: u64 = all_msgs
            .iter()
            .filter_map(|m| m["content"].as_str())
            .map(|c| ProxyCompressor::estimate_tokens(c))
            .sum();
        (all_msgs, tokens, tokens)
    };

    if let Some(budget_resp) = check_budget(&state.config, tokens_after) {
        return budget_resp;
    }

    let extracted_system = compressed_msgs
        .iter()
        .find(|m| m["role"].as_str() == Some("system"))
        .and_then(|m| m["content"].as_str())
        .map(|s| s.to_string());
    let compressed_messages_only: Vec<Value> = compressed_msgs
        .into_iter()
        .filter(|m| m["role"].as_str() != Some("system"))
        .collect();

    let mut altered_body = body.clone();
    if let Some(ref sys) = extracted_system {
        altered_body["system"] = json!(sys);
    } else {
        altered_body.as_object_mut().map(|o| o.remove("system"));
    }
    altered_body["messages"] = Value::Array(compressed_messages_only);

    let target_url = format!("{}/v1/messages", state.config.anthropic_api_url);
    proxy_and_inject_meta(&state, &headers, &target_url, altered_body, tokens_before, tokens_after).await
}

async fn compress_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Json<Value> {
    let bypass = should_bypass(&headers);

    let model = body
        .get("model")
        .and_then(|m| m.as_str())
        .unwrap_or("gpt-4o");
    let messages = body
        .get("messages")
        .and_then(|m| m.as_array())
        .cloned()
        .unwrap_or_default();

    let (compressed_msgs, tokens_before, tokens_after) = if !bypass && state.config.optimize {
        state
            .compressor
            .compress_json_messages(&messages, model)
            .await
    } else {
        let tokens: u64 = messages
            .iter()
            .filter_map(|m| m["content"].as_str())
            .map(|c| ProxyCompressor::estimate_tokens(c))
            .sum();
        (messages, tokens, tokens)
    };

    let tokens_saved = tokens_before.saturating_sub(tokens_after);
    let compression_ratio = if tokens_before > 0 {
        tokens_saved as f64 / tokens_before as f64
    } else {
        0.0
    };

    Json(json!({
        "messages": compressed_msgs,
        "tokens_before": tokens_before,
        "tokens_after": tokens_after,
        "tokens_saved": tokens_saved,
        "compression_ratio": compression_ratio,
        "transforms_applied": [format!("router:smart_crusher:{:.2}", compression_ratio)],
        "ccr_hashes": [],
    }))
}
