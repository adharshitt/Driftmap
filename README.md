<p align="center">
  <h1 align="center">🗺️ DriftMap</h1>
  <p align="center">
    <strong>Runtime Semantic Diff for Live Systems</strong>
    <br/>
    <em>Powered by eBPF, Rust, and WebAssembly</em>
  </p>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Rust-1.80+-orange.svg" alt="Rust Version">
  <img src="https://img.shields.io/badge/eBPF-Aya-blue.svg" alt="eBPF">
  <img src="https://img.shields.io/badge/WASM-Wasmtime-purple.svg" alt="WASM Plugins">
  <img src="https://img.shields.io/badge/License-Apache%202.0-green.svg" alt="License">
  <img src="https://img.shields.io/badge/Status-Enterprise%20Ready-success.svg" alt="Status">
</p>

## 📖 Overview

**DriftMap** is an enterprise-grade observability platform that watches two live environments (e.g., Staging vs. Production, or V1 vs. V2 of a microservice) simultaneously and surfaces **behavioral divergence** in real-time. 

Unlike traditional tools that diff static configurations or logs, DriftMap uses kernel-level eBPF to capture live network traffic with near-zero overhead. It semantically compares the responses of two systems to the exact same requests, ignoring non-deterministic fields (like timestamps or UUIDs) to highlight true architectural regressions.

---

## 📊 Real-Time Terminal Dashboard

DriftMap features a highly optimized, `ratatui`-powered Terminal UI to monitor semantic drift in real-time without leaving your SSH session.

```text
┌ Endpoints (s:Score n:Name r:Requests) ──────────────┐ ┌ Details ──────────────────────────────────────┐
│ ✓    0.0%  GET /api/health                          │ │ Endpoint: GET /api/users/:id                  │
│ ⚠   12.4%  GET /api/users/:id                       │ │ Behavioral Divergence Score: 12.4%            │
│ ✗   85.2%  POST /api/orders                         │ │ Total Sample Count: 14,230                    │
│ ✓    1.2%  GET /api/products                        │ │                                               │
│ ✓    0.5%  PUT /api/settings                        │ │ Diagnostic Breakdown:                         │
│                                                     │ │ - Protocol Status:   0.0%  (100% Match)       │
│                                                     │ │ - Structural Schema: 45.0% (Missing 'discount')│
│                                                     │ │ - Latency Profile:   4.2%  (p95 Δ: +12ms)     │
│                                                     │ │ - Header Signatures: 0.0%  (Ignored dynamic)  │
└─────────────────────────────────────────────────────┘ └───────────────────────────────────────────────┘
```
*(Simulated TUI Dashboard)*

---

## 🏗️ Architecture & Data Flow

DriftMap is designed for high-throughput, low-latency packet processing.

```mermaid
graph TD
    Client([Client Traffic]) -->|TCP| Interface(eth0 / en0)
    
    subgraph Kernel Space [Linux Kernel]
        TC[eBPF Traffic Control Hook]
    end
    
    Interface --> TC
    TC -->|NetworkPacketEvent| RingBuf[(eBPF Ring Buffer)]
    
    subgraph Userspace [DriftMap Core (Rust)]
        Reassembler[TCP TrafficCaptureBuffer]
        Matcher[Sliding Window Matcher]
        
        subgraph Semantic Engine
            Schema[StructuralSchemaDivergence]
            TDigest[StreamingQuantileEstimator]
            Normalizer[JSON Normalizer]
        end
        
        WASM((WASM Plugin Host))
    end
    
    RingBuf --> Reassembler
    Reassembler -->|HTTP Messages| Matcher
    Matcher -->|Matched Pairs| Normalizer
    Normalizer --> Schema
    Normalizer --> TDigest
    Schema --> WASM
    TDigest --> WASM
    WASM -->|BehavioralDivergenceScore| SQLite[(SQLite State Store)]
    
    SQLite --> TUI[Ratatui Dashboard]
    SQLite --> Prom[Prometheus /metrics]
    SQLite --> Webhook[State Webhooks]
```

---

## ✨ Enterprise Features

*   **Near-Zero Overhead Capture:** Uses eBPF Traffic Control (TC) hooks to duplicate raw packet data straight from the kernel. No sidecar proxies required.
*   **Semantic Equivalence Engine:**
    *   **Schema Inference:** Recursively analyzes JSON structures to detect added, removed, or type-shifted fields.
    *   **Statistical Divergence:** Uses constant-memory `t-Digest` algorithms to track p50, p95, and p99 latency and numeric value distributions.
    *   **Smart Normalization:** Automatically strips timestamps, UUIDs, and configurable non-deterministic fields.
*   **WASM Extensibility:** Write custom protocol scorers (e.g., gRPC, GraphQL) in any language that compiles to WebAssembly and load them at runtime safely via Wasmtime.
*   **Enterprise Operations:**
    *   **Zero-Downtime Hot-Reload:** Watches `driftmap.toml` via `notify` for live configuration updates without dropping packets or resetting the eBPF maps.
    *   **Telemetry Integration:** Exposes a Prometheus `/metrics` endpoint and fires JSON Webhooks for state transitions (e.g., `EQUIVALENT` → `DIVERGED`).
    *   **Local Persistence:** SQLite WAL-mode datastore for historical replay and offline unified-color diffing.

---

## 🚀 Getting Started

### 1. Interactive Initialization
Run the built-in wizard (`dialoguer`-powered) to safely provision your configuration:
```bash
driftmap init
```

### 2. Start Watching
Attach the eBPF probe and launch the real-time TUI:
```bash
sudo driftmap watch --config driftmap.toml
```

### 3. Analyze Divergence
Replay unified colored diffs of the most recent drifted responses, formatted just like `git diff`:
```bash
driftmap diff "POST /api/orders" --last 5
```

---

## 🔌 Extensibility: gRPC Example

DriftMap ships with a WASM SDK. Here is how simple it is to write a custom gRPC frame scorer in Rust:

```rust
// plugins/grpc-scorer/src/lib.rs
use driftmap_plugin_sdk::{DriftPlugin, PluginScore, Request, Response, export_plugin};

pub struct GrpcScorer;

impl DriftPlugin for GrpcScorer {
    fn score_pair(req_a: &Request, res_a: &Response, req_b: &Request, res_b: &Response) -> PluginScore {
        // 1. Parse the 5-byte gRPC frame header
        // 2. Safely compare protobuf lengths and payloads
        PluginScore { score: 0.0, annotation: core::ptr::null(), annotation_len: 0 }
    }
}
export_plugin!(GrpcScorer);
```

## 🛡️ Security & Hardening
- **Denial of Service Prevention:** `TrafficCaptureBuffer` streams are strictly capped at 1MB per connection, and unmatched `PendingRequest` queues are bounded to prevent HashDoS.
- **Sandboxed Evaluation:** All WASM plugins are executed with strict `Wasmtime` fuel limits (100,000 execution units) and memory bounds to prevent CPU hangs.
- **Fuzz Tested:** The core HTTP zero-copy parser is continuously evaluated against `cargo-fuzz` (libFuzzer) to ensure it never panics on malformed network data.

---

<p align="center">
  <em>Built with precision for top-tier infrastructure teams.</em>
</p>
