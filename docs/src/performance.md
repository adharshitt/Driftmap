# DriftMap Performance & Safety Baseline

## CPU & Memory Constraints
- **Target Overhead:** < 1% CPU utilization on a 4-core machine at 10,000 Requests/Second.
- **Memory Footprint:** < 50MB RSS in a single-instance sidecar deployment.

## Critical Benchmarks (via Criterion)
1. **HTTP/1.1 Parser:** Must reassemble and parse a standard 1.5KB request in under 5 microseconds.
2. **Schema Inferrer:** Must recursively extract nested JSON schemas (up to 5 levels) in under 20 microseconds per payload.
3. **Matcher Queue:** Garbage collection must clear 100k stale queue entries in under 1 millisecond to prevent HashDoS starvation.

## Security Controls Implemented
- **Bounded Buffer:** Reassembler streams cap out at 1MB per connection to prevent OOM.
- **Queue Constraints:** Bounded to 100 entries per unique path key to stop CPU queue thrashing.
- **WASM Safeties:** Wasmtime sandboxing prevents file/network access and bounds execution via 100,000 unit fuel timers.
