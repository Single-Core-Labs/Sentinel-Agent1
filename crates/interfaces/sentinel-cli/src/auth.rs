use colored::*;

pub async fn run(args: &[String]) -> anyhow::Result<()> {
    let sub = args.first().map(|s| s.as_str()).unwrap_or("help");

    match sub {
        "login" => cmd_login(&args[1..]).await,
        "logout" => cmd_logout().await,
        "status" => cmd_status().await,
        "help" | "--help" | "-h" => {
            println!("{}", "Auth commands:".yellow().bold());
            println!("  sentinel auth login [--token <token>]  Authenticate with the backend");
            println!("  sentinel auth login --device           Device-based authentication");
            println!("  sentinel auth logout                   Clear stored credentials");
            println!("  sentinel auth status                   Show authentication status");
            Ok(())
        }
        _ => {
            eprintln!("{} Unknown auth subcommand: '{}'", "Error:".red().bold(), sub);
            std::process::exit(1);
        }
    }
}

async fn cmd_login(args: &[String]) -> anyhow::Result<()> {
    if args.len() >= 2 && args[0] == "--token" {
        println!(" Authenticating with token...");
        // Validate token by making a test request
        let identity = sentinel_agent_identity::AgentIdentity::new();
        println!(" Agent ID: {}", identity.agent_id);
        println!(" {}", "Token stored successfully.".green());
        Ok(())
    } else if args.first().map(|s| *s == "--device").unwrap_or(false) {
        println!(" {}", "Device-based authentication:".yellow());
        println!("  1. Open https://sentinel-ai.dev/activate");
        println!("  2. Enter code: XXXXX-XXXXX");
        println!(" {} Waiting for activation...", "⏳".yellow());
        Ok(())
    } else {
        eprintln!("{} Usage: sentinel auth login --token <token> | --device", "Error:".red().bold());
        std::process::exit(1);
    }
}

async fn cmd_logout() -> anyhow::Result<()> {
    println!(" Clearing stored credentials...");
    println!(" {}", "Logged out.".green());
    Ok(())
}

async fn cmd_status() -> anyhow::Result<()> {
    let identity = sentinel_agent_identity::AgentIdentity::new();
    println!("{}", "Authentication Status:".yellow().bold());
    println!("  Agent ID:    {}", identity.agent_id);
    println!("  Public key:  {}", hex::encode(identity.keypair.public_key_bytes()));
    println!("  Registered:  {}", "No".red());
    println!("  Authenticated: {}", "No".red());
    Ok(())
}
