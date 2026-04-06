use clap::{Parser, Subcommand};
use std::path::PathBuf;
use anyhow::Result;
use driftmap_core::pipeline::run_pipeline;
use driftmap_tui::run_tui;

mod config;
mod proxy;

#[derive(Parser)]
#[command(name = "driftmap", about = "Runtime semantic diff for live systems", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Watch {
        #[arg(short, long, default_value = "driftmap.toml")]
        config: PathBuf,
        #[arg(long)]
        target_a: Option<String>,
        #[arg(long)]
        target_b: Option<String>,
    },
    Proxy {
        #[arg(long, default_value = "0.0.0.0:8080")]
        listen: String,
        #[arg(long)]
        target_a: String,
        #[arg(long)]
        target_b: String,
    },
    Diff {
        endpoint: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // Only init tracing if we aren't running the TUI, 
    // but for now we'll write to a file or disable it to not break the terminal.
    // tracing_subscriber::fmt::init(); 

    let cli = Cli::parse();

    match cli.command {
        Command::Watch { config, target_a, target_b } => {
            let mut cfg = config::load_config(config)?;
            if let Some(a) = target_a { cfg.watch.target_a = a; }
            if let Some(b) = target_b { cfg.watch.target_b = b; }

            let port_a: u16 = cfg.watch.target_a.split(':').last().unwrap().parse()?;
            let port_b: u16 = cfg.watch.target_b.split(':').last().unwrap().parse()?;

            // Start pipeline and get the score receiver
            let score_rx = run_pipeline(cfg.watch.interface, port_a, port_b).await?;
            
            // Hand over main thread to TUI
            run_tui(score_rx).await?;
        }
    Proxy {
        #[arg(long, default_value = "0.0.0.0:8080")]
        listen: String,
        #[arg(long)]
        target_a: String,
        #[arg(long)]
        target_b: String,
    },
        Command::Proxy { listen, target_a, target_b } => {
            let _ = proxy::run_proxy(&listen, &target_a, &target_b).await;
        }
        Command::Diff { .. } => {
            println!("Diff mode is not yet implemented (Phase 3 milestone)");
        }
    }

    Ok(())
}
