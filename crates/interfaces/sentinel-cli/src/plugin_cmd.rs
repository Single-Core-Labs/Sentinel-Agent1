use colored::*;
use std::path::{Path, PathBuf};

fn plugins_dir() -> PathBuf {
    if let Ok(home) = std::env::var("SENTINEL_HOME") {
        PathBuf::from(home).join("plugins")
    } else {
        let base = std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .unwrap_or_else(|_| ".".into());
        PathBuf::from(base).join(".sentinel").join("plugins")
    }
}

pub async fn run(args: &[String]) -> anyhow::Result<()> {
    let sub = args.first().map(|s| s.as_str()).unwrap_or("help");

    match sub {
        "install" => cmd_install(&args[1..]).await,
        "list" => cmd_list().await,
        "remove" | "uninstall" => cmd_remove(&args[1..]).await,
        "help" | "--help" | "-h" => {
            println!("{}", "Plugin commands:".yellow().bold());
            println!("  sentinel plugin install <dir>   Install a plugin from a local directory");
            println!("  sentinel plugin list            List installed plugins");
            println!("  sentinel plugin remove <id>      Remove an installed plugin");
            println!();
            println!("{}", "Plugin format:".yellow().bold());
            println!("  A directory containing sentinel-plugin.toml:");
            println!("    id = \"policy-guard\"");
            println!("    name = \"Policy Guard\"");
            println!("    version = \"0.1.0\"");
            println!("    description = \"...\"");
            println!();
            println!("    [hooks]");
            println!(
                "    before_tool_call = \"./policy.sh\"  # stdout: allow | veto <reason> | deny <reason>"
            );
            println!("    after_tool_call = \"./log.sh\"");
            Ok(())
        }
        _ => {
            eprintln!(
                "{} Unknown plugin subcommand: '{}'",
                "Error:".red().bold(),
                sub
            );
            std::process::exit(1);
        }
    }
}

async fn cmd_install(args: &[String]) -> anyhow::Result<()> {
    let src = match args.first() {
        Some(p) => PathBuf::from(p),
        None => {
            eprintln!(
                "{} Usage: sentinel plugin install <dir>",
                "Error:".red().bold()
            );
            std::process::exit(1);
        }
    };

    if !src.is_dir() {
        eprintln!(
            "{} Plugin source '{}' is not a directory",
            "Error:".red().bold(),
            src.display()
        );
        std::process::exit(1);
    }

    let manifest = sentinel_plugin_system::PluginFileManifest::load(&src)
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    let dest = plugins_dir().join(&manifest.id);
    if dest.exists() {
        eprintln!(
            "{} Plugin '{}' is already installed — remove it first",
            "Error:".red().bold(),
            manifest.id
        );
        std::process::exit(1);
    }

    copy_dir(&src, &dest)?;

    println!(
        " {} Installed plugin {}",
        "✓".green().bold(),
        manifest.name.green().bold()
    );
    println!("   id:      {}", manifest.id.yellow());
    println!("   version: {}", manifest.version.yellow());
    println!("   dir:     {}", dest.display().to_string().dimmed());
    println!();
    println!(
        " {} It will be loaded automatically on the next 'sentinel ai' run.",
        "→".cyan().bold()
    );
    Ok(())
}

async fn cmd_list() -> anyhow::Result<()> {
    let dir = plugins_dir();
    let plugins = sentinel_plugin_system::load_plugins_dir(&dir);

    if plugins.is_empty() {
        println!(
            " {} No plugins installed (dir: {}).",
            "●".cyan().bold(),
            dir.display().to_string().dimmed()
        );
        println!("   Run 'sentinel plugin install <dir>' to add one.");
        return Ok(());
    }

    println!(" {} Installed plugins:", "●".cyan().bold());
    for p in &plugins {
        let m = p.manifest();
        println!(
            "  {} v{} — {}",
            m.name.green().bold(),
            m.version.yellow(),
            m.description.dimmed()
        );
        println!("    id: {}", m.id.to_string().dimmed());
    }
    Ok(())
}

async fn cmd_remove(args: &[String]) -> anyhow::Result<()> {
    let id = match args.first() {
        Some(p) => p.clone(),
        None => {
            eprintln!(
                "{} Usage: sentinel plugin remove <id>",
                "Error:".red().bold()
            );
            std::process::exit(1);
        }
    };

    let dest = plugins_dir().join(&id);
    if !dest.exists() {
        eprintln!("{} Plugin '{}' is not installed", "Error:".red().bold(), id);
        std::process::exit(1);
    }

    std::fs::remove_dir_all(&dest)?;
    println!(" {} Removed plugin '{}'", "✓".green().bold(), id.yellow());
    Ok(())
}

fn copy_dir(src: &Path, dest: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dest)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dest.join(entry.file_name());
        if from.is_dir() {
            copy_dir(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}
