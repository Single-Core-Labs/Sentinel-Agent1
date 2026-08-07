mod ai;
mod app;
mod approval;
mod auth;
mod completion;
mod diagnostics;
mod display;
mod exec;
mod handler;
mod local;
mod mcp_setup;
mod model_selector;
mod plugin_cmd;
mod proxy;
mod schema;
mod server;
mod telemetry;
mod tui;
mod web;
mod setup;
mod interactive;
mod config_storage;
mod commands_handler;

use colored::*;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::WARN.into()),
        )
        .init();

    let args: Vec<String> = std::env::args().collect();

    // If no args, start interactive mode or setup
    if args.len() < 2 {
        return start_interactive_or_setup().await;
    }

    let subcommand = &args[1];
    let sub_args = &args[2..];

    match subcommand.as_str() {
        "--help" | "-h" | "help" => print_help(),
        "--version" | "-V" => println!("Sentinel v{}", env!("CARGO_PKG_VERSION")),
        "exec" => exec::run(sub_args).await?,
        "completion" => completion::run(sub_args).await?,
        "ai" => ai::run(sub_args).await?,
        "local" => local::run(sub_args).await?,
        "auth" => auth::run(sub_args).await?,
        "server" => server::run(sub_args).await?,
        "plugin" => plugin_cmd::run(sub_args).await?,
        "web" => web::run(sub_args).await?,
        "proxy" => proxy::run(sub_args).await?,
        "diagnostics" => diagnostics::run(sub_args).await?,
        other => {
            eprintln!("{} Unknown subcommand: '{}'", "Error:".red().bold(), other);
            eprintln!("Run 'sentinel --help' for usage.");
            std::process::exit(1);
        }
    }

    Ok(())
}

async fn start_interactive_or_setup() -> anyhow::Result<()> {
    // Check if config exists
    match config_storage::load_config() {
        Ok(Some(config)) => {
            // Config exists, start interactive mode
            interactive::run_interactive(config).await?;
        }
        Ok(None) => {
            // No config, run setup
            let config = setup::run_setup().await?;
            config_storage::save_config(&config)?;

            // Start interactive mode after setup
            interactive::run_interactive(config).await?;
        }
        Err(_) => {
            // Error loading config, run setup
            let config = setup::run_setup().await?;
            config_storage::save_config(&config)?;

            // Start interactive mode after setup
            interactive::run_interactive(config).await?;
        }
    }

    Ok(())
}

fn print_help() {
    println!("{}", "Sentinel — AI coding agent".cyan().bold());
    println!();
    println!("{}", "Usage:".yellow().bold());
    println!("  sentinel <command> [args]");
    println!();
    println!("{}", "Subcommands:".yellow().bold());
    println!("  ai [model]            Interactive agent session (Rust native)");
    println!("  local [model]         Run a local model via Ollama");
    println!("  exec <model> <prompt>  Run the agent with a prompt (Rust native)");
    println!("  completion [--model <id>] [--system-prompt <text>] <prompt>  One-shot completion (LLM judge)");
    println!("  auth login|logout|status Authentication management");
    println!("  server start|stop|status App server control");
    println!("  plugin install|list|remove Plugin management (tools + policy hooks)");
    println!("  web [--port <n>]        Start HTTP server with Web UI");
    println!("  proxy                  Headroom HTTP compression proxy");
    println!("  diagnostics            System diagnostic checks");
    println!();
    println!("{}", "Examples:".yellow().bold());
    println!("  sentinel ai");
    println!("  sentinel ai zai-org/GLM-5.2:novita");
    println!("  sentinel exec gpt-4o-mini \"write hello world\"");
    println!("  sentinel auth login --token <token>");
    println!("  sentinel diagnostics");
    println!("  sentinel server start");
    println!("  sentinel proxy --host 0.0.0.0 --port 8787");
    println!();
    println!("{}", "Configuration:".yellow().bold());
    println!("  See sentinel.example.toml for options");
    println!("  Config files: sentinel.toml, config.toml, .sentinel.toml");
}
