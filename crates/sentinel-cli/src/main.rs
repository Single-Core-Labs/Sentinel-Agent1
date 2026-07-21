mod approval;
mod display;
mod handler;
mod exec;
mod auth;
mod plugin;
mod server;
mod diagnostics;
mod tui;

use colored::*;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::WARN.into()),
        )
        .init();

    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        print_help();
        return Ok(());
    }

    let subcommand = &args[1];
    let sub_args = &args[2..];

    match subcommand.as_str() {
        "--help" | "-h" | "help" => print_help(),
        "--version" | "-V" => println!("Sentinel v{}", env!("CARGO_PKG_VERSION")),
        "exec" => exec::run(sub_args).await?,
        "auth" => auth::run(sub_args).await?,
        "sandbox" => sandbox::run(sub_args).await?,
        "plugin" => plugin::run(sub_args).await?,
        "server" => server::run(sub_args).await?,
        "diagnostics" => diagnostics::run(sub_args).await?,
        "tui" => tui::run(sub_args).await?,
        other => {
            eprintln!("{} Unknown subcommand: '{}'", "Error:".red().bold(), other);
            eprintln!("Run 'sentinel --help' for usage.");
            std::process::exit(1);
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
    println!("  exec <model> <prompt>    Run the agent with a prompt");
    println!("  auth login|logout|status Authentication management");
    println!("  sandbox check|config     Sandbox environment tools");
    println!("  plugin list|add|remove   Plugin and marketplace management");
    println!("  server start|stop|status App server control");
    println!("  tui [--port <addr>]     Terminal UI interactive session");
    println!("  diagnostics              System diagnostic checks");
    println!();
    println!("{}", "Examples:".yellow().bold());
    println!("  sentinel exec gpt-4o-mini \"write hello world\"");
    println!("  sentinel auth login --token <token>");
    println!("  sentinel diagnostics");
    println!("  sentinel server start");
    println!();
    println!("{}", "Configuration:".yellow().bold());
    println!("  See sentinel.example.toml for options");
    println!("  Config files: sentinel.toml, config.toml, .sentinel.toml");
}
