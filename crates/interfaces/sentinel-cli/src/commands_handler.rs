use colored::*;
use crate::prompt;
use crate::setup::{self, SentinelConfig};

pub async fn handle_model_switch(config: &mut SentinelConfig) -> anyhow::Result<()> {
    let catalog = setup::provider_catalog();
    let active_id = config.active().id.clone();
    let provider = match catalog.iter().find(|p| p.id == active_id) {
        Some(p) => p,
        None => {
            println!("{}", "Unknown provider for current selection.".red());
            return Ok(());
        }
    };

    println!("\n{}", "Available Models:".cyan().bold());
    println!("{}", "─".repeat(45));

    let current_model_id = config.active().model_id.clone();
    for (idx, (id, name)) in provider.models.iter().enumerate() {
        let marker = if id == &current_model_id { "✓" } else { " " };
        println!("  [{}] {} {}",
            format!("{}", idx + 1).cyan(),
            marker.green(),
            format!("{} ({})", name, id).dimmed()
        );
    }
    println!();

    let idx = match prompt::read_choice("Select model: ", provider.models.len())? {
        None => {
            println!("{}", "No input received; model unchanged.".yellow());
            return Ok(());
        }
        Some(idx) => idx,
    };

    let (new_id, new_name) = provider.models[idx - 1].clone();
    let active = config.active_mut();
    active.model_id = new_id;
    active.model_name = new_name.clone();
    println!("\n{} Switched to: {}", "✓".green(), new_name.green());
    Ok(())
}

pub async fn handle_provider_switch(config: &mut SentinelConfig) -> anyhow::Result<()> {
    let catalog = setup::provider_catalog();
    let candidates: Vec<_> = catalog
        .iter()
        .filter(|p| !config.providers.iter().any(|cp| cp.id == p.id))
        .collect();

    println!("\n{}", "Configured Providers:".cyan().bold());
    println!("{}", "─".repeat(45));

    for (idx, p) in config.providers.iter().enumerate() {
        let marker = if p.id == config.active_provider_id { "✓" } else { " " };
        println!("  [{}] {} {} {}",
            format!("{}", idx + 1).cyan(),
            marker.green(),
            p.name,
            format!("/ {}", p.model_name).dimmed()
        );
    }

    let add_option_index = config.providers.len() + 1;
    if !candidates.is_empty() {
        println!("  {} {}", format!("[{}]", add_option_index).cyan(), "+ Add new provider".dimmed());
    }
    println!();

    let total_options = config.providers.len() + if candidates.is_empty() { 0 } else { 1 };
    let idx = match prompt::read_choice("Select: ", total_options)? {
        None => {
            println!("{}", "No input received; provider unchanged.".yellow());
            return Ok(());
        }
        Some(idx) => idx,
    };

    if idx == add_option_index && !candidates.is_empty() {
        add_new_provider(config, &candidates)?;
        return Ok(());
    }

    let chosen = config.providers[idx - 1].clone();
    config.active_provider_id = chosen.id.clone();
    println!("\n{} Switched to: {} / {}", "✓".green(), chosen.name.green(), chosen.model_name.dimmed());
    Ok(())
}

fn add_new_provider(
    config: &mut SentinelConfig,
    candidates: &[&crate::setup::ProviderConfig],
) -> anyhow::Result<()> {
    println!("\n{}", "Add New Provider:".cyan().bold());
    println!("{}", "─".repeat(45));
    for (idx, p) in candidates.iter().enumerate() {
        println!("  {} {}", format!("[{}]", idx + 1).cyan(), p.name);
    }
    println!();

    let idx = match prompt::read_choice("Select provider to add: ", candidates.len())? {
        None => {
            println!("{}", "No input received; nothing added.".yellow());
            return Ok(());
        }
        Some(idx) => idx,
    };

    let chosen = candidates[idx - 1];
    match setup::add_provider_flow(chosen)? {
        None => {
            println!("{}", "No input received; nothing added.".yellow());
        }
        Some(entry) => {
            let name = entry.name.clone();
            let model_name = entry.model_name.clone();
            config.active_provider_id = entry.id.clone();
            config.providers.push(entry);
            println!("\n{} Added and switched to: {} / {}", "✓".green(), name.green(), model_name.dimmed());
        }
    }
    Ok(())
}

pub fn show_settings(config: &SentinelConfig) {
    let active = config.active();

    println!("\n{}", "Current Settings:".cyan().bold());
    println!("{}", "─".repeat(45));
    println!("  {}: {}", "Active Provider".yellow(), active.name.cyan());
    println!("  {}: {}", "Model".yellow(), active.model_id.green());
    println!("  {}: {}", "API Key".yellow(),
        if active.api_key.len() > 4 {
            format!("...{}", &active.api_key[active.api_key.len() - 4..]).dimmed()
        } else {
            "****".dimmed()
        }
    );

    if config.providers.len() > 1 {
        println!("\n  {}:", "All Configured Providers".yellow());
        for p in &config.providers {
            let marker = if p.id == config.active_provider_id { "✓" } else { " " };
            println!("    {} {} / {}", marker.green(), p.name, p.model_name.dimmed());
        }
    }

    println!("\n  {}: {}", "Config File".yellow(), "~/.sentinel/config.json".dimmed());
}

pub async fn execute_with_ai(prompt: &str, config: &SentinelConfig) -> anyhow::Result<()> {
    let active = config.active();
    println!("\n{} Processing with {}...",
        "⏳".yellow(),
        format!("{} / {}", active.name, active.model_name).cyan()
    );

    // TODO: Implement actual LLM call based on provider
    println!("{}\n", "Feature coming soon! LLM execution will be implemented here.".dimmed());

    println!("{} Prompt:", "›".cyan());
    println!("{}\n", prompt.green());

    println!("{} Response:", "›".cyan());
    println!("{}", "[This is where the AI response would appear]".dimmed());

    Ok(())
}
