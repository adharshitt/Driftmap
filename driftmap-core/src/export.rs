use crate::scorer::BehavioralDivergenceScore;
use crate::state::StateTransition;
use axum::{routing::get, Router};
use std::fmt::Write;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::sync::watch;

pub fn render_prometheus(scores: &[BehavioralDivergenceScore]) -> String {
    let mut out = String::with_capacity(4096);

    let _ = writeln!(
        out,
        "# HELP driftmap_score Behavioral divergence score 0.0-1.0"
    );
    let _ = writeln!(out, "# TYPE driftmap_score gauge");

    for score in scores {
        let endpoint_label = score.endpoint.replace([' ', '/'], "_");
        let _ = writeln!(
            out,
            "driftmap_score{{endpoint=\"{}\"}} {}",
            endpoint_label, score.score
        );
        let _ = writeln!(
            out,
            "driftmap_score_samples{{endpoint=\"{}\"}} {}",
            endpoint_label, score.sample_count
        );
    }
    out
}

pub async fn serve_metrics(scores_rx: watch::Receiver<crate::scorer::DashboardUpdate>, port: u16) {
    if port == 0 {
        return;
    }

    let app = Router::new().route(
        "/metrics",
        get(move || {
            let update = scores_rx.borrow().clone();
            async move { render_prometheus(&update.scores) }
        }),
    );

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    if let Ok(listener) = tokio::net::TcpListener::bind(addr).await {
        tracing::info!(
            "Prometheus metrics exposed at http://0.0.0.0:{}/metrics",
            port
        );
        let _ = axum::serve(listener, app).await;
    }
}

pub fn emit_ndjson(score: &BehavioralDivergenceScore) {
    if let Ok(json) = serde_json::to_string(score) {
        println!("{}", json);
    }
}

pub async fn fire_webhook(url: &str, transition: &StateTransition) -> anyhow::Result<()> {
    if url.is_empty() {
        return Ok(());
    }

    let client = reqwest::Client::new();
    client
        .post(url)
        .json(&serde_json::json!({
            "endpoint":  transition.endpoint,
            "from":      format!("{:?}", transition.from),
            "to":        format!("{:?}", transition.to),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }))
        .timeout(Duration::from_secs(5))
        .send()
        .await?;

    Ok(())
}
