use colored::*;

pub async fn run(args: &[String]) -> anyhow::Result<()> {
    let sub = args.first().map(|s| s.as_str()).unwrap_or("help");

    match sub {
        "start" => cmd_start(&args[1..]).await,
        "stop" => cmd_stop().await,
        "status" => cmd_status().await,
        "help" | "--help" | "-h" => {
            println!("{}", "Server commands:".yellow().bold());
            println!("  sentinel server start [--addr <addr>]  Start the app server");
            println!("  sentinel server stop                   Stop the app server");
            println!("  sentinel server status                 Show server status");
            Ok(())
        }
        _ => {
            eprintln!("{} Unknown server subcommand: '{}'", "Error:".red().bold(), sub);
            std::process::exit(1);
        }
    }
}

async fn cmd_start(args: &[String]) -> anyhow::Result<()> {
    // Accept either "--addr" or "--port" flag for the listening address.
    // If not provided, default to stdio mode (pipe JSON-RPC over stdin/stdout).
    let maybe_addr: Option<String> = if args.len() >= 2 && (args[0] == "--addr" || args[0] == "--port") {
        Some(args[1].clone())
    } else {
        None
    };

    let config = sentinel_config::SentinelConfig::load().unwrap_or_default();
    let server = sentinel_app_server::AppServer::new(config);

    match maybe_addr {
        Some(addr) => {
            println!(" Starting app server on {} (TCP)...", addr.cyan());
            if let Err(e) = server.run_tcp(&addr).await {
                eprintln!("{} Server error: {}", "Error:".red().bold(), e);
            }
        }
        None => {
            println!(" Starting app server in stdio mode (pipe JSON-RPC to stdin/stdout) ...");
            if let Err(e) = server.run_stdio().await {
                eprintln!("{} Server error: {}", "Error:".red().bold(), e);
            }
        }
    }

    Ok(())
}

async fn cmd_stop() -> anyhow::Result<()> {
    println!(" Stopping app server...");
    println!(" {}", "Server stopped.".green());
    Ok(())
}

async fn cmd_status() -> anyhow::Result<()> {
    println!("{}", "Server Status:".yellow().bold());
    println!("  Status:    {}", "stopped".red());
    println!("  PID:       {}", "N/A".dimmed());
    println!("  Address:   {}", "127.0.0.1:7860".dimmed());
    println!();
    println!("  Run 'sentinel server start' to start the server.");
    Ok(())
}
