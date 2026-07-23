use sentinel_proxy::{run_proxy, ProxyConfig};

pub async fn run(args: &[String]) -> anyhow::Result<()> {
    let mut config = ProxyConfig::default();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--host" if i + 1 < args.len() => {
                config.host = args[i + 1].clone();
                i += 2;
            }
            "--port" if i + 1 < args.len() => {
                config.port = args[i + 1].parse::<u16>()?;
                i += 2;
            }
            "--openai-api-url" if i + 1 < args.len() => {
                config.openai_api_url = args[i + 1].clone();
                i += 2;
            }
            "--anthropic-api-url" if i + 1 < args.len() => {
                config.anthropic_api_url = args[i + 1].clone();
                i += 2;
            }
            "--no-optimize" => {
                config.optimize = false;
                i += 1;
            }
            "--no-cache" => {
                config.cache = false;
                i += 1;
            }
            "--no-rate-limit" => {
                config.rate_limit = false;
                i += 1;
            }
            "--log-file" if i + 1 < args.len() => {
                config.log_file = Some(args[i + 1].clone());
                i += 2;
            }
            "--budget" if i + 1 < args.len() => {
                config.budget = Some(args[i + 1].parse::<f64>()?);
                i += 2;
            }
            "--llmlingua" => {
                config.llmlingua = true;
                i += 1;
            }
            "--llmlingua-device" if i + 1 < args.len() => {
                config.llmlingua_device = args[i + 1].clone();
                i += 2;
            }
            "--llmlingua-rate" if i + 1 < args.len() => {
                config.llmlingua_rate = args[i + 1].parse::<f64>()?;
                i += 2;
            }
            "--no-telemetry" => {
                config.no_telemetry = true;
                i += 1;
            }
            other => {
                return Err(anyhow::anyhow!("Unknown proxy option: {}", other));
            }
        }
    }

    // Read env overrides
    if let Ok(host) = std::env::var("HEADROOM_HOST") {
        config.host = host;
    }
    if let Ok(port) = std::env::var("HEADROOM_PORT") {
        config.port = port.parse::<u16>()?;
    }
    if let Ok(budget) = std::env::var("HEADROOM_BUDGET") {
        config.budget = Some(budget.parse::<f64>()?);
    }
    if let Ok(url) = std::env::var("OPENAI_TARGET_API_URL") {
        config.openai_api_url = url;
    }

    run_proxy(config).await
}
