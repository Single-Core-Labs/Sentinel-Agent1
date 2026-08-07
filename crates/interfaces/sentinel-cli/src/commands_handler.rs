use std::io::{self, Write};
use colored::*;
use crate::setup::{SentinelConfig, ProviderConfig};

pub async fn handle_model_switch(config: &mut SentinelConfig) -> anyhow::Result<()> {
    println!("\n{}", "Available Models:".cyan().bold());
    println!("{}", "─".repeat(45));

    let providers = get_all_providers();
    let current_provider = providers
        .iter()
        .find(|p| p.id == config.provider_id)
        .cloned()
        .unwrap_or_else(|| providers[0].clone());

    for (idx, (id, name)) in current_provider.models.iter().enumerate() {
        let marker = if id == &config.model_id { "✓" } else { " " };
        println!("  [{}] {} {}",
            format!("{}", idx + 1).cyan(),
            marker.green(),
            format!("{} ({})", name, id).dimmed()
        );
    }

    println!();
    print!("{} ", "Select model:".yellow().bold());
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    if let Ok(idx) = input.trim().parse::<usize>() {
        if idx > 0 && idx <= current_provider.models.len() {
            let (new_id, new_name) = current_provider.models[idx - 1].clone();
            config.model_id = new_id;
            config.model_name = new_name.clone();
            println!("\n{} Switched to: {}", "✓".green(), new_name.green());
            return Ok(());
        }
    }

    println!("{}", "Invalid selection.".red());
    Ok(())
}

pub async fn handle_provider_switch(config: &mut SentinelConfig) -> anyhow::Result<()> {
    println!("\n{}", "Available Providers:".cyan().bold());
    println!("{}", "─".repeat(45));

    let providers = get_all_providers();
    let mut provider_list = vec![(config.provider_id.clone(), config.provider_name.clone())];

    for other in &config.other_providers {
        provider_list.push((other.id.clone(), other.name.clone()));
    }

    for (idx, (id, name)) in provider_list.iter().enumerate() {
        let marker = if id == &config.provider_id { "✓" } else { " " };
        println!("  [{}] {} {}",
            format!("{}", idx + 1).cyan(),
            marker.green(),
            name
        );
    }

    println!();
    print!("{} ", "Select provider:".yellow().bold());
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    if let Ok(idx) = input.trim().parse::<usize>() {
        if idx > 0 && idx <= provider_list.len() {
            let (new_id, new_name) = provider_list[idx - 1].clone();

            // If switching to an additional provider, prompt for API key
            if new_id != config.provider_id {
                if let Some(other) = config.other_providers.iter().find(|p| p.id == new_id) {
                    println!("\n{} Enter API key for {}:", "→".cyan(), new_name.cyan());
                    print!("API Key: ");
                    io::stdout().flush()?;

                    let mut key = String::new();
                    io::stdin().read_line(&mut key)?;
                    let key = key.trim().to_string();

                    if key.is_empty() || key.len() < 10 {
                        println!("{}", "Invalid API key.".red());
                        return Ok(());
                    }
                }
            }

            config.provider_id = new_id.clone();
            config.provider_name = new_name.clone();

            // Load first model from this provider
            if let Some(provider) = providers.iter().find(|p| p.id == new_id) {
                if !provider.models.is_empty() {
                    config.model_id = provider.models[0].0.clone();
                    config.model_name = provider.models[0].1.clone();
                }
            }

            println!("\n{} Switched to: {}", "✓".green(), new_name.green());
            return Ok(());
        }
    }

    println!("{}", "Invalid selection.".red());
    Ok(())
}

pub fn show_settings(config: &SentinelConfig) {
    println!("\n{}", "Current Settings:".cyan().bold());
    println!("{}", "─".repeat(45));
    println!("  {}: {}", "Primary Provider".yellow(), config.provider_name.cyan());
    println!("  {}: {}", "Model".yellow(), config.model_id.green());
    println!("  {}: {}", "API Key".yellow(), format!("...{}", &config.api_key[config.api_key.len()-4..]).dimmed());

    if !config.other_providers.is_empty() {
        println!("  {}: {}", "Additional Providers".yellow(), config.other_providers.len().to_string().cyan());
    }

    println!("  {}: {}", "Config File".yellow(), "~/.sentinel/config.json".dimmed());
}

pub async fn execute_with_ai(prompt: &str, config: &SentinelConfig) -> anyhow::Result<()> {
    println!("\n{} Processing with {}...",
        "⏳".yellow(),
        format!("{} / {}", config.provider_name, config.model_name).cyan()
    );

    // TODO: Implement actual LLM call based on provider
    println!("{}\n", "Feature coming soon! LLM execution will be implemented here.".dimmed());

    println!("{} Prompt:", "›".cyan());
    println!("{}\n", prompt.green());

    println!("{} Response:", "›".cyan());
    println!("{}", "[This is where the AI response would appear]".dimmed());

    Ok(())
}

fn get_all_providers() -> Vec<ProviderConfig> {
    vec![
        ProviderConfig {
            id: "openai".to_string(),
            name: "OpenAI".to_string(),
            api_key_var: "OPENAI_API_KEY".to_string(),
            models: vec![
                ("gpt-4o".to_string(), "GPT-4o (Latest, most capable)".to_string()),
                ("gpt-4o-mini".to_string(), "GPT-4o Mini (Fast, efficient)".to_string()),
                ("o3-mini".to_string(), "o3 Mini (Reasoning)".to_string()),
            ],
        },
        ProviderConfig {
            id: "anthropic".to_string(),
            name: "Anthropic Claude".to_string(),
            api_key_var: "ANTHROPIC_API_KEY".to_string(),
            models: vec![
                ("claude-sonnet-4-20250514".to_string(), "Claude Sonnet 4 (Balanced, capable)".to_string()),
                ("claude-haiku-3-5-20241022".to_string(), "Claude Haiku 3.5 (Fast, compact)".to_string()),
            ],
        },
        ProviderConfig {
            id: "google".to_string(),
            name: "Google Gemini".to_string(),
            api_key_var: "GOOGLE_API_KEY".to_string(),
            models: vec![
                ("gemini-2.5-flash".to_string(), "Gemini 2.5 Flash (Fast)".to_string()),
                ("gemini-2.5-pro".to_string(), "Gemini 2.5 Pro (Advanced)".to_string()),
            ],
        },
        ProviderConfig {
            id: "mistral".to_string(),
            name: "Mistral".to_string(),
            api_key_var: "MISTRAL_API_KEY".to_string(),
            models: vec![
                ("mistral-large-latest".to_string(), "Mistral Large (Most capable)".to_string()),
                ("mistral-medium".to_string(), "Mistral Medium (Balanced)".to_string()),
                ("mistral-small".to_string(), "Mistral Small (Fast)".to_string()),
            ],
        },
    ]
}
