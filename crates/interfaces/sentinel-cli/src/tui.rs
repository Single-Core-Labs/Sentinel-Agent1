use anyhow::Result;
use colored::*;
use sentinel_app_server_client::{AppServerConnection, RemoteClient};
use sentinel_app_server_protocol::api;
use std::io::{self, Write};

/// Simple terminal UI for interacting with the Sentinel app server.
///
/// Expected usage: `sentinel tui [--port <addr>]` where `<addr>` defaults to
/// `127.0.0.1:7860`. The UI creates a session, then reads user input line by
/// line and prints the LLM response.
pub async fn run(args: &[String]) -> Result<()> {
    // Determine server address: default to 127.0.0.1:7860.
    let default_addr = "127.0.0.1:7860".to_string();
    let addr = if args.len() >= 2 && (args[0] == "--port" || args[0] == "--addr") {
        args[1].clone()
    } else {
        default_addr
    };

    // Establish remote connection.
    let connection = AppServerConnection::Remote(RemoteClient::new(&addr)?);

    // Create a new session (use default model).
    let create_params = serde_json::json!({});
    let create_res = connection
        .call(api::methods::CREATE_SESSION, Some(create_params))
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    let session_id = create_res
        .get("session_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing session_id in create_session response"))?
        .to_string();

    println!("{}", "╔══════════════════════════════════╗".green());
    println!("{}", "║        Sentinel TUI Session      ║".green().bold());
    println!("{}", "╚══════════════════════════════════╝".green());
    println!("Session ID: {}", session_id.cyan().bold());
    println!("Type your message and press Enter. Empty line or 'exit' quits.");

    loop {
        print!("{} ", ">".yellow().bold());
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim();
        if input.is_empty() || input.eq_ignore_ascii_case("exit") {
            break;
        }
        match connection.chat(&session_id, input).await {
            Ok(resp) => {
                println!("{}", resp.green());
            }
            Err(e) => {
                eprintln!("{} {}", "✖ Error:".red().bold(), e);
            }
        }
    }

    println!("{}", "Session closed.".dimmed());
    Ok(())
}
