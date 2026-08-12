mod ai;
mod app;
mod approval;
mod auth;
mod completion;
mod diagnostics;
mod display;
mod exec;
mod handler;
mod host;
mod local;
mod mcp_setup;
mod model_selector;
mod plugin_cmd;
mod proxy;
mod schema;
mod server;
mod telemetry;
mod tui;

use colored::*;
use tracing_subscriber::layer::{Layer, SubscriberExt};
use tracing_subscriber::util::SubscriberInitExt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv::dotenv().ok();

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer().with_filter(
                tracing_subscriber::EnvFilter::from_default_env()
                    .add_directive(tracing::Level::WARN.into()),
            ),
        )
        .with(sentinel_app_server::logs::LogLayer::new())
        .init();

    let args: Vec<String> = std::env::args().collect();

    crate::load_dotenv();

    if args.len() < 2 {
        return ai::run(&[]).await;
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
        "telemetry" => telemetry::run(sub_args).await?,
        "proxy" => proxy::run(sub_args).await?,
        "diagnostics" => diagnostics::run(sub_args).await?,
        "schema" => schema::run(sub_args)?,
        "tui" => tui::run(sub_args).await?,
        other => {
            eprintln!("{} Unknown subcommand: '{}'", "Error:".red().bold(), other);
            eprintln!("Run 'sentinel --help' for usage.");
            std::process::exit(1);
        }
    }

    Ok(())
}

fn load_dotenv() {
    let candidates = std::env::var("SENTINEL_HOME")
        .map(|h| std::path::PathBuf::from(h).join(".env"))
        .into_iter()
        .chain(std::iter::once(std::path::PathBuf::from(".env")))
        .collect::<Vec<_>>();

    for path in candidates {
        if path.exists()
            && let Ok(contents) = std::fs::read_to_string(&path)
        {
            for line in contents.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with('#') {
                    continue;
                }
                if let Some((key, value)) = trimmed.split_once('=') {
                    let key = key.trim();
                    if !key.is_empty() && std::env::var_os(key).is_none() {
                        unsafe { std::env::set_var(key, value.trim()) };
                    }
                }
            }
        }
    }
}

fn print_help() {
    println!("{}", "Sentinel — AI coding agent".cyan().bold());
    println!();
    println!("{}", "Usage:".yellow().bold());
    println!("  sentinel <command> [args]");
    println!();
    println!("{}", "Subcommands:".yellow().bold());
    println!(
        "  ai [model]            Interactive agent session (requires bun; use --prompt for one-shot mode without it)"
    );
    println!("  local [model]         Run a local model via Ollama");
    println!("  exec <model> <prompt>  Run the agent with a prompt (Rust native)");
    println!(
        "  completion [--model <id>] [--system-prompt <text>] <prompt>  One-shot completion (LLM judge)"
    );
    println!("  auth login|logout|status Authentication management");
    println!("  server start|stop|status App server control");
    println!("  plugin install|list|remove Plugin management (tools + policy hooks)");
    println!("  telemetry on|off|status  Anonymous crash-reporting consent");
    println!("  proxy                  Headroom HTTP compression proxy");
    println!("  diagnostics            System diagnostic checks");
    println!(
        "  schema                 Print JSON Schema for sentinel.toml (IDE validation/autocompletion)"
    );
    println!("  tui [--port <n>]        Terminal UI for app server");
    println!();
    println!("{}", "Common flags:".yellow().bold());
    println!(
        "  --model <id>          Pick a model (e.g. gpt-4o, claude-sonnet-4, gemini-2.5-flash, ollama/qwen3:8b)"
    );
    println!("  --prompt <text>      Run one non-interactive turn, then exit");
    println!("  --resume <session-id> Continue a previous session");
    println!("  --new                Start a fresh session");
    println!("  --yolo               Auto-approve tool actions (dangerous)");
    println!();
    println!("{}", "Examples:".yellow().bold());
    println!("  sentinel ai");
    println!("  sentinel ai --model gemini-2.5-flash");
    println!("  sentinel ai --model gpt-4o --prompt \"debug why the k8s pod crashes\"");
    println!("  sentinel ai --resume <session-id>");
    println!("  sentinel exec gpt-4o-mini \"write hello world\"");
    println!("  sentinel auth login --token <token>");
    println!("  sentinel diagnostics");
    println!("  sentinel server start");
    println!("  sentinel proxy --host 0.0.0.0 --port 8787");
    println!();
    println!("{}", "Configuration:".yellow().bold());
    println!("  Copy sentinel.example.toml to sentinel.toml and edit defaults");
    println!("  Config priority: ./sentinel.toml > ./config.toml > ./.sentinel.toml");
    println!("  API keys: add to .env (e.g. OPENAI_API_KEY=sk-... ) or your shell");
    println!("  See https://github.com/Single-Core-Labs/Sentinel-Agent1#readme for details");
}
