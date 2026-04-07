use aya::{
    maps::{HashMap as BpfHashMap, RingBuf},
    programs::{tc, SchedClassifier, TcAttachType},
    Ebpf,
};
use aya_log::EbpfLogger;
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, watch};
use tracing::{info, warn};

use crate::capture::Reassembler;
use crate::matcher::{Matcher, Target};
use crate::scorer::{Scorer, DashboardUpdate, SystemHealth};

pub async fn initialize_observability_pipeline(
    interface: String,
    target_a_port: u16,
    target_b_port: u16,
    ignore_fields: Vec<String>,
) -> anyhow::Result<watch::Receiver<DashboardUpdate>> {
    #[cfg(debug_assertions)]
    let mut bpf = Ebpf::load(include_bytes!("../../target/bpfel-unknown-none/debug/driftmap-probe"))?;
    #[cfg(not(debug_assertions))]
    let mut bpf = Ebpf::load(include_bytes!("../../target/bpfel-unknown-none/release/driftmap-probe"))?;
    
    if let Err(e) = EbpfLogger::init(&mut bpf) {
        warn!("failed to initialize eBPF logger: {}", e);
    }

    let mut watched_ports: BpfHashMap<_, u32, u8> = BpfHashMap::try_from(
        bpf.map_mut("FILTERED_PORT_REGISTRY")
            .ok_or_else(|| anyhow::anyhow!("FILTERED_PORT_REGISTRY map not found"))?
    )?;
    watched_ports.insert(target_a_port as u32, 1, 0)?;
    watched_ports.insert(target_b_port as u32, 1, 0)?;

    let _ = tc::qdisc_add_clsact(&interface);
    let program: &mut SchedClassifier = bpf.program_mut("intercept_traffic_control_hook")
        .ok_or_else(|| anyhow::anyhow!("intercept_traffic_control_hook program not found"))?
        .try_into()?;
    program.load()?;
    program.attach(&interface, TcAttachType::Ingress)?;
    program.attach(&interface, TcAttachType::Egress)?;

    info!("eBPF probe attached to {} (watching ports {}, {})", interface, target_a_port, target_b_port);

    let (match_tx, mut match_rx) = mpsc::channel(1024);
    let (pair_tx, _pair_rx) = mpsc::channel(1024);
    let (score_tx, score_rx) = watch::channel(DashboardUpdate {
        scores: Vec::new(),
        health: SystemHealth::default(),
    });
    
    let mut reassembler = Reassembler::new(match_tx);
    let mut matcher = Matcher::new(pair_tx);
    let scorer = Arc::new(Mutex::new(Scorer::new(ignore_fields)));
    let scorer_clone = scorer.clone();

    tokio::spawn(async move {
        let ring_buf = RingBuf::try_from(bpf.map("PACKET_EVENT_RING_BUFFER").expect("map not found")).expect("not a ringbuf");
        let mut poll = tokio::io::unix::AsyncFd::new(ring_buf).expect("failed to create AsyncFd");

        let dropped_map: Option<BpfHashMap<_, u32, u64>> = BpfHashMap::try_from(
            bpf.map("DROPPED_PACKETS").expect("DROPPED_PACKETS map not found")
        ).ok();

        loop {
            let mut guard = poll.readable_mut().await.expect("poll failed");
            let rb = guard.get_inner_mut();
            while let Some(event) = rb.next() {
                let packet_event: &driftmap_probe_common::NetworkPacketEvent = unsafe {
                    &*(event.as_ptr() as *const driftmap_probe_common::NetworkPacketEvent)
                };
                reassembler.process_incoming_payload(packet_event);
            }
            guard.clear_ready();
            
            if let Some(ref map) = dropped_map {
                if let Ok(count) = map.get(&0, 0) {
                    if count > 0 {
                        warn!("eBPF probe is dropping packets! Total dropped: {}", count);
                    }
                }
            }
        }
    });

    tokio::spawn(async move {
        while let Some((key, msg)) = match_rx.recv().await {
            let target = if key.dst_port == target_a_port || key.src_port == target_a_port {
                Target::A
            } else {
                Target::B
            };
            
            match msg {
                crate::http::HttpMessage::Request(_) => {}
                crate::http::HttpMessage::Response(res) => {
                    let dummy_req = crate::http::HttpRequest {
                        method: "GET".to_string(),
                        path: "unknown".to_string(),
                        path_template: "unknown".to_string(),
                        headers: vec![],
                        body: vec![],
                        captured_at: std::time::Instant::now(),
                    };
                    matcher.process_incoming_payload(target, dummy_req, res);
                }
            }
        }
    });

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(1000));
        loop {
            interval.tick().await;
            let scores = scorer_clone.lock().unwrap().all_scores();
            let health = SystemHealth::default(); // In a full impl, we'd pull real metrics here
            
            let _ = score_tx.send(DashboardUpdate {
                scores,
                health,
            });
        }
    });

    Ok(score_rx)
}
