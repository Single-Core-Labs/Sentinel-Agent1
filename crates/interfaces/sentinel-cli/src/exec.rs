use crate::app::App;
use crate::approval::CliApprovalGate;
use crate::display::{print_banner, print_divider};
use crate::handler::CliEventHandler;
use colored::*;
use std::sync::Arc;

pub async fn run(args: &[String]) -> anyhow::Result<()> {
    let config = Arc::new(match sentinel_config::SentinelConfig::load() {
        Ok(c) => c,
        Err(e) => {
            eprintln!(
                "{} Warning: config error: {}; using defaults",
                "W".yellow(),
                e
            );
            sentinel_config::SentinelConfig::default()
        }
    });

    let model_id = if !args.is_empty() && !args[0].starts_with('-') {
        args[0].clone()
    } else {
        config.agent.default_model.clone()
    };

    let prompt = if args.len() >= 2 {
        args[1..].join(" ")
    } else if args.len() == 1 && !args[0].starts_with('-') {
        let mut input = String::new();
        eprintln!("{}", "Enter prompt (Ctrl+D to submit):".yellow());
        for line in std::io::stdin().lines() {
            match line {
                Ok(l) => {
                    if l.trim().is_empty() {
                        break;
                    }
                    input.push_str(&l);
                    input.push('\n');
                }
                Err(_) => break,
            }
        }
        input.trim().to_string()
    } else {
        String::new()
    };

    if prompt.is_empty() {
        eprintln!(
            "{} sentinel exec [model] \"your prompt\"",
            "Usage:".yellow().bold()
        );
        std::process::exit(1);
    }

    // #49/#52/#53 — centralized model+provider resolution with validation and
    // API-key preflight, instead of a silent fallback to the first provider.
    let user_explicit_model = !args.is_empty() && !args[0].starts_with('-');
    let local_endpoint = std::env::var("SENTINEL_LOCAL_ENDPOINT")
        .or_else(|_| std::env::var("LOCAL_ENDPOINT"))
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);
    let is_local_spec =
        model_id.starts_with("ollama/")
            || model_id.starts_with("vllm/")
            || model_id.starts_with("lm-studio/")
            || model_id.starts_with("llamacpp/");
    let model_to_resolve = if local_endpoint && !user_explicit_model && !is_local_spec {
        "ollama/auto".to_string()
    } else {
        model_id.clone()
    };
    let mut selected = match crate::model_selector::resolve_model(&config, &model_to_resolve) {
        Ok(sel) => sel,
        Err(e) => {
            eprintln!("✖ {}", e);
            std::process::exit(1);
        }
    };
    match crate::model_selector::apply_local_discovery(&mut selected, user_explicit_model).await {
        Ok(Some(adopted)) => println!(" · local default model: {} (LOCAL_ENDPOINT)", adopted),
        Ok(None) => {}
        Err(e) => {
            eprintln!("✖ {}", e);
            std::process::exit(1);
        }
    }
    let model_id = selected.model_id;

    let provider = Arc::new(sentinel_provider::ProviderKind::from_info(
        selected.provider.clone(),
    )?);

    let tool_registry = sentinel_tools::ToolRegistry::new();

    let mcp_servers = config.mcp_servers();
    if !mcp_servers.is_empty() {
        println!(
            " {} MCP servers configured",
            format!("{}", mcp_servers.len()).yellow()
        );
    }
    let fetchers = crate::mcp_setup::spawn_mcp_fetchers(mcp_servers);
    fetchers.join(&tool_registry).await;

    let (headroom_compressor, headroom_retrieve_tool, headroom_memory_tools) =
        sentinel_headroom::integration::create_headroom_compressor_with_tools().await;
    tool_registry.register(headroom_retrieve_tool as Arc<dyn sentinel_tools::Tool>);
    for tool in headroom_memory_tools {
        tool_registry.register(tool);
    }

    // Guard plugins: same policy plane as `sentinel ai` so exec runs cannot
    // bypass workspace/web/command guards.
    let plugin_registry = Arc::new(sentinel_plugin_system::PluginRegistry::new());
    let (loaded_count, failed_plugins) =
        sentinel_plugin_system::load_default_plugins(&plugin_registry).await;
    if loaded_count > 0 {
        println!(
            " {} plugins loaded",
            format!("{}", loaded_count).yellow()
        );
    }
    if !failed_plugins.is_empty() {
        eprintln!(
            "{} {} plugins failed:",
            "✖".red().bold(),
            failed_plugins.len()
        );
        for err in failed_plugins {
            eprintln!("  {} {}", "•".red(), err);
        }
    }

    let tools = Arc::new(tool_registry);
    tools.register(Arc::new(
        sentinel_core::SubAgentTool::new(
            provider.clone(),
            Arc::clone(&tools),
            config.clone(),
            Arc::clone(&plugin_registry),
        )
        .with_compressor(headroom_compressor.clone()),
    ));

    let agent = sentinel_core::Agent::new(provider, tools, config.clone())
        .with_event_handler(Arc::new(CliEventHandler))
        .with_prompt_manager(sentinel_core::ProjectContext::inject_into_prompt_manager(&config))
        .with_event_store(sentinel_core::create_event_store_in(
            &sentinel_core::default_events_dir(),
        ))
        .with_compressor(headroom_compressor)
        .with_model(model_id.clone())
        .with_plugin_registry(plugin_registry.clone());

    // SENTINEL_SANDBOX=1 confines write/edit/run_shell to a scratch copy of
    // the workspace in the OS temp dir — the agent can never touch the real
    // repo (used by the cost-lab benchmark harness).
    let agent = if std::env::var("SENTINEL_SANDBOX").as_deref() == Ok("1") {
        let scratch = std::env::temp_dir().join(format!(
            "sentinel-bench-ws-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&scratch).ok();
        match sentinel_core::sandbox::LocalSandbox::new(&scratch) {
            Ok(sb) => {
                let sb: sentinel_core::sandbox::SharedSandbox = Arc::new(sb);
                println!(" {} tools sandboxed in {}", "·".cyan(), sb.root().display());
                agent.with_sandbox(sb)
            }
            Err(e) => {
                eprintln!(" {} sandbox init failed: {}; continuing unsandboxed", "W".yellow(), e);
                agent
            }
        }
    } else {
        agent
    };

    // Central app: owns the session store, permission gate, theme, LSP clients
    // and the agent; LSP clients start asynchronously and never block startup.
    let mut app = App::new((*config).clone());
    app.attach_agent(agent);
    app.set_permissions(sentinel_core::permissions_gate_for(
        &config,
        if config.agent.yolo_mode {
            Box::new(sentinel_core::AutoApprovalGate)
        } else {
            Box::new(CliApprovalGate)
        },
    ));
    app.start_background();

    // The pipeline wraps the central agent with staged execution.
    let pipeline_agent = sentinel_core::pipeline::PipelineAgent::new(
        app.take_agent().expect("agent attached to app"),
    );

    // Optional: Create sandbox for tool isolation
    let _sandbox = None::<std::sync::Arc<sentinel_core::sandbox::LocalSandbox>>;
    // Uncomment to enable sandbox:
    // let sandbox = std::sync::Arc::new(sentinel_core::sandbox::LocalSandbox::new(&std::env::current_dir().unwrap())?);

    // Configure pipeline with memory file
    let mfm = sentinel_core::memory_file::MemoryFileManager::new(
        &std::env::current_dir().unwrap_or_default(),
    );
    let pipeline_agent = pipeline_agent.with_memory_file(mfm);

    let mut thread = app.new_session(config.agent.yolo_mode);

    print_banner();
    println!(" Model:  {}", model_id.green().bold());
    println!(
        " Yolo:   {}",
        if config.agent.yolo_mode {
            "yes".green()
        } else {
            "no".yellow()
        }
    );
    println!(" Pipeline: {}", "read → triage → draft → QA → send".cyan());
    print_divider();

    let result = pipeline_agent
        .run_pipeline(&mut thread, &prompt, app.permissions())
        .await;

    match result {
        Ok(output) => match output {
            sentinel_core::AgentOutput::Success { .. } => {}
            sentinel_core::AgentOutput::Error { message } => {
                crate::display::print_error(&message);
                std::process::exit(1);
            }
        },
        Err(e) => {
            crate::display::print_error(&e.to_string());
            std::process::exit(1);
        }
    }

    if let Err(e) = app.save_session(&thread).await {
        eprintln!("{} Failed to save session: {}", "W".yellow(), e);
    }

    let (prompt_tok, completion_tok) = (
        pipeline_agent.inner().prompt_tokens(),
        pipeline_agent.inner().completion_tokens(),
    );
    let token_info = if prompt_tok > 0 || completion_tok > 0 {
        format!("{} in, {} out", prompt_tok, completion_tok)
    } else {
        String::new()
    };

    let stats = format!("turns: {}, iterations: {}", thread.turn, thread.iterations);
    let summary = if token_info.is_empty() {
        stats
    } else {
        format!("{}, {} tokens", stats, token_info)
    };
    println!("\n{} {}", "Done.".green().bold(), summary.dimmed());

    // Graceful shutdown: terminate LSP clients and background watchers.
    app.shutdown().await;

    Ok(())
}
