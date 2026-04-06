use std::collections::HashMap;
use tokio::sync::mpsc;
use std::time::{SystemTime, UNIX_EPOCH, Instant};
use driftmap_probe_common::NetworkPacketEvent;
use crate::http::{parse_http_message, HttpMessage};

#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub struct StreamKey {
    pub src_ip:      [u8; 4],
    pub dst_ip:      [u8; 4],
    pub src_port: u16,
    pub dst_port: u16,
}

pub struct TrafficCaptureBuffer {
    pub data: Vec<u8>,
    pub captured_at: std::time::Instant,
    pub last_seen_ms: u64,
}

pub struct Reassembler {
    pub streams: HashMap<StreamKey, TrafficCaptureBuffer>,
    pub tx: mpsc::Sender<(StreamKey, HttpMessage)>,
    pub timeout_ms: u64,
}

impl Reassembler {
    pub fn new(tx: mpsc::Sender<(StreamKey, HttpMessage)>) -> Self {
        Self {
            streams: HashMap::new(),
            tx,
            timeout_ms: 5000,
        }
    }

    pub fn process_incoming_payload(&mut self, event: &NetworkPacketEvent) {
        let key = StreamKey {
            src_ip: event.src_ip,
            dst_ip: event.dst_ip,
            src_port: event.src_port,
            dst_port: event.dst_port,
        };

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let buf = self.streams.entry(key.clone()).or_insert(TrafficCaptureBuffer {
            data: Vec::with_capacity(4096),
            captured_at: Instant::now(),
            last_seen_ms: now,
        });

        if buf.data.len() + event.payload_len as usize > 1024 * 1024 {
            return; // OOM Prevention: Limit TCP buffer to 1MB
        }
        buf.data.extend_from_slice(&event.payload[..event.payload_len as usize]);
        buf.last_seen_ms = now;

        while let Some((msg, consumed)) = try_extract_message(&buf.data) {
            if let Some(parsed) = parse_http_message(&buf.data[..consumed]) {
                let _ = self.tx.try_send((key.clone(), parsed));
            }
            buf.data.drain(..consumed);
            if buf.data.is_empty() { break; }
        }
    }

    pub fn collect_stale_connections(&mut self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let cutoff = now.saturating_sub(self.timeout_ms);
        self.streams.retain(|_, buf| buf.last_seen_ms > cutoff);
    }
}

fn try_extract_message(data: &[u8]) -> Option<(HttpMessage, usize)> {
    let mut headers = [httparse::EMPTY_HEADER; 64];
    let header_len = if data.starts_with(b"HTTP/") {
        let mut res = httparse::Response::new(&mut headers);
        match res.parse(data) {
            Ok(httparse::Status::Complete(len)) => Some(len),
            _ => None,
        }
    } else {
        let mut req = httparse::Request::new(&mut headers);
        match req.parse(data) {
            Ok(httparse::Status::Complete(len)) => Some(len),
            _ => None,
        }
    }?;

    let content_length = headers.iter()
        .take_while(|h| !h.name.is_empty())
        .find(|h| h.name.to_lowercase() == "content-length")
        .and_then(|h| std::str::from_utf8(h.value).ok())
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);

    let total_len = header_len + content_length;
    if data.len() < total_len {
        return None;
    }

    // Note: We don't actually need the HttpMessage here, just the length.
    // parse_http_message is called by the caller.
    Some((HttpMessage::Request(/* dummy */ HttpRequest { 
        method: String::new(), path: String::new(), path_template: String::new(), 
        headers: vec![], body: vec![] 
    }), total_len))
}

// Fixed the dummy Request to match the new definition
use crate::http::HttpRequest;
