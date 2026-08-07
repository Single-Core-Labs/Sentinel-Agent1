use colored::*;
use crate::prompt;

#[derive(Clone, Debug)]
pub struct ProviderConfig {
    pub id: String,
    pub name: String,
    pub models: Vec<(String, String)>, // (id, name)
}

/// A provider the user has actually configured: has its own key and its own
/// selected model. Every configured provider — including the one picked
/// first during setup — lives in this same list. There is no separate
/// "primary" slot, so switching away from one never makes it unreachable.
#[derive(Clone, Debug)]
pub struct ConfiguredProvider {
    pub id: String,
    pub name: String,
    pub api_key: String,
    pub model_id: String,
    pub model_name: String,
}

#[derive(Clone, Debug)]
pub struct SentinelConfig {
    pub active_provider_id: String,
    pub providers: Vec<ConfiguredProvider>,
}

impl SentinelConfig {
    pub fn active(&self) -> &ConfiguredProvider {
        self.providers
            .iter()
            .find(|p| p.id == self.active_provider_id)
            .expect("active_provider_id must always reference a configured provider")
    }

    pub fn active_mut(&mut self) -> &mut ConfiguredProvider {
        let id = self.active_provider_id.clone();
        self.providers
            .iter_mut()
            .find(|p| p.id == id)
            .expect("active_provider_id must always reference a configured provider")
    }
}

/// The catalog of providers Sentinel knows how to talk to. This is the single
/// source of truth for provider metadata (name, models) — both setup and the
/// in-session `/provider add` flow read from here so they can't drift apart.
pub(crate) fn provider_catalog() -> Vec<ProviderConfig> {
    vec![
        ProviderConfig {
            id: "openai".to_string(),
            name: "OpenAI".to_string(),
            models: vec![
                ("gpt-4o".to_string(), "GPT-4o (Latest, most capable)".to_string()),
                ("gpt-4o-mini".to_string(), "GPT-4o Mini (Fast, efficient)".to_string()),
                ("o3-mini".to_string(), "o3 Mini (Reasoning)".to_string()),
            ],
        },
        ProviderConfig {
            id: "anthropic".to_string(),
            name: "Anthropic Claude".to_string(),
            models: vec![
                ("claude-sonnet-4-20250514".to_string(), "Claude Sonnet 4 (Balanced, capable)".to_string()),
                ("claude-haiku-3-5-20241022".to_string(), "Claude Haiku 3.5 (Fast, compact)".to_string()),
            ],
        },
        ProviderConfig {
            id: "google".to_string(),
            name: "Google Gemini".to_string(),
            models: vec![
                ("gemini-2.5-flash".to_string(), "Gemini 2.5 Flash (Fast)".to_string()),
                ("gemini-2.5-pro".to_string(), "Gemini 2.5 Pro (Advanced)".to_string()),
            ],
        },
        ProviderConfig {
            id: "mistral".to_string(),
            name: "Mistral".to_string(),
            models: vec![
                ("mistral-large-latest".to_string(), "Mistral Large (Most capable)".to_string()),
                ("mistral-medium".to_string(), "Mistral Medium (Balanced)".to_string()),
                ("mistral-small".to_string(), "Mistral Small (Fast)".to_string()),
            ],
        },
    ]
}

