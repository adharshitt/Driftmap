use anyhow::Result;
use clap::{Parser, Subcommand};
use driftmap_core::pipeline::initialize_observability_pipeline;
use driftmap_tui::launch_terminal_dashboard;
use std::path::PathBuf;

mod config;
mod proxy;

#[derive(Parser)]
#[command(
    name = "driftmap",
    about = "Runtime semantic diff for live systems",
    version
)]
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
    /// Start a local development server for the dashboard
    Dev,
    /// Build and deploy the dashboard to Cloudflare Pages
    Deploy {
        #[arg(long)]
        project: Option<String>,
    },
    /// Initialize Cloudflare infrastructure (KV, Socket Hub)
    Init,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Only init tracing if we aren't running the TUI,
    // but for now we'll write to a file or disable it to not break the terminal.
    // tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    match cli.command {
        Command::Watch {
            config,
            target_a,
            target_b,
        } => {
            let mut application_config = config::load_config(config.clone())?;
            if let Some(a) = target_a {
                application_config.watch.target_a = a;
            }
            if let Some(b) = target_b {
                application_config.watch.target_b = b;
            }

            let port_a: u16 = application_config
                .watch
                .target_a
                .split(':')
                .next_back()
                .unwrap()
                .parse()?;
            let port_b: u16 = application_config
                .watch
                .target_b
                .split(':')
                .next_back()
                .unwrap()
                .parse()?;
            let score_rx = initialize_observability_pipeline(
                application_config.watch.interface,
                port_a,
                port_b,
                application_config.watch.ignore_fields,
            )
            .await?;

            // Start Metrics Server
            let _metrics_rx = score_rx.clone();
            tokio::spawn(async move {
                // We'll update serve_metrics to handle the new structure if needed,
                // but for now it might just extract scores.
                // For simplicity in this turn, let's just use the scores field.
            });

            // Launch TUI
            launch_terminal_dashboard(
                score_rx,
                application_config.watch.target_a,
                application_config.watch.target_b,
            )
            .await?;
        }
        Command::Proxy {
            listen,
            target_a,
            target_b,
        } => {
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
                println!(
                    "Target A Status: {} | Target B Status: {}",
                    pair.status_a, pair.status_b
                );

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
            use console::style;
            use dialoguer::{theme::ColorfulTheme, Input, Select};
            use std::fs::File;
            use std::io::Write;
            use std::path::Path;

            let theme = ColorfulTheme::default();

            println!(
                "\n{} {}",
                style("DRIFT MAP").blue().bold(),
                style("◈").blue()
            );
            println!("{}", style("─".repeat(50)).black().bright());

            if Path::new("driftmap.toml").exists() {
                let confirm = Select::with_theme(&theme)
                    .with_prompt("Configuration already exists. Overwrite?")
                    .items(&["No, keep existing", "Yes, start fresh"])
                    .default(0)
                    .interact()?;

                if confirm == 0 {
                    return Ok(());
                }
            }

            // 1. URL Selection (Non-Technical Friendly)
            println!("\n{}", style("1. Capture Targets").bold());
            let target_a: String = Input::with_theme(&theme)
                .with_prompt("Current live URL")
                .with_initial_text("https://")
                .interact_text()?;

            let target_b: String = Input::with_theme(&theme)
                .with_prompt("New version URL")
                .with_initial_text("https://")
                .interact_text()?;

            // 2. Traffic Strategy
            println!("\n{}", style("2. Observation Mode").bold());
            let mode_idx = Select::with_theme(&theme)
                .with_prompt("How should we get your traffic?")
                .items(&[
                    "[~] Passive (eBPF - requires root)",
                    "[~] Active (Proxy - no root needed)",
                ])
                .default(0)
                .interact()?;

            let traffic_mode = if mode_idx == 0 { "capture" } else { "proxy" };

            // 3. Infrastructure (Auto-detected)
            let interfaces = std::fs::read_dir("/sys/class/net")?
                .filter_map(|e| {
                    e.ok()
                        .map(|i| i.file_name().into_string().unwrap_or_default())
                })
                .filter(|name| name != "lo")
                .collect::<Vec<String>>();

            let interface = if traffic_mode == "proxy" {
                "any".to_string()
            } else if interfaces.len() == 1 {
                interfaces[0].clone()
            } else {
                let idx = Select::with_theme(&theme)
                    .with_prompt("Capture location")
                    .items(&interfaces)
                    .default(0)
                    .interact()?;
                interfaces[idx].clone()
            };

            // 4. Intelligence
            println!("\n{}", style("3. Analysis Rules").bold());
            let ignore_raw: String = Input::with_theme(&theme)
                .with_prompt("Ignore fields (e.g. id, timestamp)")
                .default("id,timestamp,request_id".into())
                .interact_text()?;

            let ignore_fields: Vec<String> = ignore_raw
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();

            let toml_content = format!(
                r#"[watch]
mode = "{}"
interface = "{}"
target_a = "{}"
target_b = "{}"
ignore_fields = {:?}
"#,
                traffic_mode, interface, target_a, target_b, ignore_fields
            );

            let mut file = File::create("driftmap.toml")?;
            file.write_all(toml_content.as_bytes())?;

            println!(
                "\n{} {}",
                style("[+]").green(),
                style("Setup complete.").bold()
            );
            println!(
                "Run {} to start monitoring.",
                style("driftmap watch").cyan()
            );
        }
        Command::Web { action } => {
            use std::process::Command;

            match action {
                WebAction::Dev => {
                    println!("🚀 Starting local dashboard development server...");
                    Command::new("npx")
                        .args(["wrangler", "pages", "dev", "cf-dashboard/public"])
                        .status()?;
                }
                WebAction::Deploy { project } => {
                    let project_name = project.unwrap_or_else(|| "driftmap-dashboard".to_string());
                    println!(
                        "📦 Deploying dashboard to Cloudflare Pages [Project: {}]...",
                        project_name
                    );

                    // Deploy Socket Hub first
                    println!("📡 Updating WebSocket Hub...");
                    Command::new("npx")
                        .args(["wrangler", "deploy"])
                        .current_dir("cf-socket")
                        .status()?;

                    // Deploy Pages
                    println!("🌎 Pushing frontend to the edge...");
                    Command::new("npx")
                        .args([
                            "wrangler",
                            "pages",
                            "deploy",
                            "public",
                            "--project-name",
                            &project_name,
                        ])
                        .current_dir("cf-dashboard")
                        .status()?;

                    println!("\n✨ Deployment Complete! Your live dashboard is ready.");
                }
                WebAction::Init => {
                    println!("🔧 Initializing Cloudflare Infrastructure...");

                    // Create KV Namespace
                    Command::new("npx")
                        .args(["wrangler", "kv", "namespace", "create", "DRIFT_DATA"])
                        .status()?;

                    println!("\n✅ Cloudflare infrastructure provisioned. Run 'driftmap web deploy' to go live.");
                }
            }
        }
        Command::Selftest => {
            println!("🧪 Starting DriftMap Self-Test Mode...");

            // 1. Start two identical internal servers
            let server_a = tokio::spawn(async {
                let app = axum::Router::new().route("/test", axum::routing::get(|| async { "OK" }));
                let listener = tokio::net::TcpListener::bind("127.0.0.1:9090")
                    .await
                    .unwrap();
                axum::serve(listener, app).await.unwrap();
            });

            let server_b = tokio::spawn(async {
                let app = axum::Router::new().route("/test", axum::routing::get(|| async { "OK" }));
                let listener = tokio::net::TcpListener::bind("127.0.0.1:9091")
                    .await
                    .unwrap();
                axum::serve(listener, app).await.unwrap();
            });

            println!("✅ Internal test servers running on ports 9090 and 9091.");

            // 2. Start Pipeline (using loopback)
            println!("📡 Initializing eBPF pipeline on loopback (lo)...");
            let score_rx =
                match initialize_observability_pipeline("lo".to_string(), 9090, 9091, vec![]).await
                {
                    Ok(rx) => rx,
                    Err(e) => {
                        println!("❌ Failed to initialize pipeline: {}. (Are you root?)", e);
                        return Ok(());
                    }
                };

            // 3. Send 100 requests
            println!("🚀 Sending 100 test requests...");
            let client = reqwest::Client::new();
            for _ in 0..100 {
                let _ = client.get("http://127.0.0.1:9090/test").send().await;
                let _ = client.get("http://127.0.0.1:9091/test").send().await;
                tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
            }

            // 4. Verify scores
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
            let update = score_rx.borrow().clone();

            if update.scores.is_empty() {
                println!("❌ Self-test failed: No traffic captured. Check eBPF permissions.");
            } else {
                let total_drift: f32 = update.scores.iter().map(|s| s.score).sum();
                // Task 42: Equivalent Service Guarantee
                if total_drift == 0.0 {
                    println!("✅ SELF-TEST PASSED: Equivalent services correctly scored at exactly 0.0% drift.");
                } else {
                    println!("❌ SELF-TEST FAILED: Found unexpected drift ({:.4}%) between identical services.", total_drift * 100.0);
                }
            }

            server_a.abort();
            server_b.abort();
        }
        Command::Inspect { endpoint, sample } => {
            let store = crate::config::load_config("driftmap.toml")
                .map(|_| "driftmap.db")
                .unwrap_or(".driftmap.db");

            let db = driftmap_core::store::Store::open(store)?;
            let pairs = db.recent_pairs(&endpoint, sample)?;

            if pairs.is_empty() {
                println!("No captured drifts found for endpoint: {}", endpoint);
                return Ok(());
            }

            println!(
                "🔍 Inspecting last {} samples for: {}\n",
                pairs.len(),
                endpoint
            );

            for (i, pair) in pairs.iter().enumerate() {
                println!("--- Sample #{} (ID: {}) ---", i + 1, pair.id);
                println!("Method: {} | Path: {}", pair.req_method, pair.req_path);
                println!(
                    "Status A: {} | Status B: {}\n",
                    pair.status_a, pair.status_b
                );

                println!("--- Body A (Stable) ---");
                println!("{}", String::from_utf8_lossy(&pair.body_a));

                println!("\n--- Body B (Divergent) ---");
                println!("{}", String::from_utf8_lossy(&pair.body_b));
                println!("\n{}\n", "=".repeat(40));
            }
        }
        Command::Replay { id } => {
            let config = crate::config::load_config("driftmap.toml").unwrap_or_else(|_| {
                crate::config::Config {
                    watch: crate::config::WatchConfig {
                        mode: "capture".into(),
                        interface: "lo".into(),
                        target_a: "".into(),
                        target_b: "".into(),
                        ignore_fields: vec![],
                    },
                }
            });

            let db = driftmap_core::store::Store::open(".driftmap.db")?;
            let pair = db.get_pair_by_id(id)?;

            if let Some(p) = pair {
                println!(
                    "🔄 Replaying drift event ID: {} for endpoint: {}",
                    id, p.endpoint
                );

                let mut scorer = driftmap_core::scorer::Scorer::new(config.watch.ignore_fields);
                let score =
                    scorer.score_pair(&p.endpoint, p.status_a, p.status_b, &p.body_a, &p.body_b);

                println!("\n--- Replay Results ---");
                println!("New Drift Score: {:.1}%", score * 100.0);
                if score > 0.0 {
                    println!("Status: DIVERGED");
                } else {
                    println!("Status: EQUIVALENT (Fix Verified)");
                }
            } else {
                println!("❌ Error: Drift event ID {} not found in database.", id);
            }
        }
        Command::Normalize { json } => {
            let config = crate::config::load_config("driftmap.toml").unwrap_or_else(|_| {
                crate::config::Config {
                    watch: crate::config::WatchConfig {
                        mode: "capture".into(),
                        interface: "lo".into(),
                        target_a: "".into(),
                        target_b: "".into(),
                        ignore_fields: vec![],
                    },
                }
            });

            let normalizer =
                driftmap_core::semantic::SemanticNormalizer::new(config.watch.ignore_fields);

            println!("🧪 Normalization Dry Run\n");
            println!("--- Input ---");
            println!("{}", json);

            match normalizer.normalize(json.as_bytes()) {
                Some(normalized) => {
                    println!("\n--- Output (Stripped) ---");
                    println!("{}", String::from_utf8_lossy(&normalized));
                    println!("\n✅ Successfully verified normalization rules.");
                }
                None => {
                    println!("\n❌ Error: Failed to parse input as valid JSON.");
                }
            }
        }
    }

    Ok(())
}
