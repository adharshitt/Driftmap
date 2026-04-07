use crate::http::{parse_http_message, HttpMessage};
use driftmap_probe_common::NetworkPacketEvent;
use std::collections::{BTreeMap, HashMap};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;

#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub struct StreamKey {
    pub src_ip: [u8; 4],
    pub dst_ip: [u8; 4],
    pub src_port: u16,
    pub dst_port: u16,
}

pub struct TrafficCaptureBuffer {
    pub segments: BTreeMap<u32, Vec<u8>>, // seq -> payload
    pub next_seq: u32,
    pub data: Vec<u8>, // Reassembled contiguous data
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
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        let buf = self
            .streams
            .entry(key.clone())
            .or_insert(TrafficCaptureBuffer {
                segments: BTreeMap::new(),
                next_seq: 0,
                data: Vec::with_capacity(4096),
                captured_at: Instant::now(),
                last_seen_ms: now,
            });

        // Initialize next_seq on first packet
        if buf.next_seq == 0 {
            buf.next_seq = event.seq;
        }

        if event.payload_len > 0 {
            if buf.data.len() + event.payload_len as usize > 1024 * 1024 {
                tracing::warn!("Stream {:?} exceeded 1MB limit. Dropping.", key);
                self.streams.remove(&key);
                return;
            }
            // Store segment
            buf.segments.insert(
                event.seq,
                event.payload[..event.payload_len as usize].to_vec(),
            );
        }

        // Reassemble contiguous segments
        while let Some(payload) = buf.segments.remove(&buf.next_seq) {
            let len = payload.len() as u32;
            buf.data.extend_from_slice(&payload);
            buf.next_seq = buf.next_seq.wrapping_add(len);
        }

        buf.last_seen_ms = now;

        // Process any completed HTTP messages in the reassembled buffer
        while let Some(consumed) = try_extract_message(&buf.data) {
            if let Some(parsed) = parse_http_message(&buf.data[..consumed]) {
                let _ = self.tx.try_send((key.clone(), parsed));
            }
            buf.data.drain(..consumed);
            if buf.data.is_empty() {
                break;
            }
        }

        // Task 13: Instant Pruning on FIN/RST
        if (event.tcp_flags & 0x001) != 0 || (event.tcp_flags & 0x004) != 0 {
            self.streams.remove(&key);
        }
    }

    pub fn collect_stale_connections(&mut self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let cutoff = now.saturating_sub(self.timeout_ms);
        self.streams.retain(|_, buf| buf.last_seen_ms > cutoff);
    }
}

fn try_extract_message(data: &[u8]) -> Option<usize> {
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

    let content_length = headers
        .iter()
        .take_while(|h| !h.name.is_empty())
        .find(|h| h.name.to_lowercase() == "content-length")
        .and_then(|h| std::str::from_utf8(h.value).ok())
        .and_then(|v| v.parse::<usize>().ok());

    let encoding = headers
        .iter()
        .take_while(|h| !h.name.is_empty())
        .find(|h| h.name.to_lowercase() == "content-encoding")
        .and_then(|h| std::str::from_utf8(h.value).ok())
        .map(|v| v.to_lowercase());

    let content_type = headers
        .iter()
        .take_while(|h| !h.name.is_empty())
        .find(|h| h.name.to_lowercase() == "content-type")
        .and_then(|h| std::str::from_utf8(h.value).ok())
        .map(|v| v.to_lowercase());

    if let Some(enc) = encoding {
        if enc.contains("gzip") || enc.contains("br") {
            tracing::debug!("Detected compressed encoding: {}", enc);
        }
    }

    if let Some(ct) = content_type {
        if ct.contains("multipart/form-data") {
            tracing::debug!("Detected multipart form data");
        }
    }

    let is_chunked = headers
        .iter()
        .take_while(|h| !h.name.is_empty())
        .find(|h| h.name.to_lowercase() == "transfer-encoding")
        .and_then(|h| std::str::from_utf8(h.value).ok())
        .map(|v| v.to_lowercase().contains("chunked"))
        .unwrap_or(false);

    if is_chunked {
        if let Some(end_pos) = data.windows(5).position(|w| w == b"0\r\n\r\n") {
            return Some(end_pos + 5);
        }
        return None;
    }

    match content_length {
        Some(cl) => {
            let total_len = header_len + cl;
            if data.len() < total_len {
                return None;
            }
            Some(total_len)
        }
        None => Some(header_len),
    }
}