/// Runs the full first-time setup: pick a provider, key, model, then
/// optionally add more. Returns `Err` if stdin closes (EOF) before setup
/// finishes — there's no reasonable config to save in that case.
pub async fn run_setup() -> anyhow::Result<SentinelConfig> {
    clear_screen();
    print_header();

    let catalog = provider_catalog();

    let provider = select_provider(&catalog)?
        .ok_or_else(|| anyhow::anyhow!("Setup aborted: no input received."))?;
    let primary = add_provider_flow(&provider)?
        .ok_or_else(|| anyhow::anyhow!("Setup aborted: no input received."))?;

    let mut providers = vec![primary.clone()];
    providers.extend(ask_add_more_providers(&catalog, &provider.id)?);

    clear_screen();
    print_header();
    println!("✓ Setup Complete!\n");
    println!("  Primary Provider: {}", primary.name.cyan().bold());
    println!("  Model: {}", primary.model_name.green());
    println!("  Additional Providers: {}\n",
        if providers.len() <= 1 {
            "None".dimmed().to_string()
        } else {
            format!("{}", providers.len() - 1).green().to_string()
        }
    );
    println!("Saving configuration...");

    Ok(SentinelConfig {
        active_provider_id: primary.id,
        providers,
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

fn select_provider(providers: &[ProviderConfig]) -> anyhow::Result<Option<ProviderConfig>> {
    println!("{}", "Step 1: Choose Your Primary AI Provider".yellow().bold());
    println!("{}", "─".repeat(45));

    for (idx, provider) in providers.iter().enumerate() {
        println!("  {} {}", format!("[{}]", idx + 1).cyan(), provider.name);
    }
    println!();

    let idx = match prompt::read_choice("Select: ", providers.len())? {
        None => return Ok(None),
        Some(idx) => idx,
    };

    clear_screen();
    Ok(Some(providers[idx - 1].clone()))
}

/// Prompts for an API key and a model for `provider`, returning a fully
/// formed `ConfiguredProvider`. Used both by first-run setup and by the
/// in-session "add a new provider" flow, so both stay in sync.
pub(crate) fn add_provider_flow(provider: &ProviderConfig) -> anyhow::Result<Option<ConfiguredProvider>> {
    println!("{}", "Add Your API Key".yellow().bold());
    println!("{}", "─".repeat(45));
    println!("Provider: {}\n", provider.name.cyan().bold());
    println!("To get your API key:");
    println!("  • OpenAI: https://platform.openai.com/api-keys");
    println!("  • Anthropic: https://console.anthropic.com/");
    println!("  • Google: https://ai.google.dev/");
    println!("  • Mistral: https://console.mistral.ai/\n");

    let api_key = match prompt::read_api_key(&provider.name)? {
        None => return Ok(None),
        Some(key) => key,
    };
    println!("{} API key saved (last 4 chars: {})\n",
        "✓".green(),
        format!("...{}", &api_key[api_key.len() - 4..]).dimmed()
    );
    clear_screen();

    let model = match select_model(provider)? {
        None => return Ok(None),
        Some(model) => model,
    };

    Ok(Some(ConfiguredProvider {
        id: provider.id.clone(),
        name: provider.name.clone(),
        api_key,
        model_id: model.0,
        model_name: model.1,
    }))
}

pub(crate) fn select_model(provider: &ProviderConfig) -> anyhow::Result<Option<(String, String)>> {
    println!("{}", "Choose Your Model".yellow().bold());
    println!("{}", "─".repeat(45));
    println!("Provider: {}\n", provider.name.cyan().bold());

    for (idx, (_id, name)) in provider.models.iter().enumerate() {
        println!("  {} {}", format!("[{}]", idx + 1).cyan(), name);
    }
    println!();

    let idx = match prompt::read_choice("Select: ", provider.models.len())? {
        None => return Ok(None),
        Some(idx) => idx,
    };

    clear_screen();
    Ok(Some(provider.models[idx - 1].clone()))
}

/// Offers to add each remaining catalog provider, one at a time. EOF at any
/// point stops asking (treated the same as declining) rather than aborting
/// setup — the primary provider is already configured by this point.
fn ask_add_more_providers(
    catalog: &[ProviderConfig],
    already_configured_id: &str,
) -> anyhow::Result<Vec<ConfiguredProvider>> {
    println!("{}", "Add More Providers (Optional)".yellow().bold());
    println!("{}", "─".repeat(45));
    println!("You can add more providers now, or later with {} in the session.\n", "/provider".cyan());

    let mut additional = Vec::new();

    for provider in catalog {
        if provider.id == already_configured_id {
            continue;
        }

        let wants_it = match prompt::read_yes_no(&format!("Add {} [Y/n]: ", provider.name.cyan()))? {
            None => break,
            Some(v) => v,
        };
        if !wants_it {
            continue;
        }

        match add_provider_flow(provider)? {
            None => break,
            Some(entry) => additional.push(entry),
        }
    }

    clear_screen();
    Ok(additional)
}
