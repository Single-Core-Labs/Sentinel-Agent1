use colored::*;
use sentinel_auth::{AuthEntry, get, set, remove, load};
use std::io::{self, Write};

pub async fn run(args: &[String]) -> anyhow::Result<()> {
    let sub = args.first().map(|s| s.as_str()).unwrap_or("help");

    match sub {
        "login" => cmd_login(&args[1..]).await,
        "logout" => cmd_logout(&args[1..]).await,
        "status" => cmd_status().await,
        "help" | "--help" | "-h" => {
            print_help();
            Ok(())
        }
        _ => {
            eprintln!("{} Unknown auth subcommand: '{}'", "Error:".red().bold(), sub);
            std::process::exit(1);
        }
    }
}

async fn cmd_login(args: &[String]) -> anyhow::Result<()> {
    if args.is_empty() {
        eprintln!("{} Usage: sentinel auth login <provider>", "Error:".red().bold());
        eprintln!("   Supported providers: anthropic, openai, google, deepseek");
        std::process::exit(1);
    }

    let provider_id = &args[0];

    match provider_id.as_str() {
        "anthropic" | "openai" | "google" | "deepseek" => {},
        _ => {
            eprintln!("{} Unknown provider: '{}'", "Error:".red().bold(), provider_id);
            eprintln!("   Supported: anthropic, openai, google, deepseek");
            std::process::exit(1);
        }
    }

    print!("Enter API key for {} (hidden): ", provider_id);
    io::stdout().flush()?;

    let key = rpassword::read_password()?;

    if key.is_empty() {
        eprintln!("{} API key cannot be empty", "Error:".red().bold());
        std::process::exit(1);
    }

    set(provider_id, AuthEntry::Bearer { token: key })?;
    println!(" {} API key for '{}' stored successfully.", "✓".green(), provider_id);
    Ok(())
}

async fn cmd_logout(args: &[String]) -> anyhow::Result<()> {
    if args.is_empty() {
        eprintln!("{} Usage: sentinel auth logout <provider>", "Error:".red().bold());
        std::process::exit(1);
    }

    let provider_id = &args[0];
    remove(provider_id)?;
    println!(" {} API key for '{}' removed.", "✓".green(), provider_id);
    Ok(())
}

async fn cmd_status() -> anyhow::Result<()> {
    let creds = load()?;
    println!("{}", "Authentication Status:".yellow().bold());

    let entries = creds.all();
    if entries.is_empty() {
        println!("  (No stored credentials)");
    } else {
        for (provider_id, entry) in entries {
            match entry {
                AuthEntry::Bearer { token } => {
                    let masked = if token.len() > 4 {
                        format!("****{}", &token[token.len() - 4..])
                    } else {
                        "****".to_string()
                    };
                    println!("  {}: {}", provider_id, masked.dimmed());
                }
            }
        }
    }
    Ok(())
}

fn print_help() {
    println!("{}", "Auth commands:".yellow().bold());
    println!("  sentinel auth login <provider>     Store API key for provider");
    println!("  sentinel auth logout <provider>    Remove stored API key");
    println!("  sentinel auth status               List configured providers");
    println!();
    println!("  Supported providers: anthropic, openai, google, deepseek");
}
