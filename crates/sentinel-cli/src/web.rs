use std::net::SocketAddr;
use colored::*;
use sentinel_app_server::AppServer;

pub async fn run(args: &[String]) -> anyhow::Result<()> {
    let default_port = 9090;
    let default_host = "127.0.0.1";

    let mut port = default_port;
    let mut host = default_host.to_string();
    let mut open_browser = true;
    let mut static_dir: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--port" | "-p" => {
                i += 1;
                port = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(default_port);
            }
            "--host" | "--addr" => {
                i += 1;
                host = args.get(i).cloned().unwrap_or_else(|| default_host.to_string());
            }
            "--no-open" => {
                open_browser = false;
            }
            "--static-dir" => {
                i += 1;
                static_dir = args.get(i).cloned();
            }
            _ => {}
        }
        i += 1;
    }

    let addr: SocketAddr = format!("{}:{}", host, port).parse()?;

    println!();
    println!(" {} Starting Sentinel Web Server...", "●".green().bold());
    println!("   Host: {}", host.yellow());
    println!("   Port: {}", port.to_string().yellow());
    println!();

    let config = sentinel_config::SentinelConfig::load().unwrap_or_default();
    let server = AppServer::new(config);

    // Open browser after a short delay
    if open_browser {
        let addr_str = format!("http://{}", addr);
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            println!(" {} Opening browser...", "●".cyan().bold());
            let _ = webbrowser::open(&addr_str);
        });
    }

    match static_dir {
        Some(dir) => server.run_http_with_dir(&addr, &dir).await,
        None => server.run_http(&addr).await,
    }?;

    Ok(())
}
