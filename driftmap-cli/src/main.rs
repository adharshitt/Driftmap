use clap::{Parser, Subcommand};
use std::path::PathBuf;
use anyhow::Result;
use driftmap_core::pipeline::initialize_observability_pipeline;
use driftmap_tui::launch_terminal_dashboard;

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
        #[arg(long, default_value = "10")]
        last: usize,
    },
    Init,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Only init tracing if we aren't running the TUI, 
    // but for now we'll write to a file or disable it to not break the terminal.
    // tracing_subscriber::fmt::init(); 

    let cli = Cli::parse();

    match cli.command {
        Command::Watch { config, target_a, target_b } => {
            let mut application_config = config::load_config(config.clone())?;
            if let Some(a) = target_a { application_config.watch.target_a = a; }
            if let Some(b) = target_b { application_config.watch.target_b = b; }

            let port_a: u16 = application_config.watch.target_a.split(':').last().unwrap().parse()?;
            let port_b: u16 = application_config.watch.target_b.split(':').last().unwrap().parse()?;

            // Start pipeline and get the score receiver
            let score_rx = initialize_observability_pipeline(application_config.watch.interface, port_a, port_b).await?;
            
            
            // Start Config Hot-Reload Task
            let config_path = config.clone();
            tokio::spawn(async move {
                use notify::{Watcher, RecursiveMode, RecommendedWatcher, Event};
                let (tx, mut rx) = tokio::sync::mpsc::channel(1);
                let mut watcher = RecommendedWatcher::new(move |res| {
                    if let Ok(Event { .. }) = res {
                        let _ = tx.blocking_send(());
                    }
                }, notify::Config::default()).unwrap();
                
                let _ = watcher.watch(&config_path, RecursiveMode::NonRecursive);
                while let Some(_) = rx.recv().await {
                    tracing::info!("driftmap.toml changed! Reloading config...");
                    // In a full implementation, we'd trigger a pipeline reload here
                }
            });

            // Hand over main thread to TUI
            launch_terminal_dashboard(score_rx).await?;
        }
        Command::Proxy { listen, target_a, target_b } => {
            let _ = proxy::initialize_mirror_proxy_service(&listen, &target_a, &target_b).await;
        }
        Command::Diff { endpoint, last } => {
            let store = driftmap_core::store::Store::open(".driftmap.db")?;
            let pairs = store.recent_pairs(&endpoint, last)?;
            if pairs.is_empty() {
                println!("No diverging pairs found for endpoint: {}", endpoint);
                return Ok(());
            }
            
            for pair in pairs {
                println!("\n\x1b[1m=== Diff at {} ===\x1b[0m", pair.recorded_at);
                println!("Target A Status: {} | Target B Status: {}", pair.status_a, pair.status_b);
                
                let body_a_str = String::from_utf8_lossy(&pair.body_a);
                let body_b_str = String::from_utf8_lossy(&pair.body_b);
                
                let diff = similar::TextDiff::from_lines(&body_a_str, &body_b_str);
                for change in diff.iter_all_changes() {
                    let (sign, style) = match change.tag() {
                        similar::ChangeTag::Delete => ("-", console::Style::new().red()),
                        similar::ChangeTag::Insert => ("+", console::Style::new().green()),
                        similar::ChangeTag::Equal => (" ", console::Style::new().dim()),
                    };
                    print!("{}{}", style.apply_to(sign), style.apply_to(change));
                }
            }
        }
        Command::Init => {
            use dialoguer::Input;
            use std::fs::File;
            use std::io::Write;

            println!("Welcome to DriftMap Init Wizard\n");
            
            let interface: String = Input::new()
                .with_prompt("Which interface should DriftMap listen on?")
                .default("eth0".into())
                .interact_text()?;

            let target_a: String = Input::new()
                .with_prompt("Target A address (e.g., 127.0.0.1:3000)")
                .default("127.0.0.1:3000".into())
                .interact_text()?;

            let target_b: String = Input::new()
                .with_prompt("Target B address (e.g., 127.0.0.1:3001)")
                .default("127.0.0.1:3001".into())
                .interact_text()?;

            let toml_content = format!(
r#"[watch]
interface = "{}"
target_a = "{}"
target_b = "{}"
"#, interface, target_a, target_b);

            let mut file = File::create("driftmap.toml")?;
            file.write_all(toml_content.as_bytes())?;
            
            println!("\n✅ Created driftmap.toml successfully!");
        }

    }

    Ok(())
}
