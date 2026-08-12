use colored::*;
use sentinel_app_server::AppServer;
use std::net::SocketAddr;

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
                port = args
                    .get(i)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(default_port);
            }
            "--host" | "--addr" => {
                i += 1;
                host = args
                    .get(i)
                    .cloned()
                    .unwrap_or_else(|| default_host.to_string());
            }
            "--no-open" => {
                open_browser = false;
            }
            "--static-dir" => {
                i += 1;
                static_dir = args.get(i).cloned();
            }
            "--token" => {
                i += 1;
                // Explicit token overrides SENTINEL_SERVER_TOKEN env var.
                if let Some(tok) = args.get(i).cloned() {
                    unsafe { std::env::set_var("SENTINEL_SERVER_TOKEN", &tok) };
                }
            }
            // #61 – unknown flags are an error, not silently ignored
            other if other.starts_with('-') => {
                eprintln!(
                    "{} Unknown flag: '{}'. Run 'sentinel web --help' for usage.",
                    "Error:".red().bold(),
                    other
                );
                std::process::exit(1);
            }
            _ => {
                eprintln!(
                    "{} Unexpected argument: '{}'. Run 'sentinel web --help' for usage.",
                    "Error:".red().bold(),
                    args[i]
                );
                std::process::exit(1);
            }
        }
        i += 1;
    }

    let addr: SocketAddr = format!("{}:{}", host, port).parse()?;

    println!();
    println!(" {} Starting Sentinel Web Server...", "●".green().bold());
    println!("   Host: {}", host.yellow());
    println!("   Port: {}", port.to_string().yellow());
    println!();

    // #60 – surface config parse errors instead of silently using defaults
    let config = match sentinel_config::SentinelConfig::load() {
        Ok(c) => c,
        Err(e) => {
            eprintln!(
                "{} Warning: config error: {}; using defaults",
                "W".yellow(),
                e
            );
            sentinel_config::SentinelConfig::default()
        }
    };
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

    // Resolve the static directory to an absolute path so it works regardless
    // of the working directory the process was launched from.
    let dir = static_dir.unwrap_or_else(|| {
        // 1) Prefer a `public/` sibling of the running executable (installed layout).
        if let Ok(exe) = std::env::current_exe()
            && let Some(exe_dir) = exe.parent()
        {
            let candidate = exe_dir.join("public");
            if candidate.join("index.html").exists() {
                return candidate.to_string_lossy().to_string();
            }
        }
        // 2) Walk up from cwd looking for a `public/index.html` (dev layout).
        if let Ok(cwd) = std::env::current_dir() {
            for ancestor in cwd.ancestors() {
                let candidate = ancestor.join("public");
                if candidate.join("index.html").exists() {
                    return candidate.to_string_lossy().to_string();
                }
            }
        }
        // 3) Compile-time fallback: the source-tree absolute path baked in.
        //    This is set by build.rs via the SENTINEL_PUBLIC_DIR env var if present,
        //    otherwise we fall through to a relative path and let ServeDir 404.
        option_env!("SENTINEL_PUBLIC_DIR")
            .unwrap_or("public")
            .to_string()
    });

    println!("   Static: {}", dir.yellow());

    let shutdown = sentinel_app_server::shutdown::install_signal_handler();
    server
        .run_http_with_dir_with_shutdown(&addr, &dir, shutdown)
        .await?;
    println!(" {} Web server stopped.", "◼".yellow().bold());

    Ok(())
}
