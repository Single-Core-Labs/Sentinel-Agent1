use colored::*;

pub async fn run(args: &[String]) -> anyhow::Result<()> {
    let _ = args;
    println!("{}", "Sentinel Diagnostics".cyan().bold());
    println!("{}", "====================".cyan());
    println!();

    let mut all_ok = true;

    all_ok &= ck("Configuration file", || {
        match sentinel_config::SentinelConfig::load() {
            Ok(config) => {
                println!("  Default model: {}", config.agent.default_model);
                println!("  Providers:     {}", config.providers().len());
                println!("  MCP servers:   {}", config.mcp_servers().len());
                println!("  Max turns:     {}", config.agent.max_turns);
                println!("  Max iterations: {}", config.agent.max_iterations);
                Ok(())
            }
            Err(e) => Err(format!("Config load failed: {}", e)),
        }
    });

    all_ok &= ck("Environment variables", || {
        let vars = [
            ("OPENAI_API_KEY", "OpenAI"),
            ("ANTHROPIC_API_KEY", "Anthropic"),
            ("GOOGLE_API_KEY", "Google AI Studio"),
            ("DEEPSEEK_API_KEY", "DeepSeek"),
        ];
        for (env_var, name) in &vars {
            let status = if std::env::var(env_var).is_ok() { "set".green().to_string() } else { "not set".dimmed().to_string() };
            println!("  {}: {}", name, status);
        }
        Ok(())
    });

    all_ok &= ck("Git", || {
        let output = std::process::Command::new("git")
            .args(["--version"])
            .output()
            .map_err(|e| format!("git not found: {}", e))?;
        let version = String::from_utf8_lossy(&output.stdout);
        println!("  Version: {}", version.trim());
        Ok(())
    });

    all_ok &= ck("Rust toolchain", || {
        println!("  Version: {}", env!("CARGO_PKG_VERSION"));
        println!("  Target:  {}", std::env::consts::ARCH);
        println!("  OS:      {}", std::env::consts::OS);
        Ok(())
    });

    all_ok &= ck_async("Network", || async {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .map_err(|e| format!("HTTP client error: {}", e))?;
        match client.get("https://httpbin.org/get").send().await {
            Ok(_) => {
                println!("  {}", "Internet access: OK".green());
                Ok(())
            }
            Err(e) => {
                println!("  {}: {}", "Internet access: FAILED".red(), e);
                Ok(())
            }
        }
    }).await;

    println!();
    if all_ok {
        println!("{} All checks passed.", "OK".green().bold());
    } else {
        println!("{} Some checks failed.", "WARNING".yellow().bold());
    }

    Ok(())
}

fn ck(name: &str, f: impl FnOnce() -> Result<(), String>) -> bool {
    print!(" {} {}... ", "•".cyan(), name);
    match f() {
        Ok(()) => { println!("{}", "✓".green()); true }
        Err(e) => { println!("{}", "✗".red()); println!("   {}", e.red()); false }
    }
}

async fn ck_async<F, Fut>(name: &str, f: F) -> bool
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<(), String>>,
{
    print!(" {} {}... ", "•".cyan(), name);
    match f().await {
        Ok(()) => { println!("{}", "✓".green()); true }
        Err(e) => { println!("{}", "✗".red()); println!("   {}", e.red()); false }
    }
}
