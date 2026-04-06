# 🗺️ DriftMap — Project Roadmap

> **Runtime Semantic Diff for Live Systems**
> Watches two live environments simultaneously and surfaces *behavioral* divergence — not config diffs, not log diffs, but a continuously updating map of where two systems are drifting apart and how fast.

---

## Table of Contents

- [Vision](#vision)
- [Non-Goals](#non-goals)
- [Architecture Overview](#architecture-overview)
- [Phase 0 — Foundations](#phase-0--foundations-weeks-14)
- [Phase 1 — Core Engine (MVP)](#phase-1--core-engine-mvp-weeks-58)
- [Phase 2 — Semantic Layer](#phase-2--semantic-layer-weeks-912)
- [Phase 3 — Dashboard & Developer UX](#phase-3--dashboard--developer-ux-weeks-1316)
- [Phase 4 — Plugin System & Extensibility](#phase-4--plugin-system--extensibility-weeks-1720)
- [Phase 5 — Hardening & Ecosystem](#phase-5--hardening--ecosystem-weeks-2126)
- [Milestone Summary](#milestone-summary)
- [Tech Stack](#tech-stack)
- [Contributing](#contributing)
- [Open Questions](#open-questions)

---

## Vision

Modern teams run multiple environments — staging, production, canary, A/B variants. When behavior diverges between them, the debugging process is slow and manual: compare configs, grep logs, correlate traces. There is no tool that **continuously watches two live systems and tells you semantically what is drifting**.

DriftMap fills that gap.

It intercepts live traffic at the network/syscall layer using eBPF, compares request/response behavior between two targets, and outputs a living diff: which endpoints agree, which diverge, how quickly the gap is growing, and what the shape of the divergence is.

**The core insight:** behavioral equivalence is probabilistic, not byte-level. Two systems can return different JSON key orderings and be semantically identical. They can return identical status codes and be semantically broken. DriftMap reasons about the *meaning* of responses, not their bytes.

---

## Non-Goals

These are explicitly out of scope to keep the project focused:

- ❌ Not a full observability platform (we are not Datadog/Grafana)
- ❌ Not a load testing or traffic replay tool (we are not k6/Gatling)
- ❌ Not a protocol fuzzer or security scanner
- ❌ Not a service mesh or proxy (we observe, we don't route)
- ❌ Not an alert manager — we surface drift, others decide what to do with it

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────┐
│                      DriftMap                           │
│                                                         │
│  ┌──────────────┐        ┌──────────────────────────┐   │
│  │  eBPF Probe  │───────▶│   Traffic Capture Layer  │   │
│  │  (per host)  │        │   (normalize, serialize) │   │
│  └──────────────┘        └──────────┬───────────────┘   │
│                                     │                   │
│                          ┌──────────▼───────────────┐   │
│                          │   Semantic Compare Engine │   │
│                          │   - Schema inference      │   │
│                          │   - Distribution diff     │   │
│                          │   - Drift scoring         │   │
│                          └──────────┬───────────────┘   │
│                                     │                   │
│                ┌────────────────────▼─────────────────┐ │
│                │         Drift State Store             │ │
│                │   (time-series of behavioral deltas)  │ │
│                └──────┬──────────────────┬────────────┘ │
│                       │                  │              │
│              ┌────────▼──────┐  ┌────────▼──────────┐  │
│              │   TUI (tui-rs)│  │  JSON/gRPC Export │  │
│              └───────────────┘  └───────────────────┘  │
└─────────────────────────────────────────────────────────┘
```

**Two deployment modes:**

1. **Sidecar mode** — agent runs alongside each service, captures traffic locally, ships to central comparator
2. **Mirror mode** — single comparator receives mirrored traffic from both targets (e.g. via iptables/tc)

---

## Phase 0 — Foundations `[Weeks 1–4]`

> Goal: Repo is set up, development environment works, and a "hello world" eBPF packet capture runs.

### 0.1 Repository Setup
- [ ] Initialize repo with `cargo new driftmap --workspace`
- [ ] Define workspace members: `driftmap-core`, `driftmap-probe`, `driftmap-tui`, `driftmap-cli`
- [ ] Add `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `SECURITY.md`
- [ ] Set up GitHub Actions CI: `cargo check`, `cargo test`, `cargo clippy`, `cargo fmt --check`
- [ ] Add `deny.toml` for license and vulnerability checking (`cargo-deny`)
- [ ] Set up `CHANGELOG.md` following Keep a Changelog format
- [ ] Add issue templates: bug report, feature request, discussion

### 0.2 eBPF Probe Skeleton
- [ ] Research and choose eBPF framework: **Aya** (pure Rust eBPF) vs libbpf-rs
  - Decision rationale: Aya is chosen — pure Rust, no C dependency, good community
- [ ] Set up Aya workspace: `driftmap-probe` (eBPF program) + `driftmap-probe-common` (shared types)
- [ ] Write a minimal kprobe/tracepoint program that attaches to `sys_enter_read`
- [ ] Confirm attach/detach lifecycle works without kernel panics
- [ ] Document minimum kernel version requirement (≥ 5.8 for ring buffers)
- [ ] Add Vagrant/Docker dev environment for kernel testing

### 0.3 Packet Capture Strategy
- [ ] Evaluate capture points (choose one for MVP):
  - `TC (traffic control) hook` — post-routing, works for both ingress/egress ✅ chosen
  - `XDP` — faster but ingress-only
  - `kprobe on tcp_sendmsg / tcp_recvmsg` — works but high overhead
- [ ] Write eBPF TC hook that captures raw TCP payload bytes for HTTP/1.1
- [ ] Implement ring buffer from eBPF → userspace (using Aya's `AsyncPerfEventArray` or `RingBuf`)
- [ ] Confirm latency overhead < 2% under load (benchmark target)

### 0.4 Config & CLI Skeleton
- [ ] Define initial `driftmap.toml` config schema (targets, ports, sampling rate)
- [ ] Scaffold CLI with `clap`: subcommands `watch`, `diff`, `export`, `version`
- [ ] Implement `driftmap watch --target-a <addr> --target-b <addr>` as the core entry point
- [ ] Add structured logging with `tracing` crate

**Phase 0 Exit Criteria:**
- `driftmap watch` attaches eBPF probe on two ports and prints raw captured bytes to stdout
- CI is green
- README explains what the project does and how to run Phase 0

---

## Phase 1 — Core Engine (MVP) `[Weeks 5–8]`

> Goal: End-to-end pipeline. Capture HTTP traffic from two targets, parse it, and produce a raw behavioral diff to stdout.

### 1.1 HTTP Parser
- [ ] Write HTTP/1.1 parser on top of raw TCP bytes (no framework — we need zero-copy control)
  - Parse request: method, path, headers, body
  - Parse response: status code, headers, body
- [ ] Handle chunked transfer encoding
- [ ] Handle partial packets and stream reassembly (TCP is a stream, not messages)
- [ ] Add HTTP/2 detection (flag as "not yet supported" with clear error)
- [ ] Unit test suite: 50+ fixture payloads from real services

### 1.2 Request Matching Engine
- [ ] Design the **pairing problem**: given a request to Target A and a request to Target B for the same endpoint, how do we match them?
  - Strategy: `(method, path-template, time-window)` as the matching key
  - Path templating: `/users/123` → `/users/:id` (simple regex-based extraction)
- [ ] Implement sliding time window matcher (configurable, default 500ms)
- [ ] Handle unmatched requests (logged as "unpaired" — could indicate routing divergence itself)
- [ ] Add sampling: configurable rate (1%, 10%, 100%) to limit overhead

### 1.3 Raw Diff Engine
- [ ] For each matched pair, compute:
  - **Status code match** — exact
  - **Header diff** — set difference (which headers appear in A but not B)
  - **Body diff** — raw byte diff (Phase 2 adds semantic layer on top)
  - **Latency delta** — p50/p95/p99 per endpoint, rolling 60s window
- [ ] Assign each diff a **raw divergence score** (0.0 = identical, 1.0 = completely different)
- [ ] Aggregate scores per endpoint path
- [ ] Write diff events to an in-memory ring buffer (configurable size)

### 1.4 First Output: Plaintext Reporter
- [ ] Implement stdout reporter: every 5s, print a table of endpoints sorted by divergence score
- [ ] Format:
  ```
  ENDPOINT                 REQUESTS  DIVERGENCE  STATUS  LATENCY_DELTA
  POST /api/orders         1,203     0.82        ⚠ 12%   +340ms p95
  GET  /api/users/:id      8,431     0.04        ✓       +2ms p95
  GET  /healthz            120       0.00        ✓       0ms
  ```
- [ ] Add `--json` flag to output newline-delimited JSON instead

**Phase 1 Exit Criteria:**
- Point DriftMap at two real HTTP services (e.g. two local nginx instances serving different files)
- Tool correctly identifies which endpoints differ and by how much
- Blog post / README demo GIF showing this working

---

## Phase 2 — Semantic Layer `[Weeks 9–12]`

> Goal: Move from byte-level diff to meaning-level diff. Two responses that are semantically equivalent should score 0.0. Two that are structurally diverging should score high even if status codes match.

### 2.1 Schema Inference Engine
- [ ] On first N requests (configurable, default 200), infer JSON schema for each endpoint:
  - Field names and types
  - Nullable vs required
  - Array vs object shapes
- [ ] Store inferred schema per `(target, endpoint)` pair
- [ ] Detect schema drift: when Target A's inferred schema diverges from Target B's
- [ ] Output human-readable schema diff:
  ```
  POST /api/orders — Schema Drift Detected
    Target A: { id: string, status: string, items: array }
    Target B: { id: string, status: string, items: array, shipping_eta: string? }
    → Field 'shipping_eta' present in B, absent in A
  ```

### 2.2 Value Distribution Comparison
- [ ] For numeric fields, track rolling distribution (min/max/p50/p95) per target
- [ ] For enum-like string fields, track value frequency distributions
- [ ] Compute **distribution divergence** using KL divergence or simpler histogram comparison
- [ ] Flag fields where distributions are statistically diverging (configurable p-value threshold)
- [ ] Example output:
  ```
  Field 'response_time_ms' in GET /api/search
    Target A: p50=45ms  p95=120ms  p99=340ms
    Target B: p50=48ms  p95=890ms  p99=2100ms   ← DIVERGING
  ```

### 2.3 Semantic Equivalence Rules
- [ ] Implement built-in equivalence rules:
  - Ignore key ordering in JSON objects
  - Ignore whitespace-only body differences
  - Normalize timestamps to UTC before comparing (configurable)
  - Configurable field exclusions (e.g. ignore `X-Request-Id` header)
- [ ] Allow user-defined equivalence rules in `driftmap.toml`:
  ```toml
  [[equivalence_rules]]
  endpoint = "GET /api/users/:id"
  ignore_fields = ["last_seen_at", "session_token"]
  ```

### 2.4 Behavioral State Machine
- [ ] Model each endpoint's behavior as a state: `EQUIVALENT`, `DRIFTING`, `DIVERGED`, `UNKNOWN`
- [ ] State transitions based on rolling divergence score over time
- [ ] Add hysteresis to prevent flapping (must be above threshold for 30s before transitioning to DRIFTING)
- [ ] Persist state to local SQLite via `rusqlite` (survive restarts)

**Phase 2 Exit Criteria:**
- DriftMap correctly scores two semantically identical responses (different key order, timestamps) as 0.0
- DriftMap correctly detects field-level schema divergence between two API versions
- Integration test suite with 20+ real-world response pair fixtures

---

## Phase 3 — Dashboard & Developer UX `[Weeks 13–16]`

> Goal: Beautiful, useful terminal UI. A developer should be able to run DriftMap, glance at the TUI, and immediately understand the state of their systems.

### 3.1 TUI with tui-rs / Ratatui
- [ ] Set up `ratatui` (actively maintained fork of tui-rs)
- [ ] Main layout:
  ```
  ┌─────────────────────────────────────────────────────┐
  │ DriftMap  ▸ watching prod ↔ staging  [running 4m2s] │
  ├──────────────────────┬──────────────────────────────┤
  │  ENDPOINT OVERVIEW   │  SELECTED ENDPOINT DETAIL    │
  │  (scrollable list)   │  (drift timeline + schema)   │
  ├──────────────────────┴──────────────────────────────┤
  │  STATUS BAR: 12 endpoints | 2 drifting | 0 diverged │
  └─────────────────────────────────────────────────────┘
  ```
- [ ] Left panel: sortable endpoint list with color-coded divergence scores
- [ ] Right panel: selected endpoint detail view
  - Drift score timeline (sparkline, last 5 min)
  - Schema diff
  - Recent diverging response pair (side by side)
  - Latency distributions
- [ ] Keyboard navigation: `j/k` scroll, `Enter` select, `s` sort, `f` filter, `q` quit
- [ ] `?` opens help overlay

### 3.2 Historical Replay
- [ ] Store last N diverging response pairs to disk (configurable, default 1000)
- [ ] `driftmap diff --endpoint "POST /api/orders" --last 10` — show last 10 diverging pairs
- [ ] Side-by-side diff view in terminal (colored, like `git diff`)

### 3.3 Configuration Ergonomics
- [ ] `driftmap init` — interactive config generator (asks questions, writes `driftmap.toml`)
- [ ] Config hot-reload (watch `driftmap.toml` for changes, no restart required)
- [ ] Validate config on startup with clear error messages
- [ ] Example configs in `examples/` directory:
  - `examples/staging-vs-prod.toml`
  - `examples/canary-10pct.toml`
  - `examples/ab-test.toml`

### 3.4 Notifications (optional, off by default)
- [ ] Webhook support: POST to URL when endpoint transitions to DIVERGED state
- [ ] Slack-compatible webhook payload
- [ ] PagerDuty Events API v2 support
- [ ] Configurable per-endpoint alert thresholds

**Phase 3 Exit Criteria:**
- TUI is usable by someone who has never seen DriftMap before within 30 seconds
- User study: 3 developers run it against real services and report they understand the output
- Demo video recorded for README

---

## Phase 4 — Plugin System & Extensibility `[Weeks 17–20]`

> Goal: DriftMap is useful out of the box, but different teams have different protocols and equivalence definitions. WASM plugins let users extend without forking.

### 4.1 WASM Plugin Host
- [ ] Integrate `wasmtime` as plugin runtime
- [ ] Define plugin ABI: plugins receive a response pair, return a divergence score (0.0–1.0) and optional annotation
- [ ] Plugin interface (in Rust, compiled to WASM by users):
  ```rust
  pub fn score_pair(a: &Response, b: &Response) -> DriftScore {
      // user logic here
  }
  ```
- [ ] Plugin manifest format in `driftmap.toml`:
  ```toml
  [[plugins]]
  path = "./plugins/my_custom_scorer.wasm"
  applies_to = ["POST /api/checkout"]
  ```
- [ ] Sandboxing: plugins cannot access filesystem or network (WASM capability model)
- [ ] Plugin SDK crate published to crates.io: `driftmap-plugin-sdk`

### 4.2 Protocol Plugins
- [ ] gRPC support (decode protobuf using reflection API)
- [ ] GraphQL support (normalize query/response structure)
- [ ] WebSocket support (session-level behavioral diff)
- [ ] Each as an optional feature flag: `cargo build --features grpc,graphql`

### 4.3 Export & Integration Layer
- [ ] Prometheus metrics endpoint (`/metrics`) — expose drift scores as gauges
- [ ] OpenTelemetry trace export — annotate spans with drift scores
- [ ] Structured JSON log export (compatible with Datadog, Loki, Splunk)
- [ ] `driftmap export --format csv --since 1h` for offline analysis

### 4.4 Mirror Mode (Single-Host Deployment)
- [ ] Implement mirror mode: DriftMap acts as a transparent TCP proxy
  - Accepts connections, duplicates traffic to both Target A and Target B
  - Compares responses without modifying what the original client receives
- [ ] `driftmap proxy --listen :8080 --target-a :3000 --target-b :3001`
- [ ] Useful for teams that can't deploy sidecar agents

**Phase 4 Exit Criteria:**
- A custom WASM plugin can be written, compiled, loaded, and affects scoring
- gRPC traffic between two targets is correctly compared
- Prometheus metrics are scrapeable and accurate

---

## Phase 5 — Hardening & Ecosystem `[Weeks 21–26]`

> Goal: Production-ready. Trusted. Community-driven.

### 5.1 Performance & Safety
- [ ] Benchmark suite: measure CPU/memory overhead at 1k, 10k, 100k req/s
- [ ] Target: < 1% CPU overhead, < 50MB RSS in sidecar mode at 10k req/s
- [ ] Implement backpressure: drop samples gracefully when pipeline is saturated
- [ ] Memory-safe audit: `cargo-geiger` for unsafe usage, minimize to eBPF probe only
- [ ] Fuzz testing: `cargo fuzz` targets for HTTP parser and schema inference

### 5.2 Security Hardening
- [ ] eBPF probe privilege minimization: request only `CAP_NET_ADMIN` and `CAP_BPF`
- [ ] No root requirement in mirror mode (rootless deployment path)
- [ ] Sensitive data handling: configurable field redaction (e.g. redact `Authorization` header content)
- [ ] Security policy: `SECURITY.md` with responsible disclosure process
- [ ] Audit log: record when DriftMap starts/stops and what it observed

### 5.3 Packaging & Distribution
- [ ] GitHub Releases with pre-built binaries for:
  - `x86_64-unknown-linux-gnu`
  - `aarch64-unknown-linux-gnu` (ARM / Graviton)
- [ ] Docker image: `ghcr.io/yourname/driftmap:latest`
- [ ] Helm chart for Kubernetes sidecar deployment
- [ ] Homebrew formula (Linux only — macOS lacks eBPF)
- [ ] Debian/RPM packages via `cargo-deb` and `cargo-rpm`

### 5.4 Documentation Site
- [ ] Set up `mdBook` documentation site, deployed via GitHub Pages
- [ ] Sections:
  - Getting Started (< 5 minute first run)
  - How It Works (architecture deep-dive)
  - Configuration Reference
  - Plugin Development Guide
  - Deployment Patterns (sidecar, mirror, Kubernetes)
  - FAQ
- [ ] API reference auto-generated from code via `cargo doc`

### 5.5 Community & Governance
- [ ] Define project governance: maintainer roles, RFC process for major changes
- [ ] Create `GOVERNANCE.md`
- [ ] Set up GitHub Discussions for questions/ideas
- [ ] Write "good first issue" guide and label 10 starter issues
- [ ] Monthly changelog blog posts
- [ ] Define v1.0 stability guarantee (what APIs are stable, what can change)

**Phase 5 Exit Criteria:**
- DriftMap has been run in production by at least one external team (case study written)
- Documentation site is live
- v0.9.0 release candidate published
- v1.0.0 shipped 🎉

---

## Milestone Summary

| Milestone | Target | Deliverable |
|-----------|--------|-------------|
| **M0** — Skeleton | Week 4 | eBPF probe captures raw TCP bytes |
| **M1** — Raw Diff | Week 8 | HTTP traffic diffed, stdout reporter |
| **M2** — Semantic Diff | Week 12 | Schema inference, distribution comparison |
| **M3** — TUI | Week 16 | Full terminal dashboard |
| **M4** — Plugins | Week 20 | WASM plugins, gRPC, Prometheus |
| **M5** — v1.0 | Week 26 | Production-ready, packaged, documented |

---

## Tech Stack

| Layer | Choice | Reason |
|-------|--------|--------|
| Language | **Rust** | Performance, memory safety, great eBPF ecosystem |
| eBPF Framework | **Aya** | Pure Rust, no C toolchain required |
| HTTP parsing | **custom** | Zero-copy, full control over stream reassembly |
| TUI | **Ratatui** | Actively maintained tui-rs fork |
| CLI | **Clap** | Best-in-class Rust CLI framework |
| State store | **SQLite via rusqlite** | Embedded, zero-ops, sufficient for our needs |
| Plugin runtime | **Wasmtime** | Fastest WASM runtime, Bytecode Alliance backed |
| Config format | **TOML** | Readable, Rust-native with `toml` crate |
| Logging | **tracing** | Structured, async-aware |
| Testing | **cargo test + nextest** | Fast parallel test runner |

---

## Contributing

DriftMap is built in the open. Contributions of all kinds are welcome.

Before starting significant work, please open a GitHub Discussion or issue to align on approach. This prevents duplicated effort and wasted PRs.

**Good first issues** are labeled [`good first issue`](../../issues?q=label%3A%22good+first+issue%22) — these are scoped, well-documented, and don't require deep knowledge of the eBPF internals.

See [`CONTRIBUTING.md`](./CONTRIBUTING.md) for the full contribution guide.

---

## Open Questions

These are unresolved design decisions. Input welcome — open a Discussion.

1. **Matching strategy for stateful protocols** — For WebSockets and long-lived connections, how do we define a "session equivalent" between two targets? Time-windowed? Message-count-windowed?

2. **Schema inference confidence** — How many samples are needed before schema inference is reliable enough to surface diffs? What do we show the user during the warm-up period?

3. **gRPC without reflection** — Many gRPC services don't expose a reflection API. Should we require users to provide `.proto` files? Or use a best-effort binary format heuristic?

4. **Cross-host clock synchronization** — When matching requests across two hosts, clocks may drift by milliseconds. What's the right tolerance for the time-window matcher?

5. **Sampling strategy** — Should we sample randomly, or use a smarter strategy (e.g. prioritize endpoints with historically high divergence scores)?

6. **License** — Apache-2.0 vs MIT vs AGPL. AGPL prevents cloud providers from offering DriftMap as a hosted service without contributing back. Trade-off: adoption vs sustainability.

---

*Roadmap is a living document. Phases may shift based on community feedback and real-world usage. Open a Discussion if you think something should be reprioritized.*

*Last updated: April 2026*
