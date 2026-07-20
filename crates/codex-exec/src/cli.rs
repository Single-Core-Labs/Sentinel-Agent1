use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// Top‑level CLI configuration for `codex-exec`.
#[derive(Debug, Parser)]
#[command(name = "codex-exec", author, version, about = "CLI front‑end for a Codex‑style AI agent")]
pub struct Cli {
    /// Fail if the configuration file cannot be loaded.
    #[arg(long)]
    pub strict_config: bool,

    /// Run the session without persisting any state.
    #[arg(long)]
    pub ephemeral: bool,

    /// Ignore user‑level configuration files.
    #[arg(long, alias = "ignore-user-config")]
    pub ignore_user_config: bool,

    /// Emit JSON‑L (one JSON object per line) instead of colored human output.
    #[arg(short, long)]
    pub json: bool,

    /// Sub‑commands for specific tasks.
    #[command(subcommand)]
    pub subcommand: Option<SubCommand>,
}

/// Sub‑commands supported by the CLI.
#[derive(Debug, Subcommand)]
pub enum SubCommand {
    /// Resume a previously started session.
    Resume {
        /// Identifier of the session to resume.
        #[arg(value_name = "SESSION_ID")]
        session_id: String,
    },
    /// Run a code‑review operation on a file.
    Review {
        /// Path to the file that should be reviewed.
        #[arg(value_name = "PATH")]
        path: PathBuf,
    },
}
