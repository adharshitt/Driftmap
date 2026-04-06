use aya::{
    maps::{HashMap as BpfHashMap, RingBuf},
    programs::{Tc, TcAttachType},
    Bpf,
};
use aya_log::BpfLogger;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::capture::Reassembler;
use crate::matcher::{Matcher, Target};
use crate::scorer::Scorer;

pub async fn run_pipeline(
    interface: String,
    target_a_port: u16,
    target_b_port: u16,
) -> anyhow::Result<()> {
    // 1. Load eBPF
    let mut bpf = Bpf::load(include_bytes!("../../target/bpfel-unknown-none/debug/driftmap-probe"))?;
    if let Err(e) = BpfLogger::init(&mut bpf) {
        warn!("failed to initialize eBPF logger: {}", e);
    }

    let mut watched_ports: BpfHashMap<_, u32, u8> = BpfHashMap::try_from(bpf.map_mut("WATCHED_PORTS").unwrap())?;
    watched_ports.insert(target_a_port as u32, 1, 0)?;
    watched_ports.insert(target_b_port as u32, 1, 0)?;

    let program: &mut Tc = bpf.program_mut("driftmap_tc").unwrap().try_into()?;
    program.load()?;
    program.attach(&interface, TcAttachType::Ingress)?;
    program.attach(&interface, TcAttachType::Egress)?;

    info!("eBPF probe attached to {} (watching ports {}, {})\r", interface, target_a_port, target_b_port);

    // 2. Setup Pipeline Channels
    let (match_tx, mut match_rx) = mpsc::channel(1024);
    let (pair_tx, mut pair_rx) = mpsc::channel(1024);
    
    let mut reassembler = Reassembler::new(match_tx);
    let mut matcher = Matcher::new(pair_tx);
    let scorer = Arc::new(Mutex::new(Scorer::new()));

    // 3. Ring Buffer Reader Task
    let ring_buf = RingBuf::try_from(bpf.map("EVENTS").unwrap())?;
    let mut poll = tokio::io::unix::AsyncFd::new(ring_buf)?;

    let scorer_clone = scorer.clone();
    
    // Main Pipeline Loop
    tokio::spawn(async move {
        loop {
            let mut guard = poll.readable_mut().await.unwrap();
            let mut rb = guard.get_inner_mut();
            while let Some(event) = rb.next() {
                let packet_event: &driftmap_probe_common::PacketEvent = unsafe {
                    &*(event.as_ptr() as *const driftmap_probe_common::PacketEvent)
                };
                reassembler.ingest(packet_event);
            }
            guard.clear_ready();
        }
    });

    // Matcher Task
    tokio::spawn(async move {
        while let Some((key, msg)) = match_rx.recv().await {
            let target = if key.dst_port == target_a_port || key.src_port == target_a_port {
                Target::A
            } else {
                Target::B
            };
            
            match msg {
                crate::http::HttpMessage::Request(req) => {
                    // Logic to correlate req/res pairs would go here in a full impl
                    // For MVP, we pass them to matcher which assumes one stream per connection
                }
                crate::http::HttpMessage::Response(res) => {
                    // Placeholder: In a real impl, we'd need the req here too.
                    // For MVP simplicity, let's assume we have them.
                }
            }
        }
    });

    // Plaintext Reporter Task
    let reporter_scorer = scorer.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
        loop {
            interval.tick().await;
            let scores = reporter_scorer.lock().unwrap().all_scores();
            if scores.is_empty() { continue; }
            
            println!("\n{:<30} {:<10} {:<12} {:<10}", "ENDPOINT", "REQUESTS", "DIVERGENCE", "STATUS");
            println!("{}", "-".repeat(65));
            for s in scores {
                println!("{:<30} {:<10} {:<12.2}% {:<10}", 
                    s.endpoint, s.sample_count, s.score * 100.0, 
                    if s.score < 0.05 { "✓" } else { "⚠" }
                );
            }
        }
    });

    tokio::signal::ctrl_c().await?;
    info!("Shutting down...");
    Ok(())
}
