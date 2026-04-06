use aya::{
    maps::{HashMap as BpfHashMap, RingBuf},
    programs::{Tc, TcAttachType},
    Bpf,
};
use aya_log::BpfLogger;
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, watch};
use tracing::{info, warn};

use crate::capture::Reassembler;
use crate::matcher::{Matcher, Target};
use crate::scorer::{Scorer, DriftScore};

pub async fn run_pipeline(
    interface: String,
    target_a_port: u16,
    target_b_port: u16,
) -> anyhow::Result<watch::Receiver<Vec<DriftScore>>> {
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

    info!("eBPF probe attached to {} (watching ports {}, {})", interface, target_a_port, target_b_port);

    let (match_tx, mut match_rx) = mpsc::channel(1024);
    let (pair_tx, mut pair_rx) = mpsc::channel(1024);
    let (score_tx, score_rx) = watch::channel(Vec::new());
    
    let mut reassembler = Reassembler::new(match_tx);
    let mut matcher = Matcher::new(pair_tx);
    let scorer = Arc::new(Mutex::new(Scorer::new()));

    let ring_buf = RingBuf::try_from(bpf.map("EVENTS").unwrap())?;
    let mut poll = tokio::io::unix::AsyncFd::new(ring_buf)?;

    let scorer_clone = scorer.clone();
    
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

    tokio::spawn(async move {
        while let Some((key, msg)) = match_rx.recv().await {
            // Simplified MVP matching logic
        }
    });

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(500));
        loop {
            interval.tick().await;
            let scores = scorer_clone.lock().unwrap().all_scores();
            let _ = score_tx.send(scores);
        }
    });

    Ok(score_rx)
}
