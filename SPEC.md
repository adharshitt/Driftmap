# DriftMap — Complete 26-Week Technical Specification
## No Code. Pure Architecture. Every Decision Explained.

> Every file. Every package. Every optimization technique. Every integration method.
> Every week accounted for. This is the complete engineering blueprint.

---

## Table of Contents

1. [The Problem Being Solved — Deep](#1-the-problem-being-solved--deep)
2. [Workspace & File Structure — Complete](#2-workspace--file-structure--complete)
3. [Technology Stack — Every Choice Justified](#3-technology-stack--every-choice-justified)
4. [Package Inventory — Use vs Build Decision](#4-package-inventory--use-vs-build-decision)
5. [Every Feature — Deep Technical Explanation](#5-every-feature--deep-technical-explanation)
6. [System-Level Optimization — Low End to High End](#6-system-level-optimization--low-end-to-high-end)
7. [Integration Architecture](#7-integration-architecture)
8. [26-Week Breakdown — Week by Week](#8-26-week-breakdown--week-by-week)
9. [Line Count & Complexity Estimates](#9-line-count--complexity-estimates)
10. [Risk & Hard Problems](#10-risk--hard-problems)

---

## 1. The Problem Being Solved — Deep

### What Exists Today

Every team with multiple environments (staging, prod, canary, A/B) faces the same
problem: when those environments start behaving differently, you only find out after
something breaks. The current toolkit:

- **Config differ** (diff two YAML files) — tells you what changed in config, says
  nothing about how the running system actually behaves as a result
- **Observability tools** (Datadog, Grafana, Prometheus) — show you the health of
  ONE system. You can look at two dashboards side by side, but no tool computes
  the behavioral delta between them automatically
- **Log differ** — you grep logs from both environments and compare. Manual. Slow.
  Requires you to already know what to look for
- **Traffic replay** (Diffy — Twitter's old abandoned OSS project, Scientist gem in Ruby)
  — replay recorded traffic to both targets and compare responses. But these are
  HTTP-only, need code instrumentation, are not continuous, and Diffy is abandoned
- **Shadow testing** — mirror production traffic to staging. Tools exist for this
  (AWS traffic mirroring, Envoy's shadow routing) but they only duplicate traffic;
  the comparison work still falls entirely on you

### The Gap DriftMap Fills

DriftMap is the missing piece: a continuously running behavioral comparator that
requires zero code changes, zero instrumentation, and zero manual work. It watches
two systems at the network level, reasons about whether their behaviors are
semantically equivalent (not just byte-equal), and gives you a live score for every
endpoint showing how fast the two systems are drifting apart.

The key insight that makes DriftMap different from everything else:
**Semantic equivalence, not byte equality.**

Two responses that differ in JSON key order, whitespace, timestamp formatting,
request ID values, and server-timing headers can be 100% semantically identical.
Two responses that return identical `200 OK` status codes can be completely
diverged if one started returning empty arrays where the other returns populated ones.
Byte-level tools miss both of these. DriftMap catches them.

### Who Uses This

1. **Deployment validation** — deploy new version to staging, point DriftMap at
   staging vs prod, watch scores in real time. If nothing diverges after 10 minutes
   of real traffic, you have strong evidence the deploy is safe
2. **Canary analysis** — compare canary fleet (5% traffic) against stable fleet (95%).
   DriftMap tells you if canary is behaving differently before users report bugs
3. **Migration validation** — migrating from PostgreSQL to CockroachDB, Python to Go,
   monolith to microservices. Point DriftMap at old and new. Know when they're
   equivalent
4. **Debugging** — "staging works but prod doesn't." DriftMap shows you exactly which
   endpoint, which field, and when the divergence started

---

## 2. Workspace & File Structure — Complete

### Why a Cargo Workspace

A Cargo workspace is a collection of Rust crates that share a single `Cargo.lock`
file and can reference each other locally. This matters for DriftMap because:

- The eBPF probe crate MUST compile to a different target
  (`bpfel-unknown-none` — no operating system, no std library) than everything else
  (`x86_64-unknown-linux-gnu` — normal Linux). A workspace lets one `cargo build`
  command handle both targets with different flags
- Shared types (the `PacketEvent` struct) can live in one crate and be imported by
  both the eBPF probe and userspace code without duplication
- CI can test individual crates in parallel, cutting build times
- The plugin SDK can be published to crates.io independently of the main binary

### Complete Directory Tree

```
driftmap/
│
├── Cargo.toml                        ← workspace root, pins ALL dependency versions
├── Cargo.lock                        ← committed — reproducible builds
├── .cargo/
│   └── config.toml                   ← build target overrides for eBPF crate
├── deny.toml                         ← cargo-deny: license + vulnerability policy
├── rust-toolchain.toml               ← pins nightly toolchain (needed for eBPF)
│
├── driftmap-probe/                   ← CRATE 1: eBPF programs (kernel space)
│   ├── Cargo.toml
│   └── src/
│       └── main.rs                   ← TC hook program (~120 lines)
│
├── driftmap-probe-common/            ← CRATE 2: types shared between eBPF + userspace
│   ├── Cargo.toml
│   └── src/
│       └── lib.rs                    ← PacketEvent struct, no_std (~30 lines)
│
├── driftmap-core/                    ← CRATE 3: all userspace logic (no UI, no CLI)
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs                    ← public API surface of the core (~40 lines)
│       ├── capture.rs                ← ring buffer reader + TCP reassembler (~200 lines)
│       ├── http.rs                   ← HTTP/1.1 parser + path templatizer (~180 lines)
│       ├── matcher.rs                ← request pairing engine (~160 lines)
│       ├── diff.rs                   ← raw byte/header/status diff (~140 lines)
│       ├── schema.rs                 ← JSON schema inference engine (~250 lines)
│       ├── distribution.rs           ← t-digest streaming quantile tracker (~170 lines)
│       ├── scorer.rs                 ← semantic divergence scorer (~130 lines)
│       ├── state.rs                  ← drift state machine with hysteresis (~100 lines)
│       ├── store.rs                  ← SQLite persistence layer (~150 lines)
│       ├── export.rs                 ← Prometheus + NDJSON + webhook (~120 lines)
│       └── plugins.rs                ← WASM plugin host (~180 lines)
│
├── driftmap-tui/                     ← CRATE 4: terminal UI binary
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs                   ← entry point, wires up app (~40 lines)
│       ├── app.rs                    ← app state struct + event loop (~120 lines)
│       ├── ui.rs                     ← all widget rendering + layout (~180 lines)
│       └── events.rs                 ← keyboard input handler (~50 lines)
│
├── driftmap-plugin-sdk/              ← CRATE 5: published to crates.io for plugin authors
│   ├── Cargo.toml
│   └── src/
│       └── lib.rs                    ← Request/Response/PluginScore types + macro (~80 lines)
│
├── driftmap-cli/                     ← CRATE 6: the `driftmap` binary users run
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs                   ← clap subcommand dispatch (~120 lines)
│       └── config.rs                 ← TOML config schema + loader (~80 lines)
│
├── tests/                            ← integration tests (separate from unit tests)
│   ├── http_fixtures/                ← 50+ real HTTP request/response payloads
│   │   ├── req_001.bin
│   │   └── ...
│   ├── integration_test.rs           ← end-to-end: spin up two mock servers, run DriftMap
│   └── fuzz/                        ← cargo-fuzz targets
│       ├── fuzz_http_parser.rs
│       └── fuzz_schema_infer.rs
│
├── examples/                         ← runnable examples + config files
│   ├── staging-vs-prod.toml
│   ├── canary-10pct.toml
│   ├── ab-test.toml
│   └── simple-plugin/               ← example WASM plugin project
│       ├── Cargo.toml
│       └── src/
│           └── lib.rs
│
├── plugins/                          ← built-in optional plugins
│   └── grpc-scorer/                  ← gRPC semantic scorer plugin
│       ├── Cargo.toml
│       └── src/
│           └── lib.rs
│
├── docs/                             ← mdBook documentation source
│   ├── book.toml
│   └── src/
│       ├── SUMMARY.md
│       ├── getting-started.md
│       ├── how-it-works.md
│       ├── config-reference.md
│       ├── plugin-guide.md
│       └── deployment.md
│
├── deploy/                           ← deployment artifacts
│   ├── Dockerfile
│   ├── helm/                        ← Kubernetes Helm chart
│   │   ├── Chart.yaml
│   │   ├── values.yaml
│   │   └── templates/
│   └── systemd/
│       └── driftmap.service
│
├── scripts/
│   ├── build-ebpf.sh                ← builds probe with correct target flags
│   ├── bench.sh                     ← runs benchmarks and compares baseline
│   └── release.sh                   ← bumps version, tags, triggers GitHub Actions
│
└── .github/
    ├── workflows/
    │   ├── ci.yml                   ← check + test + clippy + fmt on every PR
    │   ├── release.yml              ← build binaries for all targets on tag push
    │   └── docs.yml                 ← build + deploy mdBook to GitHub Pages
    └── ISSUE_TEMPLATE/
        ├── bug_report.md
        └── feature_request.md
```

### Crate Dependency Direction

The rule: data flows DOWN. No crate imports from a crate above it in this list.

```
driftmap-probe-common    ← imported by both probe and core
driftmap-probe           ← standalone, only imports probe-common
driftmap-core            ← imports probe-common for PacketEvent type
driftmap-plugin-sdk      ← standalone, imported by core (plugin host) and by plugin authors
driftmap-tui             ← imports core for DriftScore, State types
driftmap-cli             ← imports core + tui, is the top-level binary
```

This means you can test `driftmap-core` in complete isolation without a terminal or
CLI. You can build a web UI later and just replace `driftmap-tui`. The architecture
is not coupled to any output format.

---

## 3. Technology Stack — Every Choice Justified

### Language: Rust

Not a preference — a technical requirement. Three reasons:

**1. eBPF programs in Rust via Aya:**
eBPF programs are compiled to a restricted bytecode that runs inside the Linux kernel.
They have no access to the standard library, no heap allocation, no function pointers,
and no loops that the BPF verifier can't prove terminate. Rust's type system maps onto
these constraints perfectly. C is the traditional choice but C has no safety guarantees
and the eBPF verifier rejects unsafe patterns that C compilers would allow.

**2. Zero-cost abstractions at packet capture scale:**
At 10,000 requests/second, the capture pipeline processes ~15,000 TCP segments per
second (each request typically spans 2-4 segments). Each segment is 1,500 bytes. That's
22 MB/s through the reassembler alone. Rust gives C-level performance with no GC pauses.
A GC pause of even 5ms at this throughput would cause segment buffer overflow and sample loss.

**3. Memory safety for network parsing:**
Network parsers written in C have a decades-long history of buffer overflows, use-after-
free bugs, and integer overflows. HTTP parsers are among the most-exploited attack surfaces
in security history. Rust's ownership model eliminates these entire classes of bugs at
compile time. For a tool that runs as root (required for eBPF), memory safety is critical.

### Async Runtime: Tokio

Tokio is the standard async runtime for Rust. The alternative is async-std, which has
a smaller community and fewer production deployments. DriftMap needs async because:
- The capture loop, HTTP parser, matcher, scorer, TUI refresh, and SQLite writes all
  run concurrently. They are I/O-bound (waiting on ring buffer data, waiting on disk)
  not CPU-bound, so async fits better than threads
- Tokio's `mpsc` channels (multi-producer single-consumer) are the primary connective
  tissue between pipeline stages — they're the queues between each processing step
- Tokio's `watch` channel (single-producer multi-consumer, only latest value) is how
  the scorer broadcasts updated scores to the TUI and export layer simultaneously

### eBPF Framework: Aya

Aya is a pure-Rust eBPF library. The alternative is `libbpf-rs` (Rust bindings around
the C `libbpf` library). Aya is chosen because:
- No C toolchain dependency. `libbpf-rs` requires `clang`, `llvm`, and kernel headers
  to be installed on the build machine. Aya compiles eBPF programs using the Rust compiler
  directly. This makes CI simpler and cross-compilation possible
- Aya supports the full eBPF feature set: TC hooks, XDP, kprobes, ring buffers, all map
  types (hash maps, arrays, ring buffers, perf event arrays)
- The userspace half of Aya is a normal Rust crate that integrates cleanly with Tokio —
  `AsyncPerfEventArray` and `RingBuf` both work natively in async contexts

### Database: SQLite via rusqlite

DriftMap stores three kinds of data persistently:
1. Current drift state per endpoint (survive restarts)
2. Historical drift scores (for sparklines — last 24 hours)
3. Last 1,000 diverging response pairs per endpoint (for `driftmap diff` replay)

SQLite is chosen over every other option:

- **vs PostgreSQL/MySQL**: DriftMap is a single-binary tool. Requiring a separate database
  daemon to be running is a massive deployment barrier. Zero ops overhead matters here
- **vs RocksDB**: RocksDB is a key-value store optimized for write-heavy workloads.
  Our query patterns (recent scores for an endpoint, range scans by time) are SQL-shaped
  and map poorly to key-value semantics. We'd end up reimplementing query logic
- **vs flat files**: Concurrent reads from the TUI + concurrent writes from the scorer
  require locking semantics. SQLite's WAL mode (Write-Ahead Logging) gives concurrent
  readers without blocking writers, which flat files cannot provide
- **vs in-memory only**: Users need to restart DriftMap without losing the state of
  which endpoints are DRIFTING vs EQUIVALENT. Persistence is required

SQLite in WAL mode with `synchronous=NORMAL` handles our write rate (one score write
per 5 seconds per endpoint) trivially. At 100 endpoints, that's 20 writes/second —
SQLite handles 100,000 writes/second.

### Terminal UI: Ratatui + Crossterm

Ratatui is the actively maintained fork of `tui-rs` (original maintainer archived the
repo in 2023). It provides layout primitives (split panels, constraints), widget types
(lists, tables, gauges, sparklines, paragraphs), and a rendering loop that writes to
a terminal backend.

Crossterm is the cross-platform terminal I/O layer — it handles entering raw mode
(so individual keypresses are captured without waiting for Enter), reading keyboard
events, and writing ANSI escape codes. It works on Linux, macOS, and Windows.

The alternative stack would be `cursive` (a higher-level TUI framework). Cursive is
easier to use but gives less control over layout. For DriftMap's split-panel layout
with sparklines, we need Ratatui's lower-level control.

### WASM Runtime: Wasmtime

Wasmtime is the reference implementation of the WebAssembly specification, maintained
by the Bytecode Alliance (Mozilla, Fastly, Intel, RedHat). It is the fastest Rust WASM
runtime and has the best API for embedding in Rust applications.

The alternative is Wasmer, which has a more complex API and historically had more
correctness issues. Wasmtime's security model (WASM capability-based sandboxing) means
plugins literally cannot access the filesystem or network — they only see the memory
the host gives them. This is critical for a security-sensitive tool.

---

## 4. Package Inventory — Use vs Build Decision

### Packages Used (Don't Build These)

| Package | Version Range | What It Does | Why Not Build |
|---------|--------------|--------------|---------------|
| `aya-ebpf` | 0.1.x | eBPF program macros, map types, helper functions | Kernel BPF helper bindings take months to get right safely |
| `aya` | 0.1.x | Userspace: load eBPF programs, read ring buffers | Complex kernel interface, maintained by dedicated team |
| `aya-log` / `aya-log-ebpf` | 0.1.x | Debug logging from eBPF side | Trivial to use, hard to build (crosses kernel boundary) |
| `httparse` | 1.x | HTTP/1.x header parser | 15 years of edge cases, zero-copy, battle-tested |
| `serde` | 1.x | Serialization/deserialization framework | Macro-based, 10k lines of macros, not worth reimplementing |
| `serde_json` | 1.x | JSON parsing and serialization | Handles every edge case in the JSON spec |
| `tokio` | 1.x | Async runtime, channels, timers | Multi-year effort, impossible to replicate |
| `ratatui` | 0.26+ | Terminal UI widgets and layout | Widget library with many edge cases |
| `crossterm` | 0.27+ | Cross-platform terminal I/O | Platform differences are painful, let library handle it |
| `wasmtime` | 20.x | WASM execution engine | Compiler + runtime = months of work minimum |
| `rusqlite` | 0.31+ | SQLite Rust bindings (bundled SQLite) | C bindings + bundled amalgamation = correct approach |
| `clap` | 4.x | CLI argument parsing with derive macros | Handles help text, validation, subcommands, completions |
| `toml` | 0.8+ | TOML deserialization | Config file format parsing is gnarly |
| `tracing` | 0.1.x | Structured async-aware logging framework | Integrates with Tokio, spans across async boundaries |
| `tracing-subscriber` | 0.3.x | Formats and outputs tracing logs to stderr | Pluggable formatters, log filtering |
| `anyhow` | 1.x | Ergonomic error propagation in application code | Box<dyn Error> with context, one line of boilerplate |
| `thiserror` | 1.x | Error type definitions in library code | Derives Display + Error cleanly |
| `axum` | 0.7+ | HTTP server for /metrics endpoint (50 lines) | Minimal tokio-native web framework |
| `reqwest` | 0.12+ | HTTP client for webhook POSTs | Standard Rust HTTP client, async, easy API |
| `chrono` | 0.4+ | UTC timestamps, RFC 3339 formatting | Date/time is full of edge cases |

### Packages Built From Scratch (No External Crate)

| Component | Why No Package Exists | Estimated Lines |
|-----------|----------------------|----------------|
| TCP Stream Reassembler | No crate reassembles eBPF-captured segments into HTTP messages | ~120 lines |
| Path Templatizer | No crate converts `/users/123` → `/users/:id` with configurable rules | ~60 lines |
| Request Pairing Algorithm | The time-window request matcher is DriftMap-specific logic | ~120 lines |
| t-Digest Implementation | Existing Rust t-digest crates don't support SQLite serialization or merging | ~130 lines |
| Schema Inference Engine | No crate infers a schema from a stream of JSON samples and diffs two schemas | ~200 lines |
| Drift State Machine | Hysteresis-based state machine for EQUIVALENT/DRIFTING/DIVERGED is domain-specific | ~80 lines |
| Semantic Scorer | The weighted formula combining status/schema/latency/header scores is novel | ~80 lines |
| WASM Plugin ABI | The memory-passing protocol between DriftMap and user plugins is custom | ~60 lines |

### Why Some Obvious Choices Are Rejected

**`hyper` for HTTP parsing:** hyper is a complete HTTP client/server library. We don't
need a server or client — we need a parser that works on raw bytes from TCP reassembly.
hyper assumes it controls the I/O layer, which conflicts with our eBPF capture approach.

**`nom` for HTTP parsing:** nom is a parser combinator framework. `httparse` is 10x
simpler for HTTP specifically and was written by the same people who wrote hyper. Using
nom for HTTP would be reinventing what `httparse` already does perfectly.

**`sled` for storage:** sled is an embedded Rust database. It's faster than SQLite for
raw key-value operations but has no SQL query capabilities. Our queries (range scans by
time, per-endpoint lookups) would require manual index management that SQLite gives for free.

**`opentelemetry` SDK:** The OpenTelemetry Rust SDK is very heavy (dozens of dependencies).
For our use case (export drift scores as metrics and annotate traces), Prometheus exposition
format covers 90% of users. OTel can be a Phase 5 addition.

---

## 5. Every Feature — Deep Technical Explanation

### Feature 1: eBPF Traffic Capture

**What the kernel-space program does:**
The TC (Traffic Control) hook is a program that the kernel runs on every network packet
that passes through a network interface, after routing decisions have been made. The
program is written in restricted Rust (compiled to BPF bytecode) and has strict
constraints: no unbounded loops, no function pointers, no dynamic memory allocation,
no system calls. It runs in kernel context with direct access to packet data.

The program does five things per packet:
1. Reads the Ethernet header to confirm this is IPv4 (skip IPv6 for MVP)
2. Reads the IP header to confirm this is TCP (skip UDP)
3. Reads the TCP header to extract source and destination ports
4. Checks whether either port is in a BPF hash map of "watched ports" that userspace
   populated at startup. If neither port matches, the program returns immediately — this
   is the hot path, must be fast
5. Reads up to 1,500 bytes of TCP payload (one MTU worth) and writes a `PacketEvent`
   into a BPF ring buffer

The ring buffer is the handoff mechanism between kernel and userspace. Kernel writes into
one end; userspace reads from the other. It's lock-free and designed for high-frequency
packet capture.

**Why TC and not other hook points:**
- XDP (eXpress Data Path): fires before routing, only sees ingress. Can't see both
  client→server and server→client traffic from one hook attachment
- kprobes on `tcp_sendmsg`/`tcp_recvmsg`: fires deep in kernel internals, extraction
  of payload bytes requires complex pointer arithmetic that BPF verifier often rejects
- uprobe on application code: requires knowing the exact binary and memory layout,
  breaks on every recompile, doesn't work for precompiled binaries
- TC is the right level of abstraction: post-routing, bi-directional, works on all
  interface types including loopback and virtual interfaces (veth, tun)

**The watched ports BPF hash map:**
Before attaching the TC hook, userspace code populates a `HashMap<u32, u8>` in BPF map
memory with the ports of Target A and Target B. The kernel-space program does O(1)
lookups into this map per packet. Adding a new port to watch requires only updating
this map — no recompilation, no hook reattachment.

**Overhead:**
The eBPF program adds approximately 1-3 microseconds of latency per packet on the critical
path. At 10,000 requests/second with average 3 segments per request, that's 30,000 program
invocations per second. Modern kernels execute this in ~15ms of total CPU time per second —
under 2% of one CPU core.

**File:** `driftmap-probe/src/main.rs` — approximately **120 lines**
**Shared types:** `driftmap-probe-common/src/lib.rs` — approximately **30 lines**

---

### Feature 2: TCP Stream Reassembler

**The fundamental problem:**
TCP is a byte-stream protocol, not a message protocol. A 4,000-byte HTTP request will
arrive as 3 separate TCP segments (3 separate PacketEvents from the eBPF probe) because
the default TCP Maximum Segment Size is ~1,460 bytes. The reassembler must buffer incoming
bytes per TCP connection and detect when a complete HTTP message has been received.

**How connections are identified:**
Each TCP connection is identified by its 4-tuple: (source IP, source port, destination IP,
destination port). DriftMap maintains a `HashMap<StreamKey, StreamBuffer>` — one entry per
active TCP connection. The `StreamKey` is the 4-tuple. The `StreamBuffer` holds accumulated
bytes and a timestamp of last activity.

**How HTTP message boundaries are detected:**
HTTP/1.1 has two boundary signals:
1. The end of headers is always `\r\n\r\n` (carriage-return newline twice)
2. The body length is declared by the `Content-Length` header. For chunked encoding, the
   body ends with `0\r\n\r\n`

The reassembler scans the accumulated buffer for `\r\n\r\n`, then reads the `Content-Length`
value to know how many more bytes to wait for before emitting a complete message.

**Direction detection:**
The reassembler must also know whether bytes are a REQUEST (client→server) or RESPONSE
(server→client). This is determined by the destination port: if `dst_port == target_a_port`,
the bytes are going TO Target A (so they're a request). If `src_port == target_a_port`, the
bytes are coming FROM Target A (so they're a response to a previous request).

**Memory management:**
Each `StreamBuffer` is a `Vec<u8>` that grows as segments arrive and shrinks as complete
messages are extracted. After a complete message is extracted, the bytes are drained from
the front of the Vec using `drain(0..message_len)`. This avoids copying — the remaining
bytes (start of the next message) stay in the same allocation.

**Garbage collection:**
TCP connections can die without a clean FIN/RST handshake (process kill, network drop).
Without GC, the `HashMap` would grow without bound. A background task runs every 1 second
and removes any `StreamBuffer` whose `last_seen` timestamp is older than 5 seconds. This
5-second timeout is configurable.

**Backpressure:**
If the HTTP parser stage is slow, the channel between reassembler and parser fills up.
The reassembler uses `try_send` (non-blocking) rather than `send` (blocking). If the
channel is full, the reassembler drops the message and increments a `dropped_messages`
counter that the TUI can display. This is intentional — dropping samples is safer than
blocking the capture loop (which would cause the eBPF ring buffer to overflow and lose
packets at the kernel level).

**File:** Part of `driftmap-core/src/capture.rs` — approximately **120 lines** of the file's ~200 total

---

### Feature 3: HTTP Parser

**Input:** Raw bytes of one complete HTTP message (request or response)
**Output:** A structured `HttpRequest` or `HttpResponse` with method, path, headers, body

**Request vs response detection:**
HTTP responses always begin with `HTTP/` (the version string). HTTP requests begin with
a method name (`GET`, `POST`, `PUT`, `DELETE`, etc.). The parser checks the first 5 bytes
to determine which type it's dealing with.

**Header parsing with `httparse`:**
`httparse` is a zero-copy HTTP parser — it doesn't allocate memory during parsing. It works
by taking a slice of bytes and returning indices into that slice (not copies of strings).
DriftMap uses `httparse` for the header portion only, then handles body extraction manually.

`httparse` is given a pre-allocated array of `EMPTY_HEADER` values (max 64 headers — enough
for real-world HTTP). It fills them in during parsing. DriftMap then iterates these headers,
converts them to lowercase (HTTP headers are case-insensitive), and copies them into a
`Vec<(String, String)>`. The copy happens once here and is amortized against all downstream
processing.

**Path templatization:**
This is one of the most important functions in DriftMap. The core insight: `/users/123` and
`/users/456` are logically the same endpoint — `GET /users/:id`. Without templatization,
DriftMap would track thousands of "unique" endpoints (one per user ID) instead of one.

The algorithm:
1. Split path at `?` to discard query string (query params are not part of the endpoint template)
2. Split by `/` to get path segments
3. For each segment, test whether it's "likely an identifier":
   - All-numeric: `123`, `456` → `:id`
   - UUID format (8-4-4-4-12 hex): `f47ac10b-58cc-4372-a567-0e02b2c3d479` → `:id`
   - Long hex strings (>8 chars, all hex): `a3f8b2c1d4e5` → `:id`
   - Short alphabetic strings are kept as-is: `users`, `api`, `v2`
4. Rejoin with `/`

This templatization must be tuned per deployment — a string like `nyc01` might be a
legitimate path segment (a datacenter name) or a parameterized ID (a location code).
The config file allows users to override the templatization rules for specific paths.

**Why not use a router library for templatization:**
Router libraries like `matchit` or `path-tree` work in the opposite direction — they take
a known set of route patterns and match incoming paths against them. DriftMap works in the
opposite direction: it sees unknown paths and must infer the pattern. There's no library
for this inference problem.

**File:** `driftmap-core/src/http.rs` — approximately **180 lines**

---

### Feature 4: Request Matcher

**The problem stated precisely:**
DriftMap receives two interleaved streams of HTTP message pairs:
- Stream A: requests to Target A and their responses
- Stream B: requests to Target B and their responses

It must produce pairs: (request_A, response_A, request_B, response_B) where the A-request
and B-request represent "the same logical operation" so their responses can be meaningfully compared.

**The matching key:**
Two requests are considered "the same logical operation" if:
- Same HTTP method (GET, POST, etc.)
- Same templatized path (`/users/:id` matches `/users/123` and `/users/456`)
- Both arrived within a configurable time window (default: 500 milliseconds)

**Data structure:**
Two `HashMap<String, VecDeque<PendingRequest>>` — one for unmatched requests from A,
one for unmatched requests from B. The key is `"METHOD /path/template"`. The value is
a queue of pending requests (FIFO).

When a new request arrives from A:
1. Check if B's queue has a pending request for the same key
2. If yes: pop the oldest pending B-request, form a `MatchedPair`, emit downstream
3. If no: push the new A-request onto A's queue and wait

**The time window problem:**
Traffic to two services is never perfectly synchronized. At 100 req/s, requests arrive
every 10ms on average. With network jitter, a request that hits Target A at time T might
hit Target B at T+50ms or T-50ms. The 500ms window accommodates this jitter.

The window creates a tradeoff: wider window = more matches but more false matches (pairing
requests that aren't actually related). Narrower window = fewer false matches but more
unmatched requests. 500ms is calibrated for same-datacenter deployments. Cross-region
deployments may need 1-2 seconds.

**FIFO matching:**
Within the same key (same endpoint), requests are matched in FIFO order. This is correct
because HTTP servers are generally FIFO per connection: the first request to A corresponds
to the first request to B for the same endpoint. However, if A's queue has 3 pending
`GET /users/:id` requests and B's queue has 0, it means B hasn't received those yet (or
is much faster). The 3 pending requests will wait up to 500ms for matches before being
discarded as "unmatched."

**Unmatched request tracking:**
Unmatched requests are interesting signals in themselves. If A has many unmatched requests
for endpoint X and B has none, it may mean B is returning errors for X (which would be
caught by the diff engine when matches do occur) or that B is routing X differently.
These unmatched counts are surfaced in the TUI as a secondary metric.

**File:** `driftmap-core/src/matcher.rs` — approximately **160 lines**

---

### Feature 5: Raw Diff Engine

**Input:** A `MatchedPair` containing two complete request+response pairs
**Output:** A `RawDiff` describing every difference at the HTTP protocol level

**Status code comparison:**
Simple integer equality. `200 != 404` is a diff. This is the highest-weight signal in
the scorer because status code changes directly indicate behavioral divergence.

**Header comparison:**
Headers are normalized to lowercase (HTTP spec says they're case-insensitive) before
comparison. Set operations produce three categories:
1. Headers present in A but not B (e.g. A returns `X-Cache: HIT`, B doesn't)
2. Headers present in B but not A (e.g. B added `Content-Security-Policy`, A hasn't)
3. Headers present in both but with different values (e.g. different `Content-Type` charset)

A built-in exclusion list of headers that are inherently non-deterministic and should
never be diffed: `date`, `x-request-id`, `x-trace-id`, `x-amzn-requestid`,
`server-timing`, `cf-ray`, `x-cache`, `age`. Users can add to this list in config.

**Body comparison:**
Phase 1 (MVP): pure byte comparison — `body_a == body_b`. Fast, but will generate false
positives for JSON key ordering differences and timestamp variations. This is intentional
for MVP — the semantic layer (schema inference, Phase 2) handles these false positives.

Phase 2 addition: before byte comparison, attempt JSON normalization:
1. Parse both bodies as JSON (if both Content-Type are application/json)
2. Sort all object keys recursively
3. Normalize timestamp strings to a canonical format
4. Strip configured ignored fields
5. Then compare the normalized representations

**Latency calculation:**
The latency of each response is calculated in the capture layer by recording the
timestamp when the first byte of the request was seen and the timestamp when the last
byte of the response was received. The difference is the observed latency. The `RawDiff`
stores `latency_delta = latency_a - latency_b` in microseconds.

**File:** `driftmap-core/src/diff.rs` — approximately **140 lines**

---

### Feature 6: Schema Inference Engine

**Why schema inference matters:**
Consider a backend migrating from API v1 to v2. The response for `GET /orders/:id`
changes: v1 returns `{"id": 1, "total": 99.99}`. v2 returns `{"id": 1, "total": 99.99,
"items": [...], "shipping": {...}}`. The status codes match (both 200). The byte
comparison fails (bodies are different). But is this a problem? It's a new API version
adding fields — which might be intentional.

Schema inference tells DriftMap: "Target B's responses for `GET /orders/:id` now include
`items` (Array, required) and `shipping` (Object, required) which Target A does not return."
This is a structural drift that humans should be aware of, even if it's intentional.

**How inference works:**
For each (endpoint, target) pair, maintain a `HashMap<FieldName, FieldStats>` where
`FieldStats` tracks:
- The inferred type (String, Integer, Float, Boolean, Object, Array, Null)
- How many responses contained this field (`seen_count`)
- How many responses total have been observed (`total_count`)
- Whether the field was ever null

Presence rate = `seen_count / total_count`. Fields with presence rate > 95% are "required."
Fields with presence rate 5-95% are "optional." Fields with presence rate < 5% are
"rare" and probably not meaningful signal.

**Warm-up period:**
Schema inference is not meaningful until enough samples have been observed. The default
`min_samples` is 50. Below this threshold, DriftMap shows "warming up" for that endpoint's
schema and emits no schema diffs. This prevents false alarms during the first minute of
watching a low-traffic endpoint.

**Nested object handling:**
JSON responses are often deeply nested. `{"user": {"profile": {"age": 25}}}` — the
schema inferrer must recurse into nested objects. The `FieldType::Object` variant itself
contains a `SchemaNode` (a `HashMap<FieldName, FieldStats>`), allowing arbitrary depth.

For performance, inference depth is capped at 5 levels by default (configurable). Very
deep nesting (10+ levels) is rare in practice and would create very large schema structures.

**Array handling:**
Arrays are typed by their element type: `Array(Box<FieldType>)`. If an array always
contains integers, it's `Array(Integer)`. If it contains mixed types, it's `Array(Mixed)`.
The first non-null element of the first observed array determines the type (heuristic,
but sufficient for most real-world APIs).

**Schema diff output:**
The schema differ compares the inferred schemas for (endpoint, Target A) and
(endpoint, Target B) and produces:
- Fields present in A's schema but absent in B (and vice versa)
- Fields present in both but with different inferred types
- Fields that changed from required to optional or vice versa

**File:** `driftmap-core/src/schema.rs` — approximately **250 lines**

---

### Feature 7: Distribution Tracker (t-Digest)

**What distributions are tracked:**
1. **Latency per endpoint** — the most important distribution. p50/p95/p99 of response
   time in microseconds, separately for Target A and Target B
2. **Numeric field values** — for JSON response fields that are numeric (e.g. `{"score": 0.87,
   "count": 42}`), their value distributions are tracked. This catches cases where the
   distribution of values shifts even though the schema stays the same

**Why t-Digest:**
Computing exact quantiles (p95, p99) requires storing every observation. For latency over
24 hours at 1,000 requests/second, that's 86 million values — gigabytes of RAM.

The t-Digest algorithm is a streaming quantile estimator that maintains a compact
representation (about 200 "centroids") regardless of how many samples have been seen.
It provides approximate quantile answers that are:
- Exact at the extremes (p0, p100)
- Very accurate in the tails (p95, p99 error < 1% in practice)
- Memory-bounded: always O(200 centroids) = O(1) space

**Why build t-Digest from scratch:**
Existing Rust t-digest crates (`tdigest`, `approx`) exist but:
- They don't implement the `merge` operation (needed to combine results from multiple
  sidecar agents in the future)
- They don't implement serialization to SQLite (needed for persistence across restarts)
- They use different internal representations that don't match what we need for
  divergence scoring

Building it from scratch takes ~130 lines and gives complete control.

**Divergence scoring from distributions:**
Given two t-Digests (one for Target A, one for Target B), the divergence score is
computed by comparing quantiles:
- p50 delta: `|p50_a - p50_b| / max(p50_a, p50_b)`
- p95 delta: `|p95_a - p95_b| / max(p95_a, p95_b)`
- Weighted combination: p95 gets 70% weight (latency spikes matter more than median shifts)
- Clamped to [0.0, 1.0]

**File:** `driftmap-core/src/distribution.rs` — approximately **170 lines**

---

### Feature 8: Semantic Scorer

**The final reduction:**
Everything upstream (raw diff, schema diff, distribution tracking) produces signals.
The semantic scorer combines all signals into one number per endpoint: a `DriftScore`
from 0.0 (identical behavior) to 1.0 (completely diverged).

**The weighting formula:**
```
DriftScore =
  status_score   × 0.40   (fraction of recent pairs with different status codes)
  schema_score   × 0.30   (normalized count of schema divergences)
  latency_score  × 0.20   (distribution divergence for latency)
  header_score   × 0.10   (fraction of pairs with header differences)
```

The weights encode domain knowledge about what matters:
- Status code changes (40% weight) are the clearest indicator of broken behavior
- Schema changes (30%) indicate API contract changes — often intentional but always significant
- Latency divergence (20%) indicates performance regression or infrastructure difference
- Header changes (10%) are the least meaningful — often cosmetic (different server version, etc.)

These weights are not hardcoded. The config file will allow users to override them for
their specific use case (some teams care more about latency than schema).

**Rolling window:**
The scorer doesn't score individual pairs. It scores the last N pairs per endpoint
(default N=1000). This means the score is a rolling average of recent behavior, not
a snapshot of the last single request. An endpoint that's 90% identical and 10% diverged
will score around 0.10, not 0 or 1.

**WASM plugin integration:**
After computing its own score, the scorer checks if any WASM plugins apply to this
endpoint. If they do, it calls each plugin with the raw request/response pair and
receives an additional score. The final score is `max(driftmap_score, plugin_score)`.
This ensures plugins can only raise the score (flag more drift), not suppress it.

**File:** `driftmap-core/src/scorer.rs` — approximately **130 lines**

---

### Feature 9: Drift State Machine

**The four states:**

`UNKNOWN` — the endpoint has been seen but hasn't yet accumulated enough samples
(< `min_samples`, default 50) to produce reliable scores. Displayed in the TUI as grey.

`EQUIVALENT` — the rolling drift score is below the drift threshold (default 0.05 = 5%).
Normal, expected state for healthy deployments. Displayed green.

`DRIFTING` — the rolling drift score is between 0.05 and 0.50 (5-50%). Something is
different but it's not clearly broken. Could be a legitimate A/B test, could be a subtle
regression. Displayed yellow. Warrants investigation.

`DIVERGED` — the rolling drift score is above 0.50 (50%). Significant behavioral
difference. Likely indicates a bug, failed migration, or unintended change. Displayed red.
Triggers webhooks/alerts if configured.

**Hysteresis:**
Without hysteresis, a single aberrant request pair could cause an endpoint to flash from
EQUIVALENT to DRIFTING and back to EQUIVALENT within seconds. This would spam alerts and
make the TUI noisy.

Hysteresis means a threshold must be crossed and STAY crossed for a minimum duration
(default 30 seconds) before the state transitions. Implementation: each endpoint tracks
a `threshold_crossed_at` timestamp. When the score crosses a threshold, this timestamp
is set. On each subsequent score update, if the timestamp is set AND the score is still
above the threshold AND `elapsed >= 30s`, THEN the state transitions.

If the score drops back below the threshold before 30 seconds, `threshold_crossed_at`
is cleared and the clock resets.

**State transition events:**
Every state transition produces a `StateTransition` struct:
`{endpoint, from_state, to_state, score_at_transition, timestamp}`. These events flow
to three consumers: the SQLite store (persist for history), the TUI (update the display),
and the export layer (fire webhooks, write to log). This event-driven architecture means
new consumers (e.g. a Slack integration) can be added without touching the state machine.

**File:** `driftmap-core/src/state.rs` — approximately **100 lines**

---

### Feature 10: TUI Dashboard

**Layout philosophy:**
The TUI is designed for a developer who opened it in a terminal window alongside their
deployment dashboard. They need to understand the state of all watched endpoints at a
glance (left panel) and drill into one specific endpoint for details (right panel).
The whole UI must be readable in a 120×40 terminal (standard for a split-screen laptop).

**Left panel — Endpoint List:**
A scrollable, keyboard-navigable list. Each row shows:
- A status symbol: `✓` (green, EQUIVALENT), `⚠` (yellow, DRIFTING), `✗` (red, DIVERGED)
- The drift score as a percentage: `8.3%`
- The endpoint name, truncated to fit: `GET /api/orders/:id`
- Request count since DriftMap started: `12,304`

The list is sortable by score (default), by endpoint name (alphabetical), or by request
count. Sort key is toggled with keyboard shortcuts shown in the status bar.

**Right panel — Endpoint Detail:**
Shows detailed information for the currently selected endpoint:

*Top section — Drift Score Sparkline:* A time-series graph of the drift score for the
last 5 minutes (60 data points, one per 5 seconds). Uses Ratatui's `Sparkline` widget.
Gives immediate visual sense of whether the score is trending up, down, or stable.

*Middle section — Score Breakdown:* Four lines showing the sub-scores:
- Status divergence: `X%`
- Schema divergence: `X%`
- Latency divergence: `X% (p95 A=120ms B=890ms)`
- Header divergence: `X%`

*Bottom section — Last Diverging Pair:* The most recent response pair that contributed
to the divergence score. Shows a side-by-side diff of the two response bodies, similar
to `git diff` output (red lines for A-only content, green lines for B-only content).
This is the most actionable part of the UI — it shows exactly what's different.

**Status bar:**
A single line at the bottom showing: total endpoints watched, counts in each state,
time since DriftMap started, and keyboard shortcut reminders.

**Keyboard controls:**
- `j` / `k` or arrow keys: navigate endpoint list
- `Enter`: select endpoint (updates right panel)
- `s`: sort by score
- `n`: sort by name
- `r`: sort by request count
- `f`: enter filter mode (type to filter endpoint list by substring)
- `?`: toggle help overlay
- `q`: quit

**Refresh rate:**
The TUI redraws every 100ms (10 fps). This is fast enough to feel responsive without
burning CPU on rendering. Score updates from the pipeline arrive via a Tokio `watch`
channel — the TUI always renders the latest score, no buffering.

**Files:** `driftmap-tui/src/app.rs` (~120 lines) + `driftmap-tui/src/ui.rs` (~180 lines)
+ `driftmap-tui/src/events.rs` (~50 lines) = approximately **350 lines total**

---

### Feature 11: WASM Plugin System

**The extensibility problem:**
DriftMap's built-in semantic scoring works well for JSON REST APIs. But teams use:
- gRPC with protobuf binary bodies (DriftMap can't parse binary protobuf without schema)
- GraphQL (response shapes are query-dependent)
- Custom binary protocols
- Domain-specific equivalence rules ("these two UUIDs are semantically equivalent because
  they refer to the same database record under different systems")

WASM plugins solve this without making DriftMap depend on protobuf, GraphQL libraries, etc.
Plugin authors compile their scorer to WASM and DriftMap loads it at runtime.

**Plugin interface (what plugin authors implement):**
A single exported function: `score_pair(request_body_a, response_body_a, request_body_b,
response_body_b) -> f32`. All data is passed as byte slices. The plugin interprets them
however it wants (parse as protobuf, as JSON, as raw bytes) and returns a score 0.0-1.0.

**Memory passing protocol:**
WASM modules have their own linear memory space. To pass data to a plugin, DriftMap must:
1. Call the plugin's `alloc(n_bytes)` function (which every plugin must export)
2. Receive back a pointer (offset into WASM linear memory)
3. Write bytes into the WASM memory at that offset using the host's memory access API
4. Call `score_pair` with the pointer and length as arguments
5. The plugin reads its own memory at those offsets

This is the standard WASM guest-host data passing pattern. It's verbose but safe — the
plugin cannot access any memory outside its own WASM linear memory space.

**Sandboxing:**
Wasmtime runs WASM in a fully sandboxed environment. By default, WASM modules:
- Cannot access the filesystem (no `open()`, `read()`, `write()`)
- Cannot make network connections
- Cannot fork processes
- Cannot access environment variables

This makes plugins safe to run even from untrusted sources. The worst a malicious plugin
can do is return a wrong score — it cannot exfiltrate data or compromise the host system.

**Plugin loading:**
At startup, DriftMap reads the `[[plugins]]` sections from `driftmap.toml`. For each
plugin:
1. Load the `.wasm` file from the specified path
2. Compile it with Wasmtime (JIT compilation happens once at load time)
3. Instantiate it with an empty linker (no host functions exposed — pure computation only)
4. Verify the required exports exist (`alloc`, `score_pair`)
5. Store the instance in a `PluginHost` struct for the lifecycle of the process

**Plugin configuration:**
Each plugin has an `applies_to` list in the config: patterns of endpoints this plugin
should be called for. If `applies_to = ["POST /api/checkout"]`, the plugin only runs
for checkout endpoint pairs, not every endpoint. This avoids calling slow plugins on
high-frequency endpoints.

**Files:** `driftmap-plugin-sdk/src/lib.rs` (~80 lines) + `driftmap-core/src/plugins.rs`
(~180 lines) = approximately **260 lines total**

---

### Feature 12: Export Layer

**Prometheus metrics endpoint:**
DriftMap starts a minimal HTTP server on port 9090 (configurable, 0 to disable). The
`/metrics` route returns drift scores in Prometheus exposition format:

Each endpoint becomes two gauges: `driftmap_score{endpoint="GET_api_users_:id"}` and
`driftmap_score_samples{endpoint="GET_api_users_:id"}`. The endpoint label has slashes
and spaces replaced with underscores to conform to Prometheus label format.

Teams can then build Grafana dashboards on top of this — plotting drift score over time,
alerting when score exceeds 0.5, correlating drift spikes with deployment timestamps.

**Newline-delimited JSON:**
When `--ndjson` flag is set or `ndjson = true` in config, every score update is written
to stdout as a single-line JSON object. This is the standard format for log shippers
(Datadog Agent, Loki promtail, Fluentd, Logstash). The log shipper picks up the JSON
lines and forwards them to the observability platform.

**Webhooks:**
When an endpoint transitions state (e.g. EQUIVALENT → DIVERGED), DriftMap POSTs a JSON
payload to the configured webhook URL. The payload contains endpoint name, old state,
new state, current score, and timestamp. This is compatible with Slack incoming webhooks
(Slack accepts any JSON with a `text` field), PagerDuty Events API v2, and generic
webhook receivers (Zapier, n8n, Pipedream).

**`driftmap diff` command:**
A separate CLI subcommand (not the live watch mode) that queries SQLite for recent
diverging response pairs. Usage: `driftmap diff "GET /api/users/:id" --last 10`.
Output: the last 10 diverging response pairs shown side-by-side with colored diff,
sorted by timestamp descending. This is the primary debugging tool — when an alert fires,
the developer runs `driftmap diff` to see exactly what changed.

**File:** `driftmap-core/src/export.rs` — approximately **120 lines**

---

## 6. System-Level Optimization — Low End to High End

### Level 1: eBPF-Level Optimizations (Kernel Space)

**Port filtering in BPF:**
The very first operation in the TC hook is a BPF HashMap lookup for the port number.
If the port doesn't match, the program returns immediately. This means packets to
unrelated services (your database, your Redis, your OS background traffic) cost only
one map lookup (~50ns) instead of full packet processing. At 1 million packets/second
on a busy server, this filter saves ~99% of processing work.

**MTU-bounded payload capture:**
The probe caps payload capture at 1,500 bytes (one MTU). Most HTTP headers fit in the
first 1,500 bytes. For large request bodies, only the first 1,500 bytes are captured —
enough for header inspection and the beginning of the body for schema inference. Large
body payloads (file uploads, large JSON arrays) are partially captured. This is an
explicit tradeoff: full fidelity would require following TCP segments across multiple
MTUs, which multiplies memory usage and processing cost.

**Ring buffer sizing:**
The eBPF ring buffer is 4MB by default. At 1,500 bytes per PacketEvent, that's 2,730
events. At 30,000 events/second (10k req/s × 3 segments/req), the ring buffer drains
every ~90ms. The userspace reader must drain the ring buffer faster than it fills.
Userspace reads in a tight async loop with no sleep — `AsyncRingBuf` notifies the
Tokio task immediately when data is available via epoll.

**BPF verifier compliance:**
The BPF verifier is the kernel's static analysis pass that rejects any program that
might access out-of-bounds memory or loop infinitely. All bounds checks in the probe
must be explicit — the verifier does not assume `payload_len < 1500` unless you prove
it with an explicit `if payload_len > 1500 { payload_len = 1500; }`. Every array access
must be preceded by a bounds check that the verifier can trace statically.

### Level 2: Userspace Data Structure Optimizations

**StreamBuffer pre-allocation:**
When a new TCP stream's first packet arrives, the `StreamBuffer` is allocated with
`Vec::with_capacity(4096)`. This pre-allocates 4KB — enough for most HTTP messages
without reallocation. For large HTTP messages (headers + body > 4KB), at most one
reallocation occurs. Measuring allocation frequency with `perf` or `heaptrack` and
tuning this initial capacity is a Phase 5 task.

**HashMap with pre-computed hashes:**
The `HashMap<StreamKey, StreamBuffer>` is looked up on every packet. Rust's default
`HashMap` uses the SipHash 1-3 algorithm (resistant to HashDoS attacks). For an
internal tool processing trusted traffic, we can swap to `AHashMap` (from the `ahash`
crate — uses AES instruction-based hashing). `AHashMap` is 2-4x faster than `HashMap`
for small keys like our 12-byte StreamKey.

**VecDeque for pending requests:**
The request matcher uses `VecDeque<PendingRequest>` — a ring buffer. Front insertions
and removals are O(1). This is correct because matching is FIFO — we pop from the front
(oldest pending) and push to the back (newest). A `Vec` would require O(N) front removal.

**t-Digest compression threshold:**
The t-Digest compresses when it exceeds `max_size` centroids (default 200). Compression
is O(N log N) (sort + merge pass). At 1,000 observations per second per endpoint with
100 endpoints, compression runs 100 times per second with N=200 — about 20,000 operations
per second. This is fast (~1ms total). But with 1,000 endpoints, it becomes 100ms per
second per core — worth profiling and potentially increasing `max_size` to reduce
compression frequency.

**SQLite batch writes:**
Instead of writing one drift score per endpoint per 5 seconds individually, batch all
pending writes into a single SQLite transaction. A transaction with 100 INSERTs takes
the same amount of time as a transaction with 1 INSERT (the expensive part is the
fsync at commit, not the INSERT count). This reduces fsync frequency from 100/second
to 1/5 seconds — 500x reduction in write overhead.

### Level 3: Tokio Runtime Configuration

**Worker thread count:**
Tokio defaults to spawning one worker thread per CPU core. DriftMap is primarily I/O
bound (waiting on ring buffer, waiting on SQLite). CPU-bound work (schema inference,
t-digest compression) is bursty but short. Optimal thread count for DriftMap is
probably 2-4 threads regardless of available CPUs. A future tuning flag can expose this.

**Task pinning:**
The ring buffer reader task should be pinned to a specific CPU core (CPU affinity) to
minimize cache invalidation. The ring buffer data flows: kernel writes to ring buffer →
CPU cache → Tokio task reads. If the Tokio task migrates to a different core, the
cache line must be transferred. `tokio::task::Builder::spawn_on` with a specific Tokio
runtime handle can achieve this.

**Channel capacity tuning:**
Each `mpsc` channel between pipeline stages has a capacity. Too small → backpressure
causes producer tasks to yield frequently, wasting context switches. Too large → memory
usage grows unbounded under load. Recommended starting capacities:
- eBPF ring buffer → reassembler: capacity 1,024 (each entry is 1,500 bytes = 1.5MB max)
- Reassembler → HTTP parser: capacity 256 (each entry is a complete HTTP message)
- HTTP parser → matcher: capacity 256
- Matcher → scorer: capacity 1,024 (matched pairs are small structs)
- Scorer → store: capacity 64 (writes are slow, need buffering)

### Level 4: Network-Level Optimizations

**Sampling:**
At very high request rates (100k req/s+), processing every request would consume multiple
CPU cores. Configurable sampling (1%, 10%, 100%) reduces processing load proportionally.
Sampling is implemented at the eBPF level: a BPF hash map contains a counter per
`(src_ip, dst_port)` pair; every Nth packet is captured, the rest are skipped immediately.
This is more efficient than userspace sampling (which still pays the ring buffer write cost).

**Selective endpoint watching:**
If only 5 of 50 endpoints need watching, configure `watch_only = ["/api/orders", "/api/payments"]`
in the config. At the eBPF level, this can be implemented as a second hash map check —
skip packets whose path prefix doesn't match the watch list. However, this requires reading
the HTTP path from inside eBPF, which means parsing enough of the TCP payload to find the
path — only possible after confirming the packet is the start of a new HTTP request
(SYN+ACK or first data segment). This is Phase 4 work.

**TC vs XDP performance:**
TC hooks run at software interrupt level (after routing, before socket delivery). XDP
hooks run even earlier (before routing, in the driver). For purely observational tools
like DriftMap, TC is appropriate. If DriftMap ever needs to manipulate packets (not
planned), XDP would be needed. TC overhead at 1Gbps is measurable (~5% CPU on a core
dedicated to TC processing). DriftMap's sampling means far less than 1Gbps of traffic
is processed by the BPF program.

### Level 5: Memory Optimization

**Body storage:**
Storing full response bodies for the "last N diverging pairs" replay feature is the
primary memory consumer. At 50KB average body size × 1,000 stored pairs × 100 endpoints
= 5GB. This is clearly too much. Two mitigations:
1. Only store bodies for diverging pairs (not all pairs) — in practice, most pairs are EQUIVALENT
2. Store bodies in SQLite (on disk) rather than in RAM. The `diverging_pairs` SQLite
   table stores body bytes as BLOBs. Retrieval for the `driftmap diff` command is a
   SQL query — fine for a read-on-demand workflow

**Schema node memory:**
Each `SchemaNode` is a `HashMap<String, FieldStats>`. For an endpoint with 20 JSON fields,
each `FieldStats` is ~40 bytes → 800 bytes per schema node. With 1,000 endpoints × 2 targets
= 2,000 schema nodes → 1.6MB. Completely acceptable.

**t-Digest memory:**
Each `TDigest` is 200 centroids × 12 bytes = 2,400 bytes. With 1,000 endpoints × 2 targets
× 10 numeric fields each = 20,000 digests = 48MB. At the upper end but manageable. Reducing
`max_size` to 100 centroids halves this with modest accuracy loss at extreme tail quantiles.

---

## 7. Integration Architecture

### How All Components Connect

The entire system is a linear pipeline with a fan-out at the end:

```
[Kernel: eBPF TC Hook]
     ↓ ring buffer (4MB, kernel↔userspace)
[Userspace: Ring Buffer Reader]    ← tokio task, runs tight loop
     ↓ mpsc channel (capacity 1024)
[TCP Stream Reassembler]           ← tokio task
     ↓ mpsc channel (capacity 256)
[HTTP Parser]                      ← tokio task
     ↓ two mpsc channels (one per target)
[Request Matcher]                  ← tokio task
     ↓ mpsc channel (capacity 1024)
[Schema Inferrer]  ←——————————————— also receives matched pairs
     ↓
[Raw Diff Engine]                  ← runs inside scorer task, same thread
     ↓
[Distribution Tracker]             ← updates happen inside scorer task
     ↓
[Semantic Scorer + WASM Plugins]   ← tokio task
     ↓ watch channel (latest scores only)
[State Machine]                    ← runs inside scorer task
     ↓ StateTransition events
[Fan-out]
     ├→ [TUI]          watch channel receiver
     ├→ [SQLite Store] mpsc channel (capacity 64)
     ├→ [Prometheus]   watch channel receiver (HTTP server reads latest scores)
     └→ [Webhook]      mpsc channel (capacity 8, fire-and-forget)
```

### Channel Type Selection

**`mpsc` (multi-producer, single-consumer, bounded):**
Used between pipeline stages where ordering matters and backpressure is desirable.
Bounded capacity means fast producers cannot overwhelm slow consumers indefinitely —
instead, the producer task yields (suspends) when the channel is full, giving the
consumer time to catch up. This is correct behavior for the capture→parser path.

**`watch` (single-producer, multi-consumer, holds only latest value):**
Used for broadcasting the current score state to all consumers (TUI, Prometheus server,
export layer). Multiple consumers need the current state but don't need every historical
state — they only care about "what are the scores right now?" If the TUI is slow to render,
it doesn't accumulate a backlog of score updates — it just reads the latest when it's ready.

**`oneshot` (single message, single consumer):**
Used for shutdown signaling. When the user presses `q`, the TUI sends a oneshot signal
to the main task, which sends shutdown signals down the pipeline in reverse order.

### eBPF Program Loading Sequence

At startup, before any traffic is captured:
1. Load the compiled eBPF object file (embedded in the binary using `include_bytes!`)
2. Create BPF maps (ring buffer, watched ports hash map)
3. Populate watched ports map with Target A and Target B ports
4. Attach the TC hook to the specified network interface
5. Spawn the Tokio task that reads from the ring buffer
6. Only then: start all downstream pipeline tasks

The eBPF object is embedded at compile time with `include_bytes!`. This makes DriftMap
a single self-contained binary — no separate `.o` file to distribute alongside it.

### Config Hot-Reload

DriftMap watches `driftmap.toml` for filesystem changes using `notify` (a cross-platform
filesystem watcher crate). When a change is detected:
1. Re-parse the config file
2. Validate the new config (if invalid, log an error and keep old config)
3. If watched ports changed: update the BPF hash map (no hook reattachment needed)
4. If thresholds changed: update threshold values in the state machine (takes effect on next score update)
5. If plugin list changed: load new plugins, unload removed ones (hot-swap)
6. If equivalence rules changed: update the rules in the scorer

Config hot-reload does NOT require restarting DriftMap. This is important for production
use — restarting would lose the warm-up period for schema inference and distribution tracking.

---

## 8. 26-Week Breakdown — Week by Week

### Phase 0: Foundations (Weeks 1–4)

**Week 1 — Environment & Toolchain**

The first week is entirely infrastructure. eBPF development requires a specific toolchain
setup that is non-trivial and must be correct before any code works.

Tasks:
- Install Rust nightly toolchain (Aya requires nightly for some eBPF features). Create
  `rust-toolchain.toml` pinning the exact nightly version so all contributors use the same one
- Install `bpf-linker` (the BPF bytecode linker that Aya uses, not LLVM directly)
- Create workspace `Cargo.toml` with all 6 member crates
- Create `driftmap-probe-common` with the `PacketEvent` struct (the first real code)
- Create `driftmap-probe` scaffold with an empty TC hook that compiles to BPF target
- Create `.cargo/config.toml` with target triple override for the probe crate
- Set up GitHub repository, branch protection rules, issue templates
- Set up GitHub Actions `ci.yml`: `cargo check`, `cargo test`, `cargo clippy`, `cargo fmt --check`
- Verify: `cargo build -p driftmap-probe --target bpfel-unknown-none` produces a `.o` file

Why a full week: the BPF toolchain has sharp edges. Build target configuration in a
workspace with mixed targets (one BPF crate, five normal crates) requires non-obvious
Cargo config. Debugging a build failure that says "undefined reference to `malloc`" when
compiling to BPF takes time if you've never done it before.

Estimated files touched: 8 files, ~200 lines total (mostly config and scaffolding)

---

**Week 2 — eBPF TC Hook (First Real eBPF Code)**

Write the actual TC hook program that attaches to a network interface.

Tasks:
- Write the TC hook in `driftmap-probe/src/main.rs`:
  - Ethernet header parsing
  - IP header parsing (IPv4 only)
  - TCP header parsing
  - Port filtering against BPF hash map
  - Payload extraction (bounded at 1,500 bytes)
  - Ring buffer write
- Write the userspace ring buffer reader in `driftmap-core/src/capture.rs`:
  - Load the embedded BPF object file
  - Create the BPF maps
  - Populate watched ports map
  - Attach TC hook to interface specified in config
  - Spawn Tokio task that reads PacketEvents from ring buffer
  - For now: print raw PacketEvents to stdout (hex dump)
- Write a simple test harness: a bash script that uses `curl` to send HTTP requests
  to localhost and verifies DriftMap prints the captured bytes

Verify: run `sudo ./driftmap watch --interface lo --port 8080` and see `curl localhost:8080`
packets appear in the output as hex dumps.

Why `sudo`: eBPF programs require elevated privileges. CAP_NET_ADMIN and CAP_BPF
capabilities are needed for TC hook attachment. Running as root is acceptable for MVP.
Phase 5 will investigate rootless alternatives.

Estimated files touched: 3 files (`main.rs` in probe, `capture.rs` in core, CI config)
~250 new lines

---

**Week 3 — TCP Reassembler**

Turn raw packet bytes into complete HTTP messages.

Tasks:
- Implement `StreamKey` struct and `HashMap<StreamKey, StreamBuffer>` in `capture.rs`
- Implement the `ingest()` function: buffer bytes per TCP connection
- Implement `try_extract_message()`: scan for `\r\n\r\n`, read Content-Length, emit complete messages
- Implement GC: background task that removes stale stream buffers every 1 second
- Implement backpressure: `try_send` to downstream channel, drop on full, increment counter
- Add direction detection: distinguish requests (client→target) from responses (target→client)
- Write unit tests: feed synthetic TCP segment sequences, verify correct message reassembly
  including: multi-segment messages, out-of-order segments (not supported in MVP — just logged),
  partial body (message spans two drain cycles), chunked transfer encoding detection

Test cases to cover:
- Single-segment small HTTP request: trivial
- 3-segment HTTP request: verify correct assembly
- Connection dies mid-message: verify GC cleans up correctly
- Two interleaved connections on the same interface: verify no cross-contamination

Estimated files: primarily `capture.rs`, plus test fixtures
~200 new lines in `capture.rs`, ~80 lines of tests

---

**Week 4 — HTTP Parser + Phase 0 Integration**

Parse reassembled bytes into structured types, and wire everything together end-to-end.

Tasks:
- Implement `parse_request()` in `http.rs` using `httparse` for headers
- Implement `parse_response()` in `http.rs` using `httparse`
- Implement `templatize_path()` with numeric/UUID/hex detection
- Implement direction-aware parsing (requests go to one channel, responses to another)
- Wire the full pipeline: eBPF → reassembler → parser → stdout
- Write unit tests for the parser:
  - Standard GET request with no body
  - POST request with JSON body
  - Response with chunked Transfer-Encoding
  - Response with very large headers (64 headers)
  - Malformed HTTP (should return `None`, not panic)
  - Path templatization: 20 test cases covering numeric IDs, UUIDs, hex strings, normal segments
- Write the initial `driftmap.toml` config schema and loader (`config.rs`)
- Write the initial `driftmap-cli/src/main.rs` with the `watch` subcommand
- Update `README.md` with: what DriftMap does, how to build it, how to run Phase 0

Phase 0 exit demo: run DriftMap pointing at a local nginx, make HTTP requests via curl,
see parsed HTTP requests and responses printed to stdout with method, path, status code, headers.

Estimated files: `http.rs` (~180 lines new), `config.rs` (~80 lines), `main.rs` (~60 lines),
plus tests (~100 lines)

---

### Phase 1: Core Engine MVP (Weeks 5–8)

**Week 5 — Request Matcher**

Tasks:
- Implement `PendingRequest` and `MatchedPair` types in `matcher.rs`
- Implement the two pending queues (`HashMap<String, VecDeque<PendingRequest>>`)
- Implement `ingest()`: pair incoming requests using time-window matching
- Implement `gc()`: expire pending requests older than the window
- Spawn the matcher as a Tokio task consuming from the parser's output channels
- Unit tests:
  - Perfect synchronization: A request arrives 1ms before B request → should match
  - Outside window: A request arrives 600ms before B request (default 500ms window) → no match
  - Multiple pending: 3 GET /users/:id requests pending from A, one arrives from B → FIFO match
  - Unmatched request tracking: verify counter increments correctly
- Integration test: spin up two simple HTTP servers, send identical requests to both,
  verify DriftMap produces MatchedPairs

Estimated: ~160 lines in `matcher.rs`, ~80 lines of tests

---

**Week 6 — Raw Diff Engine**

Tasks:
- Implement `compute_raw_diff()` in `diff.rs`
- Implement header normalization (lowercase) and set operations (only_a, only_b, value_diff)
- Implement the built-in header exclusion list (date, x-request-id, etc.)
- Implement body byte comparison
- Implement latency delta calculation
- Connect diff engine to matcher output
- For now: print diffs to stdout in a human-readable format
- Unit tests:
  - Identical responses → RawDiff with all-zero counts, body_identical=true
  - Different status codes → status_match=false
  - Different headers → correct categorization into only_a/only_b/value_diff
  - Date header excluded → not in diff output
  - Bodies differ → body_identical=false, correct lengths

Estimated: ~140 lines in `diff.rs`, ~60 lines of tests

---

**Week 7 — Scorer + State Machine**

Tasks:
- Implement the rolling window of RawDiffs in `scorer.rs`
- Implement the weighting formula: status × 0.40 + schema × 0.30 + latency × 0.20 + header × 0.10
- Implement the state machine in `state.rs` with four states and hysteresis
- Implement `watch::channel` to broadcast scores to downstream consumers
- Write a minimal stdout reporter: every 5 seconds, print a table of endpoints and scores
  (this is what the TUI will replace in Phase 3)
- Unit tests for scorer:
  - All-matching pairs → score 0.0
  - All-mismatching status codes → score ~0.40
  - Mixed history → score proportional to mismatch fraction
- Unit tests for state machine:
  - Score stays below threshold → no transition
  - Score crosses threshold but not for 30s → no transition
  - Score crosses threshold for 30s → transition fires
  - Score drops back before 30s → clock resets, no transition

Estimated: ~130 lines in `scorer.rs`, ~100 lines in `state.rs`, ~120 lines of tests

---

**Week 8 — Phase 1 Integration + Demo**

Tasks:
- Wire the complete pipeline end-to-end
- Set up SQLite store (`store.rs`) for state persistence (basic schema, no pruning yet)
- Write integration tests using `tokio::time::pause()` to fast-forward time
  and verify state machine behavior under controlled conditions
- Set up a Phase 1 demo: two local Docker containers running different versions of a
  simple JSON API (one returns `{"status":"ok"}`, other returns `{"status":"ok","version":"2"}`)
- Run DriftMap against them, verify schema divergence is detected and scored correctly
- Record a demo GIF for the README
- Write a "How It Works" section in the README
- Tag `v0.1.0-alpha` release on GitHub

Phase 1 exit: DriftMap detects behavioral differences between two real HTTP services
without any code changes to those services.

Estimated: ~150 lines in `store.rs`, ~80 lines of integration tests

---

### Phase 2: Semantic Layer (Weeks 9–12)

**Week 9 — t-Digest Implementation**

Tasks:
- Implement `Centroid` struct and `TDigest` struct in `distribution.rs`
- Implement `add()`: insert value into sorted centroid list
- Implement `compress()`: merge centroids using t-digest scaling function
- Implement `quantile()`: walk centroids to answer p50/p95/p99 queries
- Implement `FieldDistribution`: wraps two TDigests (one per target), exposes `divergence_score()`
- Connect latency tracking: record per-pair latency in the scorer, feed into FieldDistribution
- Unit tests:
  - Add 10,000 uniform random values → check p50 ≈ 0.5, p95 ≈ 0.95 (within 2%)
  - Add 10,000 exponential values → check p99 is in the right order of magnitude
  - Two identical distributions → divergence_score() ≈ 0.0
  - One distribution with high p95 outliers → divergence_score() > 0.5

Estimated: ~170 lines in `distribution.rs`, ~60 lines of tests

---

**Week 10 — Schema Inference Engine**

Tasks:
- Implement `SchemaNode`, `FieldStats`, `FieldType` in `schema.rs`
- Implement `observe()`: update schema from one JSON response body
- Implement `infer_type()`: map `serde_json::Value` variants to `FieldType`
- Implement nested object recursion (capped at 5 levels)
- Implement `diff()`: compare two schema nodes, produce `SchemaDiff`
- Connect schema inferrer to the matched pairs stream
- Unit tests:
  - 100 identical responses → schema with all fields required, correct types
  - 100 responses where one field appears 60% of the time → field marked optional
  - Schema A has field `items`, schema B doesn't → SchemaDiff shows field_only_a: ["items"]
  - Field is Integer in A, String in B → SchemaDiff shows type_mismatch

Estimated: ~250 lines in `schema.rs`, ~80 lines of tests

---

**Week 11 — Equivalence Rules + Semantic JSON Normalization**

Tasks:
- Implement JSON body normalization before byte comparison:
  - Sort object keys recursively
  - Normalize ISO 8601 timestamps to UTC epoch (if field name contains "at", "time", "date")
  - Strip fields listed in equivalence_rules config
- Implement equivalence rules loading from `driftmap.toml` config
- Implement the `[ignore]` config section: excluded headers, excluded endpoints
- Add the `min_samples` warmup period to schema inferrer
- Update scorer to use `SchemaDiff` from schema inferrer as the `schema_score` input
- Update scorer to use `FieldDistribution.divergence_score()` as the `latency_score` input
- Integration test: two services that return the same data with different key ordering
  and timestamps → DriftMap should score them as EQUIVALENT after normalization

Estimated: ~100 new lines spread across `diff.rs`, `schema.rs`, `scorer.rs`, `config.rs`

---

**Week 12 — Phase 2 Hardening + Body Storage**

Tasks:
- Implement diverging pair storage in SQLite `diverging_pairs` table
- Implement `driftmap diff <endpoint> --last N` subcommand that queries SQLite and prints
  side-by-side colored diff using `similar` crate (Rust diff library)
- Implement SQLite pruning: delete drift_scores older than 24h, keep only last 1000 pairs per endpoint
- Implement WAL mode and synchronous=NORMAL pragma for SQLite
- Performance profiling: use `flamegraph` to identify hot paths in the schema inference
  and scoring pipeline. Document findings
- Fix any performance issues found in profiling
- Update integration tests to cover the full Phase 2 pipeline

**New package introduced: `similar`** — a Rust diff library that produces unified diff
output (the familiar `+`/`-` line format). Used only in the `driftmap diff` command output,
not in the hot path. Small crate, no significant dependencies.

Estimated: ~100 new lines in `store.rs`, ~50 lines in `main.rs` (new subcommand)

---

### Phase 3: Dashboard & Developer UX (Weeks 13–16)

**Week 13 — TUI Foundation**

Tasks:
- Add `ratatui` and `crossterm` dependencies to `driftmap-tui/Cargo.toml`
- Implement terminal setup in `main.rs`: enable raw mode, alternate screen, cleanup on exit
- Implement the `App` struct in `app.rs` with all state fields
- Implement the Tokio event loop: `select!` over keyboard events, score updates, tick timer
- Implement a basic two-panel layout in `ui.rs` using `ratatui::layout::Layout`
- Left panel: a static list of endpoint names (hardcoded, not yet from real scores)
- Right panel: a static text block
- Keyboard: `q` to quit, `j`/`k` to navigate

Verify: TUI renders without crashing, keyboard navigation works, terminal is restored
cleanly on exit (no leftover alternate screen or raw mode artifacts)

Estimated: ~80 lines in `main.rs`, ~50 lines in `app.rs`, ~60 lines in `ui.rs`, ~30 lines in `events.rs`

---

**Week 14 — TUI Data Integration**

Tasks:
- Connect the `watch::Receiver<Vec<DriftScore>>` from the scorer to the App struct
- Render real endpoint list with color-coded scores (green/yellow/red based on score)
- Implement sorting (by score, by name, by request count) with `s`/`n`/`r` keys
- Implement filter mode: `f` key enters filter, typing narrows the list, `Esc` clears
- Load historical scores from SQLite for sparkline data
- Render the sparkline in the right panel using `ratatui::widgets::Sparkline`
- Render score breakdown (status/schema/latency/header sub-scores) in right panel
- Implement the status bar with endpoint counts by state

Estimated: primarily additions to `app.rs` (~70 more lines) and `ui.rs` (~120 more lines)

---

**Week 15 — Diverging Pair Diff View + Polish**

Tasks:
- Implement the "last diverging pair" section in the TUI detail panel
- Load the most recent diverging pair from SQLite for the selected endpoint
- Render side-by-side diff using `similar` crate output formatted with color
- Handle the case where no diverging pairs are recorded yet (show "no divergence recorded")
- Implement `?` help overlay: floating panel listing all keyboard shortcuts
- Implement clean shutdown: `q` sends shutdown signal down the pipeline, tasks drain
  their channels and exit cleanly before process exits
- Polish: fix any layout overflow issues (terminal too small), add padding, fix color
  inconsistencies
- User test: have 3 developers use the TUI against real services and collect feedback

Estimated: ~80 new lines in `ui.rs`, ~40 lines in `app.rs`

---

**Week 16 — `driftmap init` + Config Hot-Reload**

Tasks:
- Implement `driftmap init` interactive wizard using `dialoguer` crate (terminal prompts)
  - "What interface should DriftMap listen on?" (lists available interfaces)
  - "What is Target A's address and port?"
  - "What is Target B's address and port?"
  - "What sampling rate? (1%, 10%, 100%)"
  - Writes `driftmap.toml` to current directory
- Implement config hot-reload using `notify` crate (filesystem watcher)
  - Watch `driftmap.toml` for modifications
  - Re-parse on change
  - Apply non-breaking changes live (thresholds, ignore lists, equivalence rules)
  - For breaking changes (watched ports, interface), log a warning and advise restart
- Record Phase 3 demo video for README
- Tag `v0.3.0-alpha`

**New packages introduced:**
- `dialoguer` — interactive terminal prompts (yes/no, text input, select lists)
- `notify` — cross-platform filesystem change notification

Estimated: ~60 lines in `main.rs` (new init subcommand), ~80 lines in `config.rs` (hot-reload logic)

---

### Phase 4: Plugin System & Extensibility (Weeks 17–20)

**Week 17 — Plugin SDK + WASM Host**

Tasks:
- Create `driftmap-plugin-sdk` crate with `Request`, `Response`, `PluginScore` types and `export_plugin!` macro
- Implement WASM plugin host in `driftmap-core/src/plugins.rs`:
  - `Engine` and `Module` compilation at startup
  - `Store` creation per plugin instance
  - `Linker` with no host imports (full sandboxing)
  - `Instance` creation and function export verification
- Implement the memory-passing protocol: `alloc`, write bytes, call `score_pair`, read result
- Write an example plugin in `examples/simple-plugin/`: always returns 0.0 (identity scorer)
  as a "hello world" to verify the plugin loading pipeline works
- Integrate plugin host into scorer: call applicable plugins after computing own score

Estimated: ~80 lines in plugin SDK, ~180 lines in `plugins.rs`

---

**Week 18 — gRPC Plugin**

Tasks:
- Write the `plugins/grpc-scorer/` plugin that handles gRPC traffic
- gRPC over HTTP/2 uses binary protobuf encoding. Without a `.proto` file, decoding is
  impossible with full fidelity. The gRPC plugin uses "best-effort" decoding:
  - Parse the gRPC framing (5-byte header: compression flag + message length)
  - If protobuf reflection API is available (some gRPC servers expose it), use it
  - Otherwise, compare binary bodies directly (any byte difference = score 1.0)
- Add HTTP/2 detection to the HTTP parser (the first bytes of HTTP/2 are `PRI * HTTP/2.0\r\n`)
- For HTTP/2 connections: capture at the frame level rather than message level (different
  reassembly logic — HTTP/2 is multiplexed, multiple streams over one TCP connection)
- Document the HTTP/2 limitation clearly: Phase 4 ships basic gRPC support, full
  HTTP/2 multiplexing support is Phase 5 or later

This is the hardest feature in Phase 4. HTTP/2 multiplexing (STREAM_ID per logical request,
frames interleaved) makes TCP stream reassembly significantly more complex. The HTTP/2
connection preface parsing alone is ~100 lines.

Estimated: ~200 lines in the gRPC plugin, ~100 new lines in `http.rs` for HTTP/2 detection

---

**Week 19 — Prometheus + OpenTelemetry Export**

Tasks:
- Implement `/metrics` HTTP server in `export.rs` using `axum`
- Implement Prometheus exposition format rendering (text format, not protobuf)
- Test with actual Prometheus scraping and Grafana dashboard
- Implement NDJSON export (for Datadog/Loki/Fluentd log shippers)
- Implement webhook POST on state transitions
- Implement `driftmap export --format json` (one-shot current-state dump to stdout)
- Write example Grafana dashboard JSON (included in the repo under `deploy/grafana/`)
- Write example Prometheus alert rules (`deploy/prometheus/alerts.yml`)

Estimated: ~120 lines in `export.rs`, plus configuration/documentation files

---

**Week 20 — Mirror Mode + Phase 4 Integration**

Mirror mode: DriftMap acts as a transparent TCP proxy. Instead of deploying a sidecar
agent on each target host, one DriftMap instance sits in front of both targets and
duplicates traffic to both.

Tasks:
- Implement `driftmap proxy --listen :8080 --target-a :3000 --target-b :3001`
- Accept incoming TCP connections on the listen port
- For each connection: open two outgoing connections (to A and to B)
- Forward all client bytes to both A and B
- Collect responses from both A and B
- Return A's response to the client (B's response is discarded — it's only used for comparison)
- The comparison side: feed both request and both responses into the DriftMap pipeline as if
  they came from the eBPF probe
- This mode does NOT require eBPF or root privileges — uses only userspace TCP
- Trade-off: mirror mode adds latency (the client waits for A's response, which goes through
  an extra TCP hop) and roughly doubles network bandwidth for watched traffic

Mirror mode implementation uses `tokio::net::TcpListener` and `tokio::net::TcpStream` —
entirely in the Tokio async networking layer.

Estimated: ~150 new lines in `main.rs` and a new `proxy.rs` file in `driftmap-cli`

---

### Phase 5: Hardening & Ecosystem (Weeks 21–26)

**Week 21 — Performance Benchmarking + Profiling**

Tasks:
- Write benchmark harness using `criterion` crate:
  - Benchmark HTTP parser: 1,000 requests/second, 10,000 requests/second
  - Benchmark TCP reassembler: 3 segments per message at 30,000 segments/second
  - Benchmark schema inference: 100 observations per endpoint across 100 endpoints
  - Benchmark t-digest: 1,000 adds per second per digest across 1,000 digests
  - Benchmark scorer: compute scores for 100 endpoints per second
- Run all benchmarks on target hardware (a typical 4-core cloud VM)
- Generate flamegraphs with `cargo-flamegraph` to identify hot functions
- Target: < 1% CPU overhead at 10,000 requests/second on one 4-core VM
- Fix any hot paths identified in profiling (likely: HashMap lookups, Vec allocations,
  schema inference on high-cardinality fields)
- Document performance findings in `docs/src/performance.md`

**New package introduced: `criterion`** — Rust benchmarking framework

Estimated: ~200 lines of benchmark code, documentation changes

---

**Week 22 — Security Hardening**

Tasks:
- Implement sensitive field redaction: `redact_fields = ["authorization", "x-api-key"]`
  in config causes those header values to be replaced with `[REDACTED]` before any
  comparison or storage
- Implement privilege dropping: after eBPF attachment (which requires CAP_NET_ADMIN
  and CAP_BPF), drop to a less-privileged user if configured (`run_as = "driftmap"`)
- Implement rootless mirror mode: mirror mode doesn't need eBPF at all, so it should
  work as a normal unprivileged user
- Write `SECURITY.md` with responsible disclosure policy
- Audit all SQLite queries for SQL injection (should be none since we use parameterized
  queries via `rusqlite`, but worth auditing explicitly)
- Run `cargo-deny` check for known CVEs in all dependencies
- Run `cargo-geiger` to audit all `unsafe` blocks (should be limited to eBPF probe and
  the WASM memory-passing code)
- Run `cargo audit` and address any advisories

Estimated: ~80 lines spread across `capture.rs`, `store.rs`, `main.rs`; documentation files

---

**Week 23 — Fuzz Testing + Edge Cases**

Tasks:
- Write fuzz targets using `cargo-fuzz` (which uses libFuzzer):
  - `fuzz_http_parser`: feed arbitrary bytes to the HTTP parser, verify no panics
  - `fuzz_schema_infer`: feed arbitrary JSON bytes to schema inferrer, verify no panics
  - `fuzz_reassembler`: feed arbitrary packet sequences to reassembler, verify no panics
- Run fuzzer for 24 hours against each target
- Fix any panics or unwanted behaviors discovered by fuzzer
- Write property-based tests using `proptest` crate:
  - "Parsing and re-serializing an HTTP message produces the same message"
  - "Schema inferred from N identical responses has all fields marked as required"
  - "t-digest quantile is always monotonically non-decreasing (q=0.9 >= q=0.5)"

**New packages introduced:**
- `cargo-fuzz` — coverage-guided fuzzing for Rust
- `proptest` — property-based testing

Estimated: ~100 lines of fuzz targets, ~100 lines of property tests

---

**Week 24 — Packaging & Distribution**

Tasks:
- Set up GitHub Actions `release.yml`:
  - Triggers on `v*` tag push
  - Builds for `x86_64-unknown-linux-gnu` and `aarch64-unknown-linux-gnu`
  - Uses `cross` tool for cross-compilation (Docker-based cross-compiler)
  - Uploads binaries to GitHub Releases with checksum files (`sha256sum`)
- Write `Dockerfile` using multi-stage build:
  - Stage 1: build image (Rust nightly + bpf-linker)
  - Stage 2: runtime image (distroless or debian-slim, just the binary + eBPF object)
- Write Helm chart for Kubernetes sidecar deployment:
  - DaemonSet (one DriftMap pod per node)
  - ConfigMap for `driftmap.toml`
  - ServiceAccount with minimal permissions
  - Optional Prometheus ServiceMonitor
- Write `deploy/systemd/driftmap.service` for non-Kubernetes deployments
- Test Docker image builds and Helm chart installs

Estimated: Helm chart (~100 lines YAML), Dockerfile (~40 lines), GitHub Actions (~60 lines YAML)

---

**Week 25 — Documentation Site**

Tasks:
- Set up `mdBook` and configure GitHub Pages deployment via `docs.yml` GitHub Action
- Write all documentation sections:
  - **Getting Started**: install binary, run against two local HTTP servers, see first output
    in under 5 minutes. No assumptions about eBPF knowledge
  - **How It Works**: pipeline diagram, explanation of each stage, what "semantic equivalence"
    means. Target audience: experienced developer new to DriftMap
  - **Configuration Reference**: every `driftmap.toml` field, its type, default value,
    and an example. Generated in part from the Config struct's doc comments
  - **Plugin Development Guide**: write a plugin, compile to WASM, load it, test it.
    Includes a worked example: a plugin that checks if a JSON field called `version`
    matches between both targets
  - **Deployment Patterns**: sidecar mode (two hosts), mirror mode (one host in front),
    Kubernetes DaemonSet, Docker Compose example
  - **Troubleshooting FAQ**: common issues (eBPF attachment fails → check kernel version,
    no pairs matching → check time window, schema inference shows wrong types → check
    Content-Type header)
- Set up search (mdBook has built-in search via JavaScript)
- Deploy to GitHub Pages

Estimated: ~2,000 words of documentation, mdBook configuration files

---

**Week 26 — v1.0 Stabilization + Launch**

Tasks:
- Audit all public API surfaces for stability. Define what is "stable" (won't change
  without major version bump): the config file format, the Prometheus metrics names,
  the plugin ABI, the CLI subcommand names and flags
- Write `GOVERNANCE.md`: how the project makes decisions, how to become a maintainer,
  the RFC process for significant changes
- Find and fix any remaining issues from user testing
- Write the v1.0 blog post (cross-posted to dev.to, Reddit r/rust, Hacker News)
- Tag `v1.0.0` release
- Create GitHub Discussions for Q&A and ideas
- Label 10 "good first issue" issues for new contributors with detailed descriptions
- Post to r/rust, HN, and relevant Slack/Discord communities

---

## 9. Line Count & Complexity Estimates

### Per-File Estimates

| File | Lines | Complexity | Primary Difficulty |
|------|-------|------------|-------------------|
| `driftmap-probe/src/main.rs` | 120 | High | BPF verifier constraints, kernel API |
| `driftmap-probe-common/src/lib.rs` | 30 | Low | Simple struct definition |
| `driftmap-core/src/capture.rs` | 200 | High | Stream reassembly, direction detection |
| `driftmap-core/src/http.rs` | 180 | Medium | Edge cases in HTTP parsing |
| `driftmap-core/src/matcher.rs` | 160 | Medium | FIFO pairing, time window logic |
| `driftmap-core/src/diff.rs` | 140 | Low | Set operations, exclusion lists |
| `driftmap-core/src/schema.rs` | 250 | High | Recursive inference, presence rates |
| `driftmap-core/src/distribution.rs` | 170 | High | t-digest math, compression algorithm |
| `driftmap-core/src/scorer.rs` | 130 | Medium | Rolling window, weighted formula |
| `driftmap-core/src/state.rs` | 100 | Medium | Hysteresis, transition events |
| `driftmap-core/src/store.rs` | 150 | Low | SQL queries, WAL config |
| `driftmap-core/src/export.rs` | 120 | Low | String formatting, HTTP server |
| `driftmap-core/src/plugins.rs` | 180 | High | WASM memory protocol, sandboxing |
| `driftmap-core/src/lib.rs` | 40 | Low | Public re-exports |
| `driftmap-tui/src/app.rs` | 120 | Medium | Async event loop, state management |
| `driftmap-tui/src/ui.rs` | 180 | Medium | Layout constraints, widget composition |
| `driftmap-tui/src/events.rs` | 50 | Low | Keyboard mapping |
| `driftmap-tui/src/main.rs` | 40 | Low | Setup and wiring |
| `driftmap-plugin-sdk/src/lib.rs` | 80 | Medium | ABI design, macro |
| `driftmap-cli/src/main.rs` | 120 | Low | Subcommand dispatch |
| `driftmap-cli/src/config.rs` | 80 | Low | Serde deserialization |
| **Total production code** | **~2,590** | | |
| Test files (unit + integration) | ~600 | | |
| Benchmark files | ~200 | | |
| Fuzz targets | ~100 | | |
| **Total all code** | **~3,490** | | |

### Why the Codebase Is Small

The brevity comes from three things:
1. Rust's type system eliminates error handling boilerplate (Result + ? operator)
2. Serde eliminates all serialization/deserialization code (derive macros)
3. Tokio + mpsc channels are the architecture — the pipeline structure means each file
   does exactly one thing, with no coordination logic bleeding between files

A comparable system in Python would be 3-4x the line count. In Go, roughly 2x.
In C, 5-10x (plus the parser security bugs).

---

## 10. Risk & Hard Problems

### Risk 1: BPF Verifier Rejection (High Probability, Medium Impact)

**The problem:** The Linux kernel's BPF verifier statically analyzes every BPF program
before loading it. It rejects any program where it cannot prove memory safety. Common
reasons for rejection: array access without explicit bounds check visible to the verifier
(even if logically impossible to be out of bounds), loops where iteration count isn't
provably bounded, stack frames larger than 512 bytes (the BPF stack limit).

**The payload capture code is particularly at risk:** `ctx.data() + payload_offset` must
be bounds-checked against `ctx.data_end()` explicitly. A missing bounds check causes
the verifier to reject the entire program with an error like "invalid access to packet,
off=X size=Y, R1=pkt(id=0,off=X,r=0), R2=pkt_end(id=0,off=0,r=0): R1 offset is outside
of the packet".

**Mitigation:** Study existing Aya examples closely before writing BPF code. The Aya
codebase has examples of correct packet access patterns. Expect 2-3 days of debugging
verifier rejections in Week 2. Document every verifier constraint encountered.

### Risk 2: Request Pairing False Matches (Medium Probability, High Impact)

**The problem:** If two services receive different traffic rates (e.g. Target A receives
500 req/s and Target B receives 550 req/s because of different routing), the FIFO matcher
will gradually accumulate mismatched pairs. `GET /users/:id` request #100 from A gets
paired with `GET /users/:id` request #103 from B — different users, different responses,
artificially high divergence score.

**Mitigation 1:** Use a short time window (500ms default). Mismatched pairing across
time windows is impossible.

**Mitigation 2:** Include request body hash in the matching key for non-GET requests.
Two `POST /api/orders` requests with different bodies are different logical operations.
The body hash makes them unmatchable.

**Mitigation 3:** Surface unmatched request rates in the TUI. High unmatched rates
indicate the two targets are receiving different traffic distributions — DriftMap should
warn about this rather than producing misleading divergence scores.

### Risk 3: Schema Inference Wrong Conclusions During Low Traffic (Medium Probability, Low Impact)

**The problem:** With `min_samples = 50`, schema inference doesn't activate for 50
requests. But on a low-traffic endpoint that receives 1 request/hour, 50 requests takes
50 hours. During this time, no schema diffs are shown even if they exist.

**Mitigation:** Make `min_samples` configurable per endpoint. For low-traffic endpoints,
users can set a lower threshold. Accept that schema inference is a eventually-consistent
signal, not a real-time one. The status code and header diffs still work immediately.

### Risk 4: HTTP/2 Multiplexing Complexity (High Probability on Modern Infrastructure)

**The problem:** HTTP/2 multiplexes multiple logical request/response streams over one
TCP connection. TCP stream reassembly as described in Feature 2 assumes one logical
HTTP conversation per TCP connection (HTTP/1.1 behavior). HTTP/2 breaks this assumption.

Specifically: HTTP/2 assigns a stream ID to each request. Multiple requests with
different stream IDs can be in flight simultaneously on one TCP connection. Frames
for different streams are interleaved. Reassembling HTTP/2 at the TCP level requires
HTTP/2 frame parsing (not just TCP segment reassembly) and per-stream state tracking.

**Mitigation:** MVP explicitly doesn't support HTTP/2. Detect HTTP/2 connections
(the `PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n` connection preface) and log a clear warning:
"HTTP/2 detected on connection — not yet supported. Switch to HTTP/1.1 or use mirror mode."
Mirror mode can handle HTTP/2 because Tokio's TcpStream works at the byte level and
both targets get the same bytes — no frame-level parsing needed.

### Risk 5: Memory Pressure at Scale (Low Probability, High Impact)

**The problem:** At 1,000 endpoints with 100 fields each, 2 targets, 1,000 pending
pairs per endpoint in the matching queues, and 200-centroid t-digests per field, the
total in-memory state is:
- Schema nodes: 1,000 × 2 × 100 fields × 40 bytes = 8MB
- t-Digests: 1,000 × 2 × 100 fields × 200 centroids × 12 bytes = 480MB
- Pending requests in matcher: 1,000 endpoints × 10 average pending × 2,000 bytes = 20MB
- Ring buffer: 4MB (fixed)
- Total: ~512MB peak

512MB is acceptable on any server (even a $5/month VPS has 1GB). But if deployed to a
constrained environment (embedded system, very small container), this could be an issue.

**Mitigation:** Expose `max_endpoints` config (default 1,000). Endpoints beyond the limit
are tracked only at the status-code level (no schema inference, no distribution tracking).
Also expose `distribution_fields_per_endpoint` (default 100) to reduce t-digest count.

---

*This document is the complete engineering specification for DriftMap v1.0.
Total scope: 26 weeks, ~3,500 lines of code across 21 production files,
6 Rust crates, 19 external packages, 8 components built from scratch.*

*Last updated: April 2026*
