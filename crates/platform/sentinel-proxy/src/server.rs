use std::net::SocketAddr;
use std::sync::Arc;
use axum::http::Method;
use tower_http::cors::{CorsLayer, Any};
use tracing::info;

use crate::compression::ProxyCompressor;
use crate::config::ProxyConfig;
use crate::handlers::{self, AppState};
use crate::stats::SharedStats;

pub async fn run_proxy(config: ProxyConfig) -> anyhow::Result<()> {
    let stats = SharedStats::new();
    let compressor = Arc::new(ProxyCompressor::new(stats.clone()));

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()?;

    let state = AppState {
        compressor,
        stats: stats.clone(),
        config: config.clone(),
        client,
    };

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers(Any);

    let app = handlers::build_router(state).layer(cors);

    let addr: SocketAddr = format!("{}:{}", config.host, config.port).parse()?;

    info!(
        "Headroom proxy listening on http://{}",
        addr
    );
    info!("Optimization: {}", if config.optimize { "enabled" } else { "disabled" });
    info!("OpenAI target: {}", config.openai_api_url);
    info!("Anthropic target: {}", config.anthropic_api_url);
    if let Some(ref budget) = config.budget {
        info!("Daily budget: ${}", budget);
    }
    if config.llmlingua {
        info!("LLMLingua: enabled (device={}, rate={})", config.llmlingua_device, config.llmlingua_rate);
    }

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
