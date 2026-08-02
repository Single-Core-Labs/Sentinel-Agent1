use colored::*;
use sentinel_analytics::consent::{
    is_consent_granted, load_consent, prompt_for_consent_once, save_consent, TelemetryConsent,
};

/// Boot sequence: ask for consent once, then install the crash hook.
/// Non-interactive runs never block and default to opt-out.
pub fn boot(non_interactive: bool) {
    if sentinel_analytics::crash::is_hook_initialized() {
        return;
    }
    let consent = prompt_for_consent_once(non_interactive);
    let crash_dir = crash_dir();
    match consent {
        TelemetryConsent::OptedIn => {
            let client = sentinel_analytics::AnalyticsEventsClient::new(
                sentinel_analytics::AnalyticsDestination::CaptureFile {
                    path: crash_dir.join("analytics.ndjson"),
                },
                sentinel_analytics::AnalyticsQueueConfig::default(),
            );
            sentinel_analytics::crash::install_crash_hook(
                Some(std::sync::Arc::new(client)),
                Some(crash_dir),
            );
        }
        _ => {
            // Kept local-only dumps for opt-out users so a crash is never lost
            // entirely — it just never leaves the machine.
            sentinel_analytics::crash::install_crash_hook(None, Some(crash_dir));
        }
    }
}

fn crash_dir() -> std::path::PathBuf {
    if let Ok(home) = std::env::var("SENTINEL_HOME") {
        std::path::PathBuf::from(home).join("logs")
    } else {
        let base = std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .unwrap_or_else(|_| ".".into());
        std::path::PathBuf::from(base).join(".sentinel").join("logs")
    }
}

pub async fn run(args: &[String]) -> anyhow::Result<()> {
    let sub = args.first().map(|s| s.as_str()).unwrap_or("status");

    match sub {
        "on" => cmd_on(),
        "off" => cmd_off(),
        "status" => cmd_status(),
        "help" | "--help" | "-h" => {
            println!("{}", "Telemetry commands:".yellow().bold());
            println!("  sentinel telemetry on       Opt into anonymous crash reporting");
            println!("  sentinel telemetry off      Opt out (default)");
            println!("  sentinel telemetry status   Show current consent");
            Ok(())
        }
        _ => {
            eprintln!(
                "{} Unknown telemetry subcommand: '{}'",
                "Error:".red().bold(),
                sub
            );
            std::process::exit(1);
        }
    }
}

fn cmd_on() -> anyhow::Result<()> {
    let path = save_consent(true).map_err(|e| anyhow::anyhow!("{}", e))?;
    println!(" {} Telemetry enabled. Anonymous crash reports will be sent.", "✓".green().bold());
    println!("   Consent: {}", path.display());
    Ok(())
}

fn cmd_off() -> anyhow::Result<()> {
    let path = save_consent(false).map_err(|e| anyhow::anyhow!("{}", e))?;
    println!(" {} Telemetry disabled. No data is collected.", "✗".red().bold());
    println!("   Consent: {}", path.display());
    Ok(())
}

fn cmd_status() -> anyhow::Result<()> {
    match load_consent() {
        TelemetryConsent::OptedIn => {
            println!(" {} Anonymous crash reporting: {} (opt-in)", "•".green(), "ENABLED".green().bold())
        }
        TelemetryConsent::OptedOut => {
            println!(" {} Anonymous crash reporting: {} (opt-out)", "•".yellow(), "disabled".yellow())
        }
        TelemetryConsent::Unset => {
            println!(" {} Anonymous crash reporting: not decided yet", "•".cyan())
        }
    }
    println!("   Active: {}", if is_consent_granted() { "yes".green() } else { "no".red() });
    Ok(())
}