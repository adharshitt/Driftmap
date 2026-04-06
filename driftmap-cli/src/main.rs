use clap::{Parser, Subcommand};
use std::path::PathBuf;
use anyhow::Result;

mod config;

#[derive(Parser)]
#[command(name = "driftmap", about = "Runtime semantic diff for live systems", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Watch two live services and surface behavioral drift
    Watch {
        /// Path to the driftmap.toml configuration file
        #[arg(short, long, default_value = "driftmap.toml")]
        config: PathBuf,

        /// Override target A address
        #[arg(long)]
        target_a: Option<String>,

        /// Override target B address
        #[arg(long)]
        target_b: Option<String>,
    },
    /// Show recent diverging response pairs for an endpoint (Phase 3)
    Diff {
        endpoint: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    match cli.command {
        Command::Watch { config, .. } => {
            tracing::info!("Loading configuration from {:?}", config);
            let _cfg = config::load_config(config)?;
            tracing::info!("DriftMap watch mode starting (Phase 0 logic placeholder)");
        }
        Command::Diff { .. } => {
            tracing::warn!("Diff mode is not yet implemented (Phase 3 milestone)");
        }
    }

    Ok(())
}
