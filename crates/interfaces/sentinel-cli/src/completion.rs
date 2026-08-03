use colored::*;
use std::sync::Arc;

/// One-shot completion subcommand used by the eval harness (LLM-as-judge).
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

    let mut model_id: Option<String> = None;
    let mut system_prompt: Option<String> = None;
    let mut positionals: Vec<String> = Vec::new();

    {
        let mut iter = args.iter();
        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "--model" => model_id = iter.next().cloned(),
                "--system-prompt" => system_prompt = iter.next().cloned(),
                "-h" | "--help" => {
                    println!("Usage: sentinel completion [--model <id>] [--system-prompt <text>] [prompt...]");
                    return Ok(());
                }
                _ if arg.starts_with('-') => {
                    eprintln!(
                        "{} Unknown flag: '{}'. Run 'sentinel completion --help' for usage.",
                        "Error:".red().bold(),
                        arg
                    );
                    std::process::exit(1);
                }
                _ => positionals.push(arg.clone()),
            }
        }
    }

    let model_id = model_id.unwrap_or_else(|| config.agent.default_model.clone());

    let prompt = if !positionals.is_empty() {
        positionals.join(" ")
    } else {
        use std::io::Read;
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf)?;
        buf.trim().to_string()
    };

    if prompt.is_empty() {
        eprintln!("{} completion requires a prompt", "Error:".red().bold());
        std::process::exit(1);
    }

    let provider_info = config
        .providers()
        .iter()
        .find(|p| p.models.iter().any(|m| m.id == model_id))
        .or_else(|| config.providers().first())
        .cloned();

    let provider_info = match provider_info {
        Some(p) => p,
        None => {
            eprintln!(
                "{} No provider found for model '{}'",
                "Error:".red().bold(),
                model_id
            );
            std::process::exit(1);
        }
    };

    let provider = Arc::new(sentinel_provider::ProviderKind::from_info(provider_info)?);
    use sentinel_provider::ModelProvider;

    let mut req = sentinel_protocol::CompletionRequest::new(&model_id);
    if let Some(sp) = system_prompt {
        req = req.with_system(sp);
    }
    req = req.with_message(sentinel_protocol::Message::user(prompt));

    let response = provider.complete(&req).await?;
    if let Some(choice) = response.choices.first() {
        println!("{}", choice.message.extract_text());
    }

    Ok(())
}
