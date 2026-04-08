use anyhow::Result;
use clap::{Parser, Subcommand};
use driftmap_core::pipeline::initialize_observability_pipeline;
use driftmap_tui::launch_terminal_dashboard;
use std::path::PathBuf;
use dialoguer::{Input, Select, theme::ColorfulTheme};
use std::fs::File;
use std::io::Write;
use std::path::Path;
use console::style;

mod config;
mod proxy;

#[derive(Parser)]
#[command(name = "driftmap", about = "Runtime semantic diff for live systems", version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
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
        #[arg(long)]
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
    Web {
        #[command(subcommand)]
        action: WebAction,
    },
    Selftest,
    Inspect {
        #[arg(long)]
        endpoint: String,
        #[arg(long, default_value = "5")]
        sample: usize,
    },
    Replay {
        #[arg(long)]
        id: i64,
    },
    Normalize {
        #[arg(long)]
        json: String,
    },
}

#[derive(Subcommand)]
enum WebAction {
    Dev,
    Deploy { #[arg(long)] project: Option<String> },
    Init,
}

#[tokio::main]
async fn main() -> Result<()> {
    // 1. Background Update Check
    let _ = tokio::spawn(async {
        if let Ok(latest) = get_latest_version().await {
            let current = env!("CARGO_PKG_VERSION");
            if latest != current {
                println!("\n{} {} is available! (current: v{})", 
                    style("◈ Update:").blue().bold(),
                    style(format!("v{}", latest)).green().bold(),
                    current
                );
                println!("Run: {}\n", style("curl -sSL https://raw.githubusercontent.com/adharshitt/Driftmap/main/install.sh | bash").cyan());
            }
        }
    });

    let cli = Cli::parse();

    // 2. Proactive Onboarding Logic
    let command = if let Some(cmd) = cli.command {
        cmd
    } else {
        println!("\n{} {}", style("DRIFT MAP").blue().bold(), style("◈").blue());
        println!("{}", style("─".repeat(50)).black().bright());

        let create_new = if !Path::new("driftmap.toml").exists() {
            let confirm = Select::with_theme(&ColorfulTheme::default())
                .with_prompt("Setup a new DriftMap observation?")
                .items(&["Yes, set up now", "No, show help"])
                .default(0)
                .interact()?;
            
            if confirm == 1 { return Ok(()); }
            true
        } else {
            let confirm = Select::with_theme(&ColorfulTheme::default())
                .with_prompt("Found existing configuration. Use it?")
                .items(&["Yes, start monitoring", "No, create new setup"])
                .default(0)
                .interact()?;
            confirm == 1
        };

        if create_new {
            run_interactive_setup().await?;
        }
        
        Command::Watch {
            config: PathBuf::from("driftmap.toml"),
            target_a: None,
            target_b: None,
        }
    };

    // 3. Command Execution
    match command {
        Command::Watch { config, target_a, target_b } => {
            let mut app_config = config::load_config(config.clone())?;
            if let Some(a) = target_a { app_config.watch.target_a = a; }
            if let Some(b) = target_b { app_config.watch.target_b = b; }

            let port_a: u16 = app_config.watch.target_a.split(':').next_back().unwrap().parse()?;
            let port_b: u16 = app_config.watch.target_b.split(':').next_back().unwrap().parse()?;
            
            let score_rx = initialize_observability_pipeline(
                app_config.watch.interface, 
                port_a, 
                port_b,
                app_config.watch.ignore_fields
            ).await?;
            
            launch_terminal_dashboard(score_rx, app_config.watch.target_a, app_config.watch.target_b).await?;
        }
        Command::Init => { run_interactive_setup().await?; }
        Command::Proxy { listen, target_a, target_b } => {
            proxy::initialize_mirror_proxy_service(&listen, &target_a, &target_b).await?;
        }
        Command::Diff { endpoint, last } => {
            let db = driftmap_core::store::Store::open(".driftmap.db")?;
            let pairs = db.recent_pairs(&endpoint, last)?;
            for p in pairs {
                println!("Diff ID {}: {} vs {} (recorded at {})", p.id, p.status_a, p.status_b, p.recorded_at);
            }
        }
        Command::Web { action } => {
            match action {
                WebAction::Dev => {
                    std::process::Command::new("npx").args(["wrangler", "pages", "dev", "cf-dashboard/public"]).status()?;
                }
                WebAction::Deploy { project } => {
                    let project_name = project.unwrap_or_else(|| "driftmap-dashboard".to_string());
                    std::process::Command::new("npx").args(["wrangler", "deploy"]).current_dir("cf-socket").status()?;
                    std::process::Command::new("npx").args(["wrangler", "pages", "deploy", "public", "--project-name", &project_name]).current_dir("cf-dashboard").status()?;
                }
                WebAction::Init => {
                    std::process::Command::new("npx").args(["wrangler", "kv", "namespace", "create", "DRIFT_DATA"]).status()?;
                }
            }
        }
        Command::Selftest => {
            println!("🧪 Starting DriftMap Self-Test Mode...");
            let server_a = tokio::spawn(async {
                let app = axum::Router::new().route("/test", axum::routing::get(|| async { "OK" }));
                let listener = tokio::net::TcpListener::bind("127.0.0.1:9090").await.unwrap();
                axum::serve(listener, app).await.unwrap();
            });
            let server_b = tokio::spawn(async {
                let app = axum::Router::new().route("/test", axum::routing::get(|| async { "OK" }));
                let listener = tokio::net::TcpListener::bind("127.0.0.1:9091").await.unwrap();
                axum::serve(listener, app).await.unwrap();
            });
            println!("✅ Internal test servers running.");
            let score_rx = match initialize_observability_pipeline("lo".to_string(), 9090, 9091, vec![]).await {
                Ok(rx) => rx,
                Err(e) => { println!("❌ Failed: {}. (Are you root?)", e); return Ok(()); }
            };
            let client = reqwest::Client::new();
            for _ in 0..100 {
                let _ = client.get("http://127.0.0.1:9090/test").send().await;
                let _ = client.get("http://127.0.0.1:9091/test").send().await;
            }
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
            let update = score_rx.borrow().clone();
            if update.scores.is_empty() { println!("❌ No traffic captured."); }
            else {
                let total: f32 = update.scores.iter().map(|s| s.score).sum();
                if total == 0.0 { println!("✅ SELF-TEST PASSED: 0.0% drift."); }
                else { println!("❌ SELF-TEST FAILED: {:.4}% drift.", total * 100.0); }
            }
            server_a.abort(); server_b.abort();
        }
        Command::Inspect { endpoint, sample } => {
            let db = driftmap_core::store::Store::open(".driftmap.db")?;
            let pairs = db.recent_pairs(&endpoint, sample)?;
            for p in pairs {
                println!("\n--- Drift ID: {} ---", p.id);
                println!("A: {}\nB: {}", String::from_utf8_lossy(&p.body_a), String::from_utf8_lossy(&p.body_b));
            }
        }
        Command::Replay { id } => {
            let db = driftmap_core::store::Store::open(".driftmap.db")?;
            if let Some(p) = db.get_pair_by_id(id)? {
                let mut scorer = driftmap_core::scorer::Scorer::new(vec![]);
                let score = scorer.score_pair(&p.endpoint, p.status_a, p.status_b, &p.body_a, &p.body_b);
                println!("Verified Replay Score: {:.1}%", score * 100.0);
            }
        }
        Command::Normalize { json } => {
            let norm = driftmap_core::semantic::SemanticNormalizer::new(vec![]);
            if let Some(n) = norm.normalize(json.as_bytes()) {
                println!("{}", String::from_utf8_lossy(&n));
            }
        }
    }
    Ok(())
}

async fn run_interactive_setup() -> Result<()> {
    let theme = ColorfulTheme::default();
    println!("\n{} {}", style("DRIFT MAP").blue().bold(), style("◈").blue());
    println!("{}", style("─".repeat(50)).black().bright());

    println!("\n{}", style("1. Capture Targets").bold());
    let target_a: String = Input::with_theme(&theme)
        .with_prompt("What is your current live URL?")
        .with_initial_text("https://")
        .interact_text()?;

    let target_b: String = Input::with_theme(&theme)
        .with_prompt("What is your new version URL?")
        .with_initial_text("https://")
        .interact_text()?;

    println!("\n{}", style("2. Observation Strategy").bold());
    let mode_idx = Select::with_theme(&theme)
        .with_prompt("How should we send test traffic?")
        .items(&[
            "I will send it manually",
            "Replay my last 100 requests automatically",
            "Use a traffic template (REST, GraphQL, gRPC)"
        ])
        .default(0)
        .interact()?;
    
    let traffic_mode = if mode_idx == 0 { "capture" } else { "proxy" };

    println!("\n{}", style("3. Credentials (Optional)").bold());
    let token: String = Input::with_theme(&theme)
        .with_prompt("GitHub Token (for deep diff analysis)")
        .allow_empty(true)
        .interact_text()?;

    let interfaces = std::fs::read_dir("/sys/class/net")?
        .filter_map(|e| e.ok().map(|i| i.file_name().into_string().unwrap_or_default()))
        .filter(|name| name != "lo").collect::<Vec<String>>();

    let interface = if interfaces.len() == 1 { interfaces[0].clone() } 
                    else if !interfaces.is_empty() {
                        let idx = Select::with_theme(&theme).with_prompt("Capture location").items(&interfaces).default(0).interact()?;
                        interfaces[idx].clone()
                    } else { "any".to_string() };

    let toml_content = format!(r#"[watch]
mode = "{}"
interface = "{}"
target_a = "{}"
target_b = "{}"
github_token = "{}"
ignore_fields = ["id", "timestamp"]
"#, traffic_mode, interface, target_a, target_b, token);

    let mut file = File::create("driftmap.toml")?;
    file.write_all(toml_content.as_bytes())?;
    println!("\n{} {}", style("[+]").green(), style("Setup complete. Run 'driftmap' to start.").bold());
    Ok(())
}

async fn get_latest_version() -> Result<String> {
    let client = reqwest::Client::builder().user_agent("DriftMap-Update-Checker").build()?;
    let res = client.get("https://api.github.com/repos/adharshitt/Driftmap/releases/latest").send().await?;
    if res.status() == 200 {
        let json: serde_json::Value = res.json().await?;
        if let Some(tag) = json["tag_name"].as_str() {
            return Ok(tag.trim_start_matches('v').to_string());
        }
    }
    anyhow::bail!("Failed to fetch version")
}
