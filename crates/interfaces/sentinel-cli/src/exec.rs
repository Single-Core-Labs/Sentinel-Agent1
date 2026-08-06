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
    let selected = match crate::model_selector::resolve_model(&config, &model_id) {
        Ok(sel) => sel,
        Err(e) => {
            eprintln!("✖ {}", e);
            std::process::exit(1);
        }
    };
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
    let tools = Arc::new(tool_registry);
    tools.register(Arc::new(sentinel_core::SubAgentTool::new(
        provider.clone(),
        Arc::clone(&tools),
        config.clone(),
    )));

    let agent = sentinel_core::Agent::new(provider, tools, config.clone())
        .with_event_handler(Arc::new(CliEventHandler))
        .with_compressor(headroom_compressor)
        .with_model(model_id.clone());

    // Central app: owns the session store, permission gate, theme, LSP clients
    // and the agent; LSP clients start asynchronously and never block startup.
    let mut app = App::new((*config).clone());
    app.attach_agent(agent);
    app.set_permissions(if config.agent.yolo_mode {
        Box::new(sentinel_core::AutoApprovalGate)
    } else {
        Box::new(CliApprovalGate)
    });
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

    // Optional: Set up worktree manager for parallel agents
    let _wtm =
        sentinel_core::worktree::WorktreeManager::new(&std::env::current_dir().unwrap_or_default());
    // Use wtm.create_worktree("agent-1").await for parallel agent isolation

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
