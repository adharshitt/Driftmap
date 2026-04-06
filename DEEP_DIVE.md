# DriftMap — Complete Technical Deep Dive
## Every Feature. Every File. Every Decision.

> This document explains every single feature of DriftMap from the ground up —
> how each works internally, what code it requires, which packages to use vs build
> yourself, how files are organized, and how everything integrates at the system level.

---

## Table of Contents

1. [Workspace Structure](#1-workspace-structure)
2. [eBPF Probe — Traffic Capture](#2-ebpf-probe--traffic-capture)
3. [TCP Stream Reassembler](#3-tcp-stream-reassembler)
4. [HTTP Parser](#4-http-parser)
5. [Request Matcher](#5-request-matcher)
6. [Raw Diff Engine](#6-raw-diff-engine)
7. [Schema Inference Engine](#7-schema-inference-engine)
8. [Distribution Tracker](#8-distribution-tracker)
9. [Semantic Scorer](#9-semantic-scorer)
10. [Drift State Machine](#10-drift-state-machine)
11. [SQLite State Store](#11-sqlite-state-store)
12. [TUI Dashboard](#12-tui-dashboard)
13. [WASM Plugin System](#13-wasm-plugin-system)
14. [Export Layer](#14-export-layer)
15. [CLI & Config](#15-cli--config)
16. [Integration Map](#16-integration-map)
17. [Package Decision Table](#17-package-decision-table)
18. [Line Count Estimates](#18-line-count-estimates)

---

## 1. Workspace Structure

**One Cargo workspace, six crates.** Each crate has a single job. No crate imports from a crate "above" it — data flows in one direction only.

```
driftmap/
├── Cargo.toml                   ← workspace root
├── driftmap-probe/              ← eBPF programs (compiled to BPF bytecode)
│   ├── Cargo.toml
│   └── src/
│       └── main.rs              ← TC hook, ring buffer writer
├── driftmap-probe-common/       ← shared types between eBPF and userspace
│   ├── Cargo.toml
│   └── src/
│       └── lib.rs               ← PacketEvent struct (no_std)
├── driftmap-core/               ← all userspace logic
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── capture.rs           ← ring buffer reader, packet reassembly
│       ├── http.rs              ← HTTP/1.1 parser
│       ├── matcher.rs           ← request pairing engine
│       ├── diff.rs              ← raw byte/header/status diff
│       ├── schema.rs            ← JSON schema inference
│       ├── distribution.rs      ← value distribution tracker
│       ├── scorer.rs            ← semantic divergence scorer
│       ├── state.rs             ← drift state machine
│       └── store.rs             ← SQLite persistence
├── driftmap-tui/                ← terminal UI
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs
│       ├── app.rs               ← app state, event loop
│       ├── ui.rs                ← layout, widgets
│       └── events.rs            ← keyboard input handler
├── driftmap-plugin-sdk/         ← published crate for WASM plugin authors
│   ├── Cargo.toml
│   └── src/
│       └── lib.rs               ← Request/Response types, DriftScore type
└── driftmap-cli/                ← binary entry point
    ├── Cargo.toml
    └── src/
        ├── main.rs
        └── config.rs            ← TOML config schema + loader
```

**Workspace `Cargo.toml`:**
```toml
[workspace]
members = [
    "driftmap-probe",
    "driftmap-probe-common",
    "driftmap-core",
    "driftmap-tui",
    "driftmap-plugin-sdk",
    "driftmap-cli",
]
resolver = "2"

[workspace.dependencies]
# Pin versions once at workspace level, inherit in members
tokio          = { version = "1", features = ["full"] }
tracing        = "0.1"
serde          = { version = "1", features = ["derive"] }
serde_json     = "1"
anyhow         = "1"
```

**Why this split:**
- `driftmap-probe` must compile to BPF target (`bpfel-unknown-none`) — it cannot link any std crate
- `driftmap-probe-common` is `no_std` so both the eBPF program and userspace can share the same `PacketEvent` type without a second definition
- `driftmap-core` has zero UI concerns — fully testable in isolation
- `driftmap-tui` can be swapped for a web UI later without touching core logic

---

## 2. eBPF Probe — Traffic Capture

**File:** `driftmap-probe/src/main.rs`
**Estimated size:** ~120 lines

**What it does:** Attaches a TC (Traffic Control) hook to a network interface. For every TCP packet matching our configured ports, it reads the payload bytes and writes a `PacketEvent` into a ring buffer that userspace reads.

**Why TC hook and not XDP:**
- XDP fires before routing — you can't see both ingress AND egress in one hook
- TC fires after routing decisions, giving visibility into both directions from one attachment point
- TC works on virtual interfaces (veth, loopback) — XDP requires hardware offload support on some drivers

**Why not kprobes on `tcp_sendmsg`:**
- kprobes fire inside kernel context — hard to extract payload bytes safely at scale
- TC gives you the full packet at the right abstraction level

```rust
// driftmap-probe/src/main.rs
#![no_std]
#![no_main]

use aya_ebpf::{
    macros::{classifier},          // TC classifier macro
    programs::TcContext,
    maps::RingBuf,
    bindings::TC_ACT_OK,           // tell kernel to continue forwarding
};
use driftmap_probe_common::PacketEvent;

// Ring buffer map — userspace reads from this
#[map]
static EVENTS: RingBuf = RingBuf::with_byte_size(4 * 1024 * 1024, 0); // 4MB

#[classifier]
pub fn driftmap_tc(ctx: TcContext) -> i32 {
    match try_capture(ctx) {
        Ok(_) => TC_ACT_OK,   // always let packet through
        Err(_) => TC_ACT_OK,  // even on error, don't drop
    }
}

fn try_capture(ctx: TcContext) -> Result<(), ()> {
    // 1. Parse ethernet header to find IP offset
    // 2. Parse IP header to find TCP offset
    // 3. Check if dest/src port matches watched ports
    // 4. Extract payload bytes (up to 1500 bytes — MTU)
    // 5. Write PacketEvent to ring buffer

    let eth_hdr = ctx.load::<EthHdr>(0)?;
    if eth_hdr.ether_type != EtherType::Ipv4 { return Ok(()); }

    let ip_hdr = ctx.load::<Ipv4Hdr>(EthHdr::LEN)?;
    if ip_hdr.proto != IpProto::Tcp { return Ok(()); }

    let tcp_offset = EthHdr::LEN + (ip_hdr.ihl() as usize * 4);
    let tcp_hdr = ctx.load::<TcpHdr>(tcp_offset)?;

    let src_port = u16::from_be(tcp_hdr.source);
    let dst_port = u16::from_be(tcp_hdr.dest);

    // Filter: only capture if port matches one of our watched ports
    // Ports are stored in a BPF HashMap set at startup by userspace
    if !WATCHED_PORTS.get(&(src_port as u32)).is_some()
        && !WATCHED_PORTS.get(&(dst_port as u32)).is_some() {
        return Ok(());
    }

    let payload_offset = tcp_offset + (tcp_hdr.doff() as usize * 4);
    let payload_len = (ctx.len() as usize)
        .saturating_sub(payload_offset)
        .min(1500); // cap at MTU

    if payload_len == 0 { return Ok(()); }

    // Reserve space in ring buffer
    let mut event = match EVENTS.reserve::<PacketEvent>(0) {
        Some(e) => e,
        None => return Err(()), // ring buffer full — drop this sample
    };

    // Zero the struct first (BPF verifier requires initialization)
    let ev = event.as_mut_ptr();
    unsafe {
        (*ev).src_port = src_port;
        (*ev).dst_port = dst_port;
        (*ev).payload_len = payload_len as u16;
        // bpf_probe_read_kernel copies payload_len bytes into ev.payload
        bpf_probe_read_kernel_buf(
            ctx.data() + payload_offset,
            &mut (*ev).payload[..payload_len],
        )?;
    }

    event.submit(0);
    Ok(())
}
```

**Shared types — `driftmap-probe-common/src/lib.rs`** (~30 lines):
```rust
#![no_std]

#[repr(C)]
#[derive(Copy, Clone)]
pub struct PacketEvent {
    pub src_port:    u16,
    pub dst_port:    u16,
    pub payload_len: u16,
    pub payload:     [u8; 1500],  // max one MTU
}

// Safety: PacketEvent is plain data, safe to send across eBPF/userspace boundary
#[cfg(feature = "userspace")]
unsafe impl aya::Pod for PacketEvent {}
```

**Packages used:**
- `aya-ebpf` — eBPF program authoring in Rust (BUILD, not in std userspace tree)
- `aya-log-ebpf` — optional debug logging from eBPF side

**No packages built from scratch here** — Aya handles all the BPF map/helper abstractions. The 120 lines of BPF code is the minimum; the kernel does the heavy lifting.

---

## 3. TCP Stream Reassembler

**File:** `driftmap-core/src/capture.rs`
**Estimated size:** ~200 lines

**The problem:** TCP is a stream protocol. A single HTTP request may arrive as 3 separate TCP segments. The eBPF probe gives us raw segments. We need to reassemble them into complete HTTP messages before we can parse anything.

**How it works:**

```
PacketEvent(port=3000, seq=100, payload="GET /api") arrives
PacketEvent(port=3000, seq=108, payload="/users HTTP/1.1\r\n") arrives
PacketEvent(port=3000, seq=124, payload="Host: localhost\r\n\r\n") arrives
                                ↓
StreamReassembler buffers by (src_ip, src_port, dst_port)
Detects HTTP message boundary (\r\n\r\n)
Emits complete HttpMessage to the next stage
```

```rust
// driftmap-core/src/capture.rs

use std::collections::HashMap;
use tokio::sync::mpsc;
use aya::maps::ring_buf::RingBuf;
use crate::http::parse_http_message;

/// One TCP stream identified by its 4-tuple
#[derive(Hash, Eq, PartialEq, Clone)]
struct StreamKey {
    src_ip:   [u8; 4],
    src_port: u16,
    dst_ip:   [u8; 4],
    dst_port: u16,
}

/// Accumulated bytes for one stream, waiting for a complete HTTP message
struct StreamBuffer {
    data:         Vec<u8>,
    last_seen_ms: u64,
}

pub struct Reassembler {
    streams:   HashMap<StreamKey, StreamBuffer>,
    tx:        mpsc::Sender<RawHttpMessage>,
    timeout_ms: u64,  // expire stale buffers after N ms (default: 5000)
}

impl Reassembler {
    /// Called for every PacketEvent from the eBPF ring buffer
    pub fn ingest(&mut self, event: &PacketEvent) {
        let key = StreamKey { /* from event fields */ };
        let buf = self.streams.entry(key).or_insert(StreamBuffer {
            data: Vec::with_capacity(4096),
            last_seen_ms: now_ms(),
        });

        buf.data.extend_from_slice(&event.payload[..event.payload_len as usize]);
        buf.last_seen_ms = now_ms();

        // Try to extract complete HTTP messages from the buffer
        // HTTP/1.1: headers end at \r\n\r\n, then Content-Length bytes of body
        while let Some(msg) = try_extract_message(&mut buf.data) {
            let _ = self.tx.try_send(msg); // non-blocking, drop if consumer is slow
        }
    }

    /// Called periodically to expire stale half-open streams
    pub fn gc(&mut self) {
        let cutoff = now_ms() - self.timeout_ms;
        self.streams.retain(|_, buf| buf.last_seen_ms > cutoff);
    }
}

fn try_extract_message(data: &mut Vec<u8>) -> Option<RawHttpMessage> {
    // Find header boundary
    let boundary = data.windows(4).position(|w| w == b"\r\n\r\n")?;
    let header_end = boundary + 4;

    // Parse Content-Length if present to know body size
    let content_length = extract_content_length(&data[..header_end]).unwrap_or(0);
    let total_len = header_end + content_length;

    if data.len() < total_len {
        return None; // haven't received the full body yet
    }

    let raw = data[..total_len].to_vec();
    data.drain(..total_len); // consume bytes from buffer
    Some(RawHttpMessage { bytes: raw })
}
```

**Key design decisions:**
- `HashMap<StreamKey, StreamBuffer>` — O(1) lookup per packet, minimal allocation
- `try_send` not `send` — if the downstream parser is slow, we drop samples rather than block the capture loop (backpressure via dropping, not blocking)
- GC runs every 1s to free buffers from connections that died without closing cleanly (half-open, RST without FIN)

**Packages used:**
- `tokio::sync::mpsc` — async channel to the HTTP parser stage
- `aya` (userspace half) — reads from the eBPF ring buffer

**Built from scratch:** The reassembly logic itself. No existing crate handles "reassemble eBPF-captured TCP segments into HTTP messages" because this use case is novel.

---

## 4. HTTP Parser

**File:** `driftmap-core/src/http.rs`
**Estimated size:** ~180 lines

**What it does:** Takes a `Vec<u8>` of a complete HTTP message (request or response) and produces a structured `HttpMessage`. Must be zero-allocation on the hot path — we parse millions of these.

**Why not use `httparse` crate:** We could, and we do for the header parsing part. But `httparse` only handles headers — we need to also handle body parsing, content-encoding detection, and our own `HttpMessage` type. So we wrap `httparse` for the header pass and handle the rest ourselves.

```rust
// driftmap-core/src/http.rs

use httparse;  // the ONE external package we use here

#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub method:  String,
    pub path:    String,           // raw path e.g. "/users/123?page=2"
    pub path_template: String,     // normalized e.g. "/users/:id"
    pub headers: Vec<(String, String)>,
    pub body:    Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status:  u16,
    pub headers: Vec<(String, String)>,
    pub body:    Vec<u8>,
    pub latency_us: u64,  // set by matcher, not parser
}

pub enum HttpMessage {
    Request(HttpRequest),
    Response(HttpResponse),
}

pub fn parse_http_message(raw: &[u8]) -> Option<HttpMessage> {
    // Detect request vs response by first bytes
    if raw.starts_with(b"HTTP/") {
        parse_response(raw).map(HttpMessage::Response)
    } else {
        parse_request(raw).map(HttpMessage::Request)
    }
}

fn parse_request(raw: &[u8]) -> Option<HttpRequest> {
    let mut headers = [httparse::EMPTY_HEADER; 64];
    let mut req = httparse::Request::new(&mut headers);

    match req.parse(raw) {
        Ok(httparse::Status::Complete(header_len)) => {
            let method = req.method?.to_string();
            let path   = req.path?.to_string();
            let path_template = templatize_path(&path);

            let hdrs: Vec<(String,String)> = headers.iter()
                .take_while(|h| !h.name.is_empty())
                .map(|h| (
                    h.name.to_lowercase(),
                    String::from_utf8_lossy(h.value).to_string()
                ))
                .collect();

            let content_length: usize = hdrs.iter()
                .find(|(k,_)| k == "content-length")
                .and_then(|(_,v)| v.parse().ok())
                .unwrap_or(0);

            let body = raw[header_len..header_len + content_length].to_vec();

            Some(HttpRequest { method, path, path_template, headers: hdrs, body })
        }
        _ => None,
    }
}

/// Convert "/users/123/posts/456" → "/users/:id/posts/:id"
/// This is how we group requests to the same endpoint regardless of IDs
fn templatize_path(path: &str) -> String {
    // Split at ? first to ignore query string
    let path_only = path.split('?').next().unwrap_or(path);

    path_only.split('/')
        .map(|segment| {
            // If segment is purely numeric, or looks like a UUID, or is a hex string → replace
            if segment.chars().all(|c| c.is_ascii_digit())
               || is_uuid(segment)
               || (segment.len() > 8 && segment.chars().all(|c| c.is_ascii_hexdigit()))
            {
                ":id"
            } else {
                segment
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn is_uuid(s: &str) -> bool {
    // xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx
    s.len() == 36
        && s.chars().enumerate().all(|(i, c)| {
            if [8,13,18,23].contains(&i) { c == '-' }
            else { c.is_ascii_hexdigit() }
        })
}
```

**Packages used:**
- `httparse` — safe, zero-copy HTTP/1.x header parser (4KB crate, no dependencies)

**Built from scratch:**
- Path templatization (`:id` extraction)
- Request/Response type system
- The detection logic (request vs response) 
- Body extraction after `httparse` gives us header length

---

## 5. Request Matcher

**File:** `driftmap-core/src/matcher.rs`
**Estimated size:** ~160 lines

**The hardest problem in DriftMap.** We receive a stream of HTTP messages from Target A and a stream from Target B. We need to pair them up: "this request to A corresponds to this request to B" so we can compare the responses.

**The matching key:** `(method, path_template, time_window_bucket)`

Two requests match if:
1. Same HTTP method
2. Same templatized path (so `/users/123` and `/users/456` both match `/users/:id`)
3. Arrived within the same configurable time window (default 500ms)

**The data structure:**

```rust
// driftmap-core/src/matcher.rs

use std::collections::{HashMap, VecDeque};
use tokio::time::{Duration, Instant};
use crate::http::{HttpRequest, HttpResponse};

/// A request waiting for its counterpart from the other target
struct PendingRequest {
    request:    HttpRequest,
    arrived_at: Instant,
}

/// Fully matched pair ready for diff
pub struct MatchedPair {
    pub endpoint:    String,           // "GET /users/:id"
    pub req_a:       HttpRequest,
    pub res_a:       HttpResponse,
    pub req_b:       HttpRequest,
    pub res_b:       HttpResponse,
}

pub struct Matcher {
    /// Requests from A waiting for a matching request from B
    pending_a: HashMap<String, VecDeque<PendingRequest>>,
    /// Requests from B waiting for a matching request from A
    pending_b: HashMap<String, VecDeque<PendingRequest>>,

    window: Duration,         // default: 500ms
    tx:     mpsc::Sender<MatchedPair>,
}

impl Matcher {
    /// Call this when a complete request+response pair arrives from one target
    pub fn ingest(&mut self, from: Target, req: HttpRequest, res: HttpResponse) {
        let key = format!("{} {}", req.method, req.path_template);

        let (my_pending, their_pending) = match from {
            Target::A => (&mut self.pending_a, &mut self.pending_b),
            Target::B => (&mut self.pending_b, &mut self.pending_a),
        };

        // Is there a pending request from the other side for the same endpoint?
        if let Some(queue) = their_pending.get_mut(&key) {
            // Find the oldest pending request within the time window
            while let Some(front) = queue.front() {
                if front.arrived_at.elapsed() > self.window {
                    queue.pop_front(); // expired, discard
                    continue;
                }
                // Found a match!
                let their = queue.pop_front().unwrap();
                let pair = match from {
                    Target::A => MatchedPair {
                        endpoint: key.clone(),
                        req_a: req.clone(), res_a: res.clone(),
                        req_b: their.request, res_b: /* we need their response too */
                    },
                    Target::B => /* symmetric */
                };
                let _ = self.tx.try_send(pair);
                return;
            }
        }

        // No match found yet — store as pending
        my_pending
            .entry(key)
            .or_insert_with(VecDeque::new)
            .push_back(PendingRequest { request: req, arrived_at: Instant::now() });
    }

    /// Expire stale pending requests (called every 100ms)
    pub fn gc(&mut self) {
        let cutoff = self.window;
        for queue in self.pending_a.values_mut().chain(self.pending_b.values_mut()) {
            while queue.front().map(|p| p.arrived_at.elapsed() > cutoff).unwrap_or(false) {
                queue.pop_front();
            }
        }
    }
}
```

**Note:** We need request+response pairs, not just requests. This means the capture layer must correlate TCP connections: "this response on port 3000 with sequence 500 is the response to the request with sequence 100 from the same TCP connection." The `StreamKey` includes direction — that's how we know which bytes are the request and which are the response.

**Packages used:** Just `std` — `HashMap`, `VecDeque`, `Instant`. Zero external dependencies in this file.

**Built from scratch:** The entire pairing algorithm. Nothing like this exists as a library.

---

## 6. Raw Diff Engine

**File:** `driftmap-core/src/diff.rs`
**Estimated size:** ~140 lines

**What it does:** Takes a `MatchedPair` and produces a `RawDiff` — a structured description of every byte-level difference between the two responses. This is the foundation that the semantic scorer then reasons on top of.

```rust
// driftmap-core/src/diff.rs

use crate::matcher::MatchedPair;

#[derive(Debug)]
pub struct RawDiff {
    pub endpoint:        String,
    pub status_match:    bool,
    pub status_a:        u16,
    pub status_b:        u16,
    pub headers_only_a:  Vec<String>,   // header names in A but not B
    pub headers_only_b:  Vec<String>,   // header names in B but not A
    pub headers_value_diff: Vec<(String, String, String)>, // (name, val_a, val_b)
    pub body_identical:  bool,
    pub body_a_len:      usize,
    pub body_b_len:      usize,
    pub latency_delta_us: i64,          // positive = A was slower
}

pub fn compute_raw_diff(pair: &MatchedPair) -> RawDiff {
    let status_match = pair.res_a.status == pair.res_b.status;

    // Header diff — convert both to HashMaps, then set operations
    let hdrs_a: HashMap<&str, &str> = pair.res_a.headers.iter()
        .map(|(k,v)| (k.as_str(), v.as_str()))
        .collect();
    let hdrs_b: HashMap<&str, &str> = pair.res_b.headers.iter()
        .map(|(k,v)| (k.as_str(), v.as_str()))
        .collect();

    // Skip headers that are inherently non-deterministic
    let skip_headers = ["date", "x-request-id", "x-trace-id", "server-timing"];

    let headers_only_a: Vec<String> = hdrs_a.keys()
        .filter(|k| !hdrs_b.contains_key(*k) && !skip_headers.contains(k))
        .map(|k| k.to_string())
        .collect();

    let headers_only_b: Vec<String> = hdrs_b.keys()
        .filter(|k| !hdrs_a.contains_key(*k) && !skip_headers.contains(k))
        .map(|k| k.to_string())
        .collect();

    let headers_value_diff: Vec<(String,String,String)> = hdrs_a.iter()
        .filter(|(k,v)| {
            hdrs_b.get(*k).map(|bv| bv != *v).unwrap_or(false)
            && !skip_headers.contains(k)
        })
        .map(|(k,v)| (k.to_string(), v.to_string(), hdrs_b[k].to_string()))
        .collect();

    // Body diff — just byte comparison for now (semantic layer goes deeper)
    let body_identical = pair.res_a.body == pair.res_b.body;

    RawDiff {
        endpoint:        pair.endpoint.clone(),
        status_match,
        status_a:        pair.res_a.status,
        status_b:        pair.res_b.status,
        headers_only_a,
        headers_only_b,
        headers_value_diff,
        body_identical,
        body_a_len:      pair.res_a.body.len(),
        body_b_len:      pair.res_b.body.len(),
        latency_delta_us: pair.res_a.latency_us as i64 - pair.res_b.latency_us as i64,
    }
}
```

**Packages used:** Just `std::collections::HashMap`. Zero external dependencies.

**Built from scratch:** The diff logic is entirely custom. There's no "HTTP response differ" library because the domain knowledge of what to skip (request IDs, dates, tracing headers) is inherently application-specific.

---

## 7. Schema Inference Engine

**File:** `driftmap-core/src/schema.rs`
**Estimated size:** ~250 lines

**What it does:** Observes JSON response bodies over time and infers a schema for each endpoint on each target. When the two inferred schemas diverge, that's a structural drift signal.

**How schema inference works:**

```
See 200 responses to GET /users/:id:
Response 1: {"id":1, "name":"Alice", "email":"a@x.com"}
Response 2: {"id":2, "name":"Bob",   "email":"b@x.com", "admin": true}
Response 3: {"id":3, "name":"Carol", "email":"c@x.com"}

Inferred schema:
  id:    Integer (required — always present)
  name:  String  (required)
  email: String  (required)
  admin: Boolean (optional — present in 1/3 = 33% of responses)
```

```rust
// driftmap-core/src/schema.rs

use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum FieldType {
    String, Integer, Float, Boolean, Object(SchemaNode), Array(Box<FieldType>), Null
}

#[derive(Debug, Clone)]
pub struct FieldStats {
    pub field_type:   FieldType,
    pub seen_count:   u32,   // how many responses had this field
    pub total_count:  u32,   // how many responses total
    pub nullable:     bool,  // was this field ever null?
}

impl FieldStats {
    pub fn presence_rate(&self) -> f32 {
        self.seen_count as f32 / self.total_count as f32
    }
    pub fn is_required(&self) -> bool {
        self.presence_rate() > 0.95  // present in >95% of responses
    }
}

pub type SchemaNode = HashMap<String, FieldStats>;

pub struct SchemaInferrer {
    /// Per-endpoint, per-target schema
    schemas: HashMap<(String, Target), SchemaNode>,
    sample_count: HashMap<(String, Target), u32>,
    min_samples: u32,  // don't emit schema diffs until we've seen this many (default: 50)
}

impl SchemaInferrer {
    pub fn observe(&mut self, endpoint: &str, target: Target, body: &[u8]) {
        let Ok(value) = serde_json::from_slice::<Value>(body) else { return };
        let Value::Object(obj) = value else { return }; // only handle JSON objects for now

        let key = (endpoint.to_string(), target);
        *self.sample_count.entry(key.clone()).or_insert(0) += 1;
        let total = self.sample_count[&key];

        let schema = self.schemas.entry(key).or_insert_with(HashMap::new);

        // Update existing fields with new sample
        for (field, val) in &obj {
            let stats = schema.entry(field.clone()).or_insert(FieldStats {
                field_type:  infer_type(val),
                seen_count:  0,
                total_count: total,
                nullable:    false,
            });
            stats.seen_count += 1;
            stats.total_count = total;
            if val.is_null() { stats.nullable = true; }
        }

        // Fields NOT in this response still get total_count bumped
        for stats in schema.values_mut() {
            stats.total_count = total;
        }
    }

    pub fn diff(&self, endpoint: &str) -> Option<SchemaDiff> {
        let schema_a = self.schemas.get(&(endpoint.to_string(), Target::A))?;
        let schema_b = self.schemas.get(&(endpoint.to_string(), Target::B))?;

        // Only emit diffs after warm-up
        let count_a = self.sample_count.get(&(endpoint.to_string(), Target::A)).copied()?;
        let count_b = self.sample_count.get(&(endpoint.to_string(), Target::B)).copied()?;
        if count_a < self.min_samples || count_b < self.min_samples { return None; }

        let mut fields_only_a = vec![];
        let mut fields_only_b = vec![];
        let mut type_mismatches = vec![];

        for (field, stats_a) in schema_a {
            match schema_b.get(field) {
                None => {
                    if stats_a.is_required() {
                        fields_only_a.push(field.clone());
                    }
                }
                Some(stats_b) => {
                    if stats_a.field_type != stats_b.field_type {
                        type_mismatches.push((
                            field.clone(),
                            stats_a.field_type.clone(),
                            stats_b.field_type.clone(),
                        ));
                    }
                }
            }
        }

        for (field, stats_b) in schema_b {
            if !schema_a.contains_key(field) && stats_b.is_required() {
                fields_only_b.push(field.clone());
            }
        }

        if fields_only_a.is_empty() && fields_only_b.is_empty() && type_mismatches.is_empty() {
            return None; // schemas are equivalent
        }

        Some(SchemaDiff { endpoint: endpoint.to_string(), fields_only_a, fields_only_b, type_mismatches })
    }
}

fn infer_type(v: &Value) -> FieldType {
    match v {
        Value::String(_)  => FieldType::String,
        Value::Number(n)  => if n.is_f64() { FieldType::Float } else { FieldType::Integer },
        Value::Bool(_)    => FieldType::Boolean,
        Value::Null       => FieldType::Null,
        Value::Object(m)  => FieldType::Object(
            m.iter().map(|(k,v)| (k.clone(), FieldStats {
                field_type: infer_type(v), seen_count: 1,
                total_count: 1, nullable: v.is_null(),
            })).collect()
        ),
        Value::Array(a)   => FieldType::Array(Box::new(
            a.first().map(infer_type).unwrap_or(FieldType::Null)
        )),
    }
}
```

**Packages used:**
- `serde_json` — deserialize response bodies to `Value` for field inspection

**Built from scratch:** The entire schema inference and diffing logic. No crate does "infer JSON schema from a stream of samples and diff two inferred schemas" — this is DriftMap's secret sauce.

---

## 8. Distribution Tracker

**File:** `driftmap-core/src/distribution.rs`
**Estimated size:** ~170 lines

**What it does:** For numeric fields in JSON responses (or latency values), tracks a rolling distribution on each target and detects when the two distributions statistically diverge.

**The data structure — TDigest:**

We use a t-digest algorithm. It's a streaming quantile estimator that:
- Uses O(100) memory regardless of how many samples you've seen (not O(N))
- Gives accurate p95/p99 even from streaming data
- Can be merged (which lets us combine sidecar agents)

We implement TDigest ourselves (it's ~120 lines) rather than using a crate because the existing Rust t-digest crates don't support merging or serialization to SQLite.

```rust
// driftmap-core/src/distribution.rs

/// A centroid in the t-digest
#[derive(Clone)]
struct Centroid {
    mean:  f64,
    count: u32,
}

/// Streaming approximate quantile estimator
pub struct TDigest {
    centroids: Vec<Centroid>,
    count:     u64,
    max_size:  usize,  // compress when we exceed this (default: 200)
}

impl TDigest {
    pub fn new() -> Self {
        Self { centroids: Vec::new(), count: 0, max_size: 200 }
    }

    pub fn add(&mut self, value: f64) {
        self.count += 1;
        // Insert into sorted centroid list, merge if close
        let pos = self.centroids.partition_point(|c| c.mean < value);
        self.centroids.insert(pos, Centroid { mean: value, count: 1 });
        if self.centroids.len() > self.max_size {
            self.compress();
        }
    }

    pub fn quantile(&self, q: f64) -> f64 {
        // Walk centroids until we've accumulated q fraction of total count
        let target = q * self.count as f64;
        let mut seen = 0.0_f64;
        for c in &self.centroids {
            seen += c.count as f64;
            if seen >= target { return c.mean; }
        }
        self.centroids.last().map(|c| c.mean).unwrap_or(0.0)
    }

    fn compress(&mut self) {
        // Merge adjacent centroids using the t-digest scaling function
        // This keeps the digest size bounded while preserving accuracy at tails
        let mut merged: Vec<Centroid> = Vec::with_capacity(self.max_size);
        let total = self.count as f64;

        for c in self.centroids.drain(..) {
            if let Some(last) = merged.last_mut() {
                let combined_count = last.count + c.count;
                // t-digest merge condition: merged centroid stays within size budget
                let limit = 4.0 * total * (merged.len() as f64 / self.max_size as f64)
                    * (1.0 - merged.len() as f64 / self.max_size as f64);
                if combined_count as f64 <= limit {
                    last.mean = (last.mean * last.count as f64 + c.mean * c.count as f64)
                        / combined_count as f64;
                    last.count = combined_count;
                    continue;
                }
            }
            merged.push(c);
        }
        self.centroids = merged;
    }
}

/// Tracks distributions for one field on both targets, detects divergence
pub struct FieldDistribution {
    digest_a: TDigest,
    digest_b: TDigest,
}

impl FieldDistribution {
    pub fn observe(&mut self, target: Target, value: f64) {
        match target {
            Target::A => self.digest_a.add(value),
            Target::B => self.digest_b.add(value),
        }
    }

    /// Returns a divergence score 0.0–1.0 based on how different p95s are
    pub fn divergence_score(&self) -> f32 {
        let p95_a = self.digest_a.quantile(0.95);
        let p95_b = self.digest_b.quantile(0.95);
        let p50_a = self.digest_a.quantile(0.50);
        let p50_b = self.digest_b.quantile(0.50);

        // Normalized absolute difference, capped at 1.0
        let p95_diff = ((p95_a - p95_b).abs() / (p95_a.max(p95_b) + 1.0)) as f32;
        let p50_diff = ((p50_a - p50_b).abs() / (p50_a.max(p50_b) + 1.0)) as f32;

        // Weight p95 more heavily — latency spikes matter more than median shifts
        (p95_diff * 0.7 + p50_diff * 0.3).min(1.0)
    }
}
```

**Packages used:** None. The t-digest is ~120 lines of pure math, no external crate needed.

**Built from scratch:** The entire t-digest implementation and the divergence scoring on top of it.

---

## 9. Semantic Scorer

**File:** `driftmap-core/src/scorer.rs`
**Estimated size:** ~130 lines

**What it does:** Combines `RawDiff` + `SchemaDiff` + `FieldDistribution` divergence scores into a single `DriftScore` per endpoint. This is what gets displayed in the TUI.

**Scoring formula:**

```
DriftScore = (
    status_score   * 0.40 +   // status code mismatch is a big deal
    schema_score   * 0.30 +   // field-level structural change
    latency_score  * 0.20 +   // latency divergence
    header_score   * 0.10     // header differences (lowest weight)
).clamp(0.0, 1.0)
```

```rust
// driftmap-core/src/scorer.rs

use crate::{diff::RawDiff, schema::SchemaDiff, distribution::FieldDistribution};

#[derive(Debug, Clone)]
pub struct DriftScore {
    pub endpoint:      String,
    pub score:         f32,           // 0.0 = identical, 1.0 = completely diverged
    pub status_score:  f32,
    pub schema_score:  f32,
    pub latency_score: f32,
    pub header_score:  f32,
    pub sample_count:  u64,
}

pub struct Scorer {
    /// Rolling window of raw diffs per endpoint (last 1000)
    recent_diffs: HashMap<String, VecDeque<RawDiff>>,
    schema_diffs:  HashMap<String, Option<SchemaDiff>>,
    latency_dists: HashMap<String, FieldDistribution>,
    window_size:   usize,  // default: 1000
}

impl Scorer {
    pub fn ingest_diff(&mut self, diff: RawDiff) {
        // Track latency
        self.latency_dists
            .entry(diff.endpoint.clone())
            .or_default()
            .observe_pair(diff.latency_delta_us);

        // Store raw diff in rolling window
        let diffs = self.recent_diffs
            .entry(diff.endpoint.clone())
            .or_default();
        diffs.push_back(diff);
        if diffs.len() > self.window_size {
            diffs.pop_front();
        }
    }

    pub fn compute_score(&self, endpoint: &str) -> Option<DriftScore> {
        let diffs = self.recent_diffs.get(endpoint)?;
        if diffs.is_empty() { return None; }

        // Status score: fraction of recent pairs where status codes differed
        let status_score = diffs.iter()
            .filter(|d| !d.status_match)
            .count() as f32 / diffs.len() as f32;

        // Schema score: 0 if no schema diff, 1 if fields are missing
        let schema_score = match self.schema_diffs.get(endpoint) {
            Some(Some(sd)) => {
                let total_fields = sd.fields_only_a.len()
                    + sd.fields_only_b.len()
                    + sd.type_mismatches.len();
                (total_fields as f32 / 10.0).min(1.0) // normalize: 10+ field diffs = score 1.0
            }
            _ => 0.0,
        };

        // Latency score from distribution tracker
        let latency_score = self.latency_dists
            .get(endpoint)
            .map(|d| d.divergence_score())
            .unwrap_or(0.0);

        // Header score: avg fraction of headers that differ per response
        let header_score = diffs.iter()
            .map(|d| {
                let total = d.headers_only_a.len() + d.headers_only_b.len()
                    + d.headers_value_diff.len();
                (total as f32 / 10.0).min(1.0)
            })
            .sum::<f32>() / diffs.len() as f32;

        let score = (status_score  * 0.40
                   + schema_score  * 0.30
                   + latency_score * 0.20
                   + header_score  * 0.10).clamp(0.0, 1.0);

        Some(DriftScore {
            endpoint: endpoint.to_string(),
            score, status_score, schema_score, latency_score, header_score,
            sample_count: diffs.len() as u64,
        })
    }
}
```

**Packages used:** Just `std`. All the math is hand-rolled.

---

## 10. Drift State Machine

**File:** `driftmap-core/src/state.rs`
**Estimated size:** ~100 lines

**What it does:** Each endpoint lives in one of four states. The state machine adds hysteresis — scores must stay above/below thresholds for a sustained period before state transitions. This prevents flapping (where an endpoint rapidly bounces between EQUIVALENT and DRIFTING on normal traffic variance).

```
States:
  UNKNOWN     — not enough samples yet (< min_samples)
  EQUIVALENT  — score < 0.05 sustained for 30s
  DRIFTING    — score 0.05–0.50 sustained for 30s  ← yellow alert
  DIVERGED    — score > 0.50 sustained for 30s      ← red alert

Transitions have 30s hysteresis:
  EQUIVALENT → DRIFTING: score must be ≥ 0.05 for 30 consecutive seconds
  DRIFTING → EQUIVALENT: score must be < 0.05 for 30 consecutive seconds
  DRIFTING → DIVERGED:   score must be ≥ 0.50 for 30 consecutive seconds
```

```rust
// driftmap-core/src/state.rs

use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq)]
pub enum DriftState {
    Unknown,
    Equivalent,
    Drifting,
    Diverged,
}

struct StateRecord {
    state:         DriftState,
    entered_at:    Instant,
    /// Tracks when we first crossed the threshold for next state
    threshold_crossed_at: Option<Instant>,
}

pub struct StateMachine {
    records:       HashMap<String, StateRecord>,
    hysteresis:    Duration,       // default: 30s
    drift_threshold:   f32,       // default: 0.05
    diverged_threshold: f32,      // default: 0.50
    min_samples:   u64,           // default: 50
}

impl StateMachine {
    pub fn update(&mut self, endpoint: &str, score: &DriftScore) -> Option<StateTransition> {
        if score.sample_count < self.min_samples {
            self.records.entry(endpoint.to_string())
                .or_insert(StateRecord { state: DriftState::Unknown, /* ... */ });
            return None;
        }

        let record = self.records.entry(endpoint.to_string())
            .or_insert(StateRecord { state: DriftState::Equivalent, /* ... */ });

        let target_state = if score.score >= self.diverged_threshold {
            DriftState::Diverged
        } else if score.score >= self.drift_threshold {
            DriftState::Drifting
        } else {
            DriftState::Equivalent
        };

        if target_state == record.state {
            record.threshold_crossed_at = None;
            return None;
        }

        // Has threshold been crossed long enough?
        match record.threshold_crossed_at {
            None => {
                record.threshold_crossed_at = Some(Instant::now());
                None
            }
            Some(crossed_at) => {
                if crossed_at.elapsed() >= self.hysteresis {
                    let old = record.state.clone();
                    record.state = target_state.clone();
                    record.entered_at = Instant::now();
                    record.threshold_crossed_at = None;
                    Some(StateTransition { endpoint: endpoint.to_string(), from: old, to: target_state })
                } else {
                    None
                }
            }
        }
    }
}
```

**Packages used:** Just `std::time`. Zero dependencies.

---

## 11. SQLite State Store

**File:** `driftmap-core/src/store.rs`
**Estimated size:** ~150 lines

**What it stores:**
1. Current `DriftState` per endpoint (survive restarts)
2. Last 1000 diverging response pairs per endpoint (for the `driftmap diff` CLI command)
3. Historical drift scores (time-series, for sparklines in TUI)

```rust
// driftmap-core/src/store.rs

use rusqlite::{Connection, params};

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &str) -> anyhow::Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch("
            PRAGMA journal_mode=WAL;           -- write-ahead log for concurrent reads
            PRAGMA synchronous=NORMAL;         -- faster writes, still safe

            CREATE TABLE IF NOT EXISTS endpoint_state (
                endpoint    TEXT PRIMARY KEY,
                state       TEXT NOT NULL,
                updated_at  INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS drift_scores (
                endpoint    TEXT NOT NULL,
                score       REAL NOT NULL,
                recorded_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_drift_scores_endpoint
                ON drift_scores(endpoint, recorded_at DESC);

            CREATE TABLE IF NOT EXISTS diverging_pairs (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                endpoint    TEXT NOT NULL,
                req_method  TEXT,
                req_path    TEXT,
                status_a    INTEGER,
                status_b    INTEGER,
                body_a      BLOB,
                body_b      BLOB,
                recorded_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_pairs_endpoint
                ON diverging_pairs(endpoint, recorded_at DESC);
        ")?;
        Ok(Self { conn })
    }

    pub fn save_state(&self, endpoint: &str, state: &DriftState) -> anyhow::Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO endpoint_state (endpoint, state, updated_at)
             VALUES (?1, ?2, strftime('%s','now'))",
            params![endpoint, format!("{:?}", state)],
        )?;
        Ok(())
    }

    pub fn save_score(&self, endpoint: &str, score: f32) -> anyhow::Result<()> {
        self.conn.execute(
            "INSERT INTO drift_scores (endpoint, score, recorded_at)
             VALUES (?1, ?2, strftime('%s','now'))",
            params![endpoint, score],
        )?;
        // Prune old scores (keep last 24h)
        self.conn.execute(
            "DELETE FROM drift_scores
             WHERE endpoint = ?1
               AND recorded_at < strftime('%s','now') - 86400",
            params![endpoint],
        )?;
        Ok(())
    }

    pub fn recent_scores(&self, endpoint: &str, limit: usize) -> anyhow::Result<Vec<(i64, f32)>> {
        let mut stmt = self.conn.prepare(
            "SELECT recorded_at, score FROM drift_scores
             WHERE endpoint = ?1
             ORDER BY recorded_at DESC LIMIT ?2"
        )?;
        let rows = stmt.query_map(params![endpoint, limit as i64], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, f32>(1)?))
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}
```

**Packages used:**
- `rusqlite` — SQLite bindings for Rust, bundled SQLite (`features = ["bundled"]`)

**Why SQLite and not RocksDB or a time-series DB:**
- Zero operational overhead — no daemon, no port, one file
- WAL mode gives concurrent reads without blocking writes
- SQLite handles 100k inserts/sec easily — more than enough for our write rate
- The data is not analytically huge — 1 score/5s per endpoint × 100 endpoints × 24h = 1.7M rows max

---

## 12. TUI Dashboard

**Files:** `driftmap-tui/src/{app.rs, ui.rs, events.rs}`
**Estimated total size:** ~350 lines

**How the TUI event loop works:**

```
Main thread:
  ┌─────────────────────────────────┐
  │ tokio::select! {                │
  │   keyboard event → update state │
  │   tick (100ms)  → redraw        │
  │   score update  → update state  │
  │ }                               │
  └─────────────────────────────────┘
```

```rust
// driftmap-tui/src/app.rs

use ratatui::Terminal;
use crossterm::event::{self, Event, KeyCode};
use tokio::sync::watch;
use crate::ui::draw;
use driftmap_core::scorer::DriftScore;

pub struct App {
    pub scores:        Vec<DriftScore>,       // sorted by score desc
    pub selected:      usize,                 // index in scores list
    pub sort_by:       SortMode,
    pub filter:        Option<String>,        // user-typed filter string
    pub score_rx:      watch::Receiver<Vec<DriftScore>>,
}

#[derive(Clone)]
pub enum SortMode { ByScore, ByName, ByRequests }

pub async fn run(mut app: App, mut terminal: Terminal<impl Backend>) -> anyhow::Result<()> {
    let tick = tokio::time::interval(Duration::from_millis(100));
    tokio::pin!(tick);

    loop {
        // Redraw every tick
        tick.tick().await;
        terminal.draw(|f| draw(f, &app))?;

        // Check for score updates from core pipeline
        if app.score_rx.has_changed()? {
            app.scores = app.score_rx.borrow_and_update().clone();
            app.scores.sort_by(|a,b| match app.sort_by {
                SortMode::ByScore    => b.score.partial_cmp(&a.score).unwrap(),
                SortMode::ByName     => a.endpoint.cmp(&b.endpoint),
                SortMode::ByRequests => b.sample_count.cmp(&a.sample_count),
            });
        }

        // Non-blocking keyboard check
        if event::poll(Duration::from_millis(0))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q')        => break,
                    KeyCode::Down | KeyCode::Char('j') =>
                        app.selected = (app.selected + 1).min(app.scores.len().saturating_sub(1)),
                    KeyCode::Up | KeyCode::Char('k')   =>
                        app.selected = app.selected.saturating_sub(1),
                    KeyCode::Char('s') => app.sort_by = SortMode::ByScore,
                    KeyCode::Char('n') => app.sort_by = SortMode::ByName,
                    KeyCode::Char('r') => app.sort_by = SortMode::ByRequests,
                    _ => {}
                }
            }
        }
    }
    Ok(())
}
```

```rust
// driftmap-tui/src/ui.rs

use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph, Sparkline},
    Frame,
};

pub fn draw(f: &mut Frame, app: &App) {
    // Split screen: left list (40%) | right detail (60%)
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(f.size());

    draw_endpoint_list(f, app, chunks[0]);
    draw_endpoint_detail(f, app, chunks[1]);
    draw_status_bar(f, app, /* bottom strip */);
}

fn draw_endpoint_list(f: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app.scores.iter().enumerate().map(|(i, score)| {
        let color = score_to_color(score.score);
        let symbol = score_to_symbol(score.score); // ✓ ⚠ ✗
        let text = format!(
            "{} {:6.1}%  {}",
            symbol,
            score.score * 100.0,
            truncate(&score.endpoint, 30)
        );
        let style = if i == app.selected {
            Style::default().bg(Color::DarkGray).fg(color).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(color)
        };
        ListItem::new(text).style(style)
    }).collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" Endpoints "));
    f.render_widget(list, area);
}

fn draw_endpoint_detail(f: &mut Frame, app: &App, area: Rect) {
    let Some(score) = app.scores.get(app.selected) else { return };

    // Split detail panel: top sparkline, middle scores, bottom response diff
    let inner = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),  // sparkline
            Constraint::Length(8),  // score breakdown
            Constraint::Min(0),     // response diff
        ])
        .split(area);

    // Sparkline: last 60 score values
    let sparkline_data: Vec<u64> = app.historical_scores(&score.endpoint)
        .iter().map(|s| (*s * 100.0) as u64).collect();
    let sparkline = Sparkline::default()
        .block(Block::default().borders(Borders::ALL).title(" Drift Score (last 5min) "))
        .data(&sparkline_data)
        .style(Style::default().fg(score_to_color(score.score)));
    f.render_widget(sparkline, inner[0]);

    // Score breakdown as gauges
    let breakdown = Paragraph::new(format!(
        "Status:   {:.0}%   Schema:  {:.0}%\nLatency:  {:.0}%   Headers: {:.0}%\nSamples: {}",
        score.status_score * 100.0,
        score.schema_score * 100.0,
        score.latency_score * 100.0,
        score.header_score * 100.0,
        score.sample_count,
    )).block(Block::default().borders(Borders::ALL).title(" Score Breakdown "));
    f.render_widget(breakdown, inner[1]);
}

fn score_to_color(score: f32) -> Color {
    if score < 0.05      { Color::Green }
    else if score < 0.50 { Color::Yellow }
    else                 { Color::Red }
}
```

**Packages used:**
- `ratatui` — terminal UI framework (actively maintained tui-rs fork)
- `crossterm` — cross-platform terminal input/output (keyboard events, raw mode)

**Built from scratch:** All layout logic, color coding, sparkline data prep, the app state struct.

---

## 13. WASM Plugin System

**Files:** `driftmap-plugin-sdk/src/lib.rs` + `driftmap-core/src/plugins.rs`
**Estimated total size:** ~180 lines

**How it works:**
1. Plugin author writes Rust code using `driftmap-plugin-sdk`
2. Compiles it to `wasm32-unknown-unknown` target
3. DriftMap loads the `.wasm` file at startup using `wasmtime`
4. For each matched pair on configured endpoints, DriftMap passes request+response data to the plugin via linear memory
5. Plugin returns a score (f32) and optional annotation string
6. Score is merged with DriftMap's own score (max of the two)

```rust
// driftmap-plugin-sdk/src/lib.rs  (published to crates.io)
// This is what plugin AUTHORS use

#![no_std]

/// Plugin authors implement this trait, then export `score_pair` as a C function
pub trait DriftPlugin {
    fn score_pair(req_a: &Request, res_a: &Response,
                  req_b: &Request, res_b: &Response) -> PluginScore;
}

#[repr(C)]
pub struct Request {
    pub method:     *const u8,
    pub method_len: usize,
    pub path:       *const u8,
    pub path_len:   usize,
    pub body:       *const u8,
    pub body_len:   usize,
}

#[repr(C)]
pub struct Response {
    pub status:   u16,
    pub body:     *const u8,
    pub body_len: usize,
}

#[repr(C)]
pub struct PluginScore {
    pub score:          f32,     // 0.0–1.0
    pub annotation:     *const u8,
    pub annotation_len: usize,
}

/// Macro to reduce boilerplate for plugin authors
#[macro_export]
macro_rules! export_plugin {
    ($ty:ty) => {
        #[no_mangle]
        pub extern "C" fn score_pair(/* args */) -> f32 {
            <$ty as DriftPlugin>::score_pair(/* ... */)
        }
    }
}
```

```rust
// driftmap-core/src/plugins.rs  (DriftMap internal — loads and calls plugins)

use wasmtime::{Engine, Module, Store, Linker, Instance};

pub struct PluginHost {
    engine:   Engine,
    plugins:  Vec<LoadedPlugin>,
}

struct LoadedPlugin {
    instance:   Instance,
    store:      Store<()>,
    applies_to: Vec<String>,  // endpoint patterns this plugin handles
}

impl PluginHost {
    pub fn load(path: &str, applies_to: Vec<String>) -> anyhow::Result<LoadedPlugin> {
        let engine = Engine::default();
        let module = Module::from_file(&engine, path)?;
        let linker = Linker::new(&engine);
        let mut store = Store::new(&engine, ());
        let instance = linker.instantiate(&mut store, &module)?;
        Ok(LoadedPlugin { instance, store, applies_to })
    }

    pub fn run_plugins(&mut self, pair: &MatchedPair) -> Option<f32> {
        // Find plugins that apply to this endpoint
        let scores: Vec<f32> = self.plugins.iter_mut()
            .filter(|p| p.applies_to.iter().any(|pat| pair.endpoint.contains(pat.as_str())))
            .filter_map(|plugin| {
                // Write request/response data into WASM linear memory
                let memory = plugin.instance
                    .get_memory(&mut plugin.store, "memory")?;

                // Allocate space and copy bytes
                // (plugin must export an `alloc` function)
                let alloc = plugin.instance
                    .get_typed_func::<u32, u32>(&mut plugin.store, "alloc")
                    .ok()?;

                let score_fn = plugin.instance
                    .get_typed_func::<(u32,u32,u32,u32,u32,u32), f32>(
                        &mut plugin.store, "score_pair"
                    ).ok()?;

                // Write body_a into wasm memory, get pointer back
                let body_a_ptr = alloc.call(&mut plugin.store, pair.res_a.body.len() as u32).ok()?;
                memory.write(&mut plugin.store, body_a_ptr as usize, &pair.res_a.body).ok()?;

                // Call plugin's score function
                score_fn.call(&mut plugin.store, (
                    body_a_ptr, pair.res_a.body.len() as u32,
                    // ... other args
                    0, 0, 0, 0
                )).ok()
            })
            .collect();

        // Return max score from all plugins
        scores.into_iter().reduce(f32::max)
    }
}
```

**Packages used:**
- `wasmtime` — fast WASM runtime, safe sandboxing, Bytecode Alliance

**Built from scratch:** The plugin ABI, the memory passing protocol, the `export_plugin!` macro.

---

## 14. Export Layer

**File:** `driftmap-core/src/export.rs` + additions to `driftmap-cli/src/main.rs`
**Estimated size:** ~120 lines

**Three export formats:**

**1. Prometheus metrics** (scraped by Prometheus server):
```rust
// driftmap-core/src/export.rs

use std::fmt::Write;

pub fn render_prometheus(scores: &[DriftScore]) -> String {
    let mut out = String::with_capacity(4096);

    writeln!(out, "# HELP driftmap_score Behavioral divergence score 0.0-1.0").unwrap();
    writeln!(out, "# TYPE driftmap_score gauge").unwrap();

    for score in scores {
        // Escape endpoint for Prometheus label (replace " " with "_", remove special chars)
        let endpoint_label = score.endpoint.replace(' ', "_").replace('/', "_");
        writeln!(out,
            "driftmap_score{{endpoint=\"{}\"}} {}",
            endpoint_label, score.score
        ).unwrap();
        writeln!(out,
            "driftmap_score_samples{{endpoint=\"{}\"}} {}",
            endpoint_label, score.sample_count
        ).unwrap();
    }
    out
}

// HTTP server for /metrics endpoint (50 lines with axum)
pub async fn serve_metrics(scores_rx: watch::Receiver<Vec<DriftScore>>) {
    use axum::{routing::get, Router};

    let app = Router::new().route("/metrics", get(move || {
        let scores = scores_rx.borrow().clone();
        async move { render_prometheus(&scores) }
    }));

    axum::Server::bind(&"0.0.0.0:9090".parse().unwrap())
        .serve(app.into_make_service())
        .await.unwrap();
}
```

**2. Newline-delimited JSON** (stdout, for Datadog/Loki log shippers):
```rust
pub fn emit_ndjson(score: &DriftScore) {
    // serde_json::to_string gives us one compact JSON line
    println!("{}", serde_json::to_string(score).unwrap());
}
```

**3. Webhook** (POST on state transitions):
```rust
pub async fn fire_webhook(url: &str, transition: &StateTransition) -> anyhow::Result<()> {
    let client = reqwest::Client::new();
    client.post(url)
        .json(&serde_json::json!({
            "endpoint":  transition.endpoint,
            "from":      format!("{:?}", transition.from),
            "to":        format!("{:?}", transition.to),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }))
        .timeout(Duration::from_secs(5))
        .send().await?;
    Ok(())
}
```

**Packages used:**
- `axum` — minimal async HTTP server for `/metrics` endpoint
- `reqwest` — HTTP client for webhook POSTs
- `serde_json` — already in tree, used for JSON serialization
- `chrono` — UTC timestamps in RFC 3339 format

---

## 15. CLI & Config

**Files:** `driftmap-cli/src/{main.rs, config.rs}`
**Estimated size:** ~200 lines

```toml
# driftmap.toml — full example

[watch]
interface = "eth0"       # network interface to attach eBPF probe to
target_a = "127.0.0.1:3000"
target_b = "127.0.0.1:3001"
sampling_rate = 0.10     # capture 10% of traffic
window_ms = 500          # request matching window

[thresholds]
drift   = 0.05           # score above which endpoint is DRIFTING
diverged = 0.50          # score above which endpoint is DIVERGED
hysteresis_secs = 30     # how long threshold must hold before state transition
min_samples = 50         # samples needed before scoring starts

[ignore]
headers = ["date", "x-request-id", "x-trace-id", "server-timing"]
endpoints = ["/healthz", "/ping"]    # never diff these

[[equivalence_rules]]
endpoint = "GET /api/users/:id"
ignore_fields = ["last_seen_at", "session_token"]

[[plugins]]
path = "./plugins/checkout_scorer.wasm"
applies_to = ["POST /api/checkout"]

[export]
prometheus_port = 9090   # 0 = disabled
webhook_url = ""         # empty = disabled
ndjson = false
```

```rust
// driftmap-cli/src/config.rs

use serde::Deserialize;

#[derive(Deserialize)]
pub struct Config {
    pub watch:       WatchConfig,
    pub thresholds:  ThresholdConfig,
    pub ignore:      IgnoreConfig,
    pub equivalence_rules: Vec<EquivalenceRule>,
    pub plugins:     Vec<PluginConfig>,
    pub export:      ExportConfig,
}

// Each sub-struct derives Deserialize — toml crate handles the rest
// Zero custom parsing code needed
```

```rust
// driftmap-cli/src/main.rs

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "driftmap", about = "Runtime semantic diff for live systems")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Watch two live services and surface behavioral drift
    Watch {
        #[arg(long)] config: Option<PathBuf>,
        #[arg(long)] target_a: Option<String>,
        #[arg(long)] target_b: Option<String>,
    },
    /// Show recent diverging response pairs for an endpoint
    Diff {
        endpoint: String,
        #[arg(long, default_value = "10")] last: usize,
    },
    /// Export current scores as JSON to stdout
    Export {
        #[arg(long, default_value = "json")] format: String,
    },
    /// Generate a driftmap.toml interactively
    Init,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    match cli.command {
        Command::Watch { config, target_a, target_b } => {
            let cfg = load_config(config)?;
            // Boot the pipeline:
            // 1. Load eBPF probe → attach to interface
            // 2. Start reassembler
            // 3. Start HTTP parser
            // 4. Start matcher
            // 5. Start scorer
            // 6. Start TUI
            // All connected via tokio::mpsc channels
            run_watch(cfg).await
        }
        Command::Diff { endpoint, last } => {
            let store = Store::open(".driftmap.db")?;
            let pairs = store.recent_pairs(&endpoint, last)?;
            for pair in pairs { print_pair_diff(&pair); }
            Ok(())
        }
        // ...
    }
}
```

**Packages used:**
- `clap` — CLI parsing with derive macros
- `toml` — deserializes `driftmap.toml` into our config structs
- `tracing-subscriber` — formats and outputs tracing logs

---

## 16. Integration Map

How data flows through every component:

```
Network Interface (eth0)
    │
    ▼
[eBPF TC Hook]  driftmap-probe/src/main.rs
    │  writes PacketEvent (1500 bytes max) to ring buffer
    ▼
[Ring Buffer Reader]  driftmap-core/src/capture.rs
    │  reads PacketEvents, dispatches to Reassembler
    ▼
[TCP Reassembler]  driftmap-core/src/capture.rs
    │  buffers raw bytes per TCP stream
    │  emits RawHttpMessage when boundary detected
    ▼
[HTTP Parser]  driftmap-core/src/http.rs
    │  parses headers, body, templatizes path
    │  emits HttpRequest or HttpResponse
    ▼
[Request Matcher]  driftmap-core/src/matcher.rs
    │  pairs requests from Target A with requests from Target B
    │  emits MatchedPair (req_a, res_a, req_b, res_b)
    ├──────────────────────────────────────────┐
    ▼                                          ▼
[Raw Diff Engine]                        [Schema Inferrer]
driftmap-core/src/diff.rs               driftmap-core/src/schema.rs
    │  emits RawDiff                          │  updates SchemaNode per endpoint
    │                                         │  emits SchemaDiff when schemas diverge
    └──────────┬──────────────────────────────┘
               ▼
[Distribution Tracker]  driftmap-core/src/distribution.rs
    │  updates TDigest per (endpoint, field, target)
    │
    ▼
[Semantic Scorer]  driftmap-core/src/scorer.rs
    │  combines RawDiff + SchemaDiff + distributions → DriftScore (0.0–1.0)
    │  also calls WASM plugins if configured
    │
    ▼
[State Machine]  driftmap-core/src/state.rs
    │  updates endpoint state (UNKNOWN/EQUIVALENT/DRIFTING/DIVERGED)
    │  emits StateTransition events
    │
    ├──────────────┬──────────────────────┬──────────────┐
    ▼              ▼                      ▼              ▼
[SQLite Store] [TUI Dashboard]    [Prometheus /metrics] [Webhook]
store.rs       driftmap-tui/      export.rs             export.rs
persists       renders terminal   scraped by            fires POST
state+scores   UI every 100ms     Prometheus            on transition
```

**Channel types between stages:**
```rust
// All stages communicate via tokio::sync::mpsc or watch channels
// mpsc = one-to-one message queue (ordered, bounded)
// watch = one-to-many broadcast (only latest value, no backlog)

capture   → reassembler : mpsc::channel(1024)  // 1024 packet events buffered
reassembler → parser    : mpsc::channel(256)   // 256 raw HTTP messages buffered
parser    → matcher     : mpsc::channel(256)   // 256 parsed HTTP messages
matcher   → scorer      : mpsc::channel(1024)  // 1024 matched pairs
scorer    → tui         : watch::channel(...)  // TUI always gets latest scores, no backlog
scorer    → store       : mpsc::channel(64)    // writes to SQLite, bounded
```

---

## 17. Package Decision Table

| Feature | Package Used | Why | Alternative Considered |
|---------|-------------|-----|----------------------|
| eBPF programs | `aya-ebpf` | Pure Rust, no C toolchain | `libbpf-rs` — needs C headers |
| HTTP parsing | `httparse` | Zero-copy, 4KB, no deps | `hyper` — too heavy, full HTTP stack |
| JSON parsing | `serde_json` | De facto standard | `simd-json` — marginal speed gain, complexity |
| Terminal UI | `ratatui` | Active tui-rs fork, good widget set | `cursive` — less control over layout |
| Terminal I/O | `crossterm` | Cross-platform, works everywhere | `termion` — Linux-only |
| WASM runtime | `wasmtime` | Fastest, Bytecode Alliance, great Rust API | `wasmer` — similar but more complex API |
| SQLite | `rusqlite` | Mature, bundled SQLite (`features=["bundled"]`) | `sqlx` — async but heavier |
| CLI | `clap` | Best Rust CLI library, derive macros | `argh` — less features |
| Config | `toml` | Rust-native, serde integration | `serde_yaml` — YAML syntax worse for config |
| Async runtime | `tokio` | Standard Rust async runtime | `async-std` — smaller community |
| HTTP server (metrics) | `axum` | Minimal, tokio-native, good ergonomics | `warp` — more complex |
| HTTP client (webhook) | `reqwest` | Standard, tokio-native | `ureq` — sync only |
| Logging | `tracing` | Structured, async-aware | `log` — no structured fields |
| Error handling | `anyhow` | Simple, no boilerplate | `thiserror` — for library errors (used in core) |
| Timestamps | `chrono` | RFC 3339, UTC support | `time` — similar, but chrono more widely known |

**Built entirely from scratch (no package):**
- TCP stream reassembler
- Path templatizer (`:id` extraction)
- Request matching algorithm (pairing by `method+path+time_window`)
- t-digest streaming quantile estimator
- Semantic scoring formula
- Drift state machine with hysteresis
- Schema inference engine
- WASM plugin ABI and memory protocol

---

## 18. Line Count Estimates

| File | Estimated Lines | Notes |
|------|----------------|-------|
| `driftmap-probe/src/main.rs` | 120 | eBPF is verbose but limited by kernel API surface |
| `driftmap-probe-common/src/lib.rs` | 30 | Just the shared struct |
| `driftmap-core/src/capture.rs` | 200 | Reassembler is the most complex stream-handling code |
| `driftmap-core/src/http.rs` | 180 | Parser + path templatizer |
| `driftmap-core/src/matcher.rs` | 160 | Pairing algorithm + GC |
| `driftmap-core/src/diff.rs` | 140 | Header set ops + body compare |
| `driftmap-core/src/schema.rs` | 250 | Inference engine is the most logic-dense |
| `driftmap-core/src/distribution.rs` | 170 | t-digest + divergence score |
| `driftmap-core/src/scorer.rs` | 130 | Weighted combination formula |
| `driftmap-core/src/state.rs` | 100 | State machine is conceptually simple |
| `driftmap-core/src/store.rs` | 150 | SQLite setup + 4 queries |
| `driftmap-core/src/export.rs` | 120 | Prometheus + NDJSON + webhook |
| `driftmap-core/src/plugins.rs` | 180 | WASM host, memory protocol |
| `driftmap-tui/src/app.rs` | 120 | Event loop + app state |
| `driftmap-tui/src/ui.rs` | 180 | Layout, all widgets |
| `driftmap-tui/src/events.rs` | 50 | Key bindings |
| `driftmap-plugin-sdk/src/lib.rs` | 80 | Public SDK for plugin authors |
| `driftmap-cli/src/main.rs` | 120 | Subcommand dispatch |
| `driftmap-cli/src/config.rs` | 80 | TOML schema |
| **Total** | **~2,560 lines** | **Entire working system** |

This is achievable because:
- eBPF handles the hard networking work in the kernel
- `httparse` handles the hairy header parsing edge cases
- `serde` + `serde_json` eliminate all serialization boilerplate
- `ratatui`/`wasmtime`/`rusqlite` are batteries-included for their domains
- Every module does exactly one thing — no file is responsible for more than its listed job

The result is a system that does genuinely novel work (live behavioral diffing via eBPF) in under 3,000 lines of Rust.

---

*This document is the complete technical specification of DriftMap. Every feature is buildable from this description. Start with Phase 0 of the ROADMAP — get the eBPF probe compiling before touching anything else, since the BPF toolchain setup is the biggest environment hurdle.*
