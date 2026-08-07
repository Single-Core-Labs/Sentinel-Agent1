use std::io::{self, Write};
use colored::*;

#[derive(Clone, Debug)]
pub struct ProviderConfig {
    pub id: String,
    pub name: String,
    pub api_key_var: String,
    pub models: Vec<(String, String)>, // (id, name)
}

#[derive(Clone, Debug)]
pub struct SentinelConfig {
    pub provider_id: String,
    pub provider_name: String,
    pub model_id: String,
    pub model_name: String,
    pub api_key: String,
    pub other_providers: Vec<ProviderConfig>,
}

// Available providers
fn get_providers() -> Vec<ProviderConfig> {
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

pub async fn run_setup() -> anyhow::Result<SentinelConfig> {
    clear_screen();
    print_header();

    let providers = get_providers();

    // Step 1: Select Provider
    let provider = select_provider(&providers)?;

    // Step 2: Get API Key
    let api_key = get_api_key(&provider)?;

    // Step 3: Select Model
    let model = select_model(&provider)?;

    // Step 4: Ask to add more providers
    let other_providers = ask_add_more_providers(&providers, &provider.id)?;

    clear_screen();
    print_header();
    println!("✓ Setup Complete!\n");
    println!("  Primary Provider: {}", provider.name.cyan().bold());
    println!("  Model: {}", model.1.green());
    println!("  Additional Providers: {}\n",
        if other_providers.is_empty() {
            "None".dimmed().to_string()
        } else {
            format!("{}", other_providers.len()).green().to_string()
        }
    );

    println!("Saving configuration...");

    Ok(SentinelConfig {
        provider_id: provider.id.clone(),
        provider_name: provider.name.clone(),
        model_id: model.0,
        model_name: model.1,
        api_key: api_key.clone(),
        other_providers,
    })
}

fn print_header() {
    println!("{}", "╭─────────────────────────────────────────╮".cyan());
    println!("{}", "│  🤖 Sentinel AI Agent - Setup         │".cyan());
    println!("{}", "│  Version 0.1.0                          │".cyan());
    println!("{}", "╰─────────────────────────────────────────╯".cyan());
    println!();
}

fn clear_screen() {
    print!("\x1B[2J\x1B[1;1H");
}

fn select_provider(providers: &[ProviderConfig]) -> anyhow::Result<ProviderConfig> {
    loop {
        println!("{}", "Step 1: Choose Your Primary AI Provider".yellow().bold());
        println!("{}", "─".repeat(45));

        for (idx, provider) in providers.iter().enumerate() {
            println!("  {} {}",
                format!("[{}]", idx + 1).cyan(),
                provider.name
            );
        }

        println!();
        print!("{} ", "Select [1-4]:".yellow().bold());
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        if let Ok(idx) = input.trim().parse::<usize>() {
            if idx > 0 && idx <= providers.len() {
                clear_screen();
                return Ok(providers[idx - 1].clone());
            }
        }

        println!("{}\n", "❌ Invalid selection. Try again.".red());
    }
}

fn get_api_key(provider: &ProviderConfig) -> anyhow::Result<String> {
    loop {
        println!("{}", "Step 2: Add Your API Key".yellow().bold());
        println!("{}", "─".repeat(45));
        println!("Provider: {}\n", provider.name.cyan().bold());

        println!("To get your API key:");
        println!("  • OpenAI: https://platform.openai.com/api-keys");
        println!("  • Anthropic: https://console.anthropic.com/");
        println!("  • Google: https://ai.google.dev/");
        println!("  • Mistral: https://console.mistral.ai/\n");

        print!("{} ", "Paste your API key:".yellow().bold());
        io::stdout().flush()?;

        let mut key = String::new();
        io::stdin().read_line(&mut key)?;
        let key = key.trim().to_string();

        if key.is_empty() {
            println!("{}\n", "❌ API key cannot be empty.".red());
            continue;
        }

        if key.len() < 10 {
            println!("{}\n", "❌ API key seems too short.".red());
            continue;
        }

        println!("{} API key saved (last 4 chars: {})\n",
            "✓".green(),
            format!("...{}", &key[key.len()-4..]).dimmed()
        );

        clear_screen();
        return Ok(key);
    }
}

fn select_model(provider: &ProviderConfig) -> anyhow::Result<(String, String)> {
    loop {
        println!("{}", "Step 3: Choose Your Default Model".yellow().bold());
        println!("{}", "─".repeat(45));
        println!("Provider: {}\n", provider.name.cyan().bold());

        for (idx, (id, name)) in provider.models.iter().enumerate() {
            println!("  {} {}",
                format!("[{}]", idx + 1).cyan(),
                name
            );
        }

        println!();
        print!("{} ", "Select [1-3]:".yellow().bold());
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        if let Ok(idx) = input.trim().parse::<usize>() {
            if idx > 0 && idx <= provider.models.len() {
                let selected = provider.models[idx - 1].clone();
                clear_screen();
                return Ok(selected);
            }
        }

        println!("{}\n", "❌ Invalid selection. Try again.".red());
    }
}

fn ask_add_more_providers(
    all_providers: &[ProviderConfig],
    selected_id: &str,
) -> anyhow::Result<Vec<ProviderConfig>> {
    println!("{}", "Step 4: Add More Providers (Optional)".yellow().bold());
    println!("{}", "─".repeat(45));
    println!("You can add more providers to switch between them later.\n");

    let mut additional = Vec::new();

    for provider in all_providers {
        if provider.id != selected_id {
            print!("Add {} [Y/n]: ", provider.name.cyan());
            io::stdout().flush()?;

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;

            if input.trim().to_lowercase() != "n" {
                print!("  API Key for {}: ", provider.name.cyan());
                io::stdout().flush()?;

                let mut key = String::new();
                io::stdin().read_line(&mut key)?;
                let key = key.trim().to_string();

                if !key.is_empty() && key.len() > 10 {
                    let mut provider_with_key = provider.clone();
                    additional.push(provider_with_key);
                } else if !key.is_empty() {
                    println!("{} API key too short, skipping.", "⚠️".yellow());
                }
            }
        }
    }

    clear_screen();
    Ok(additional)
}
