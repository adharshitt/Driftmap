# DriftMap — Runtime Semantic Diff for Live Systems

Watches two live environments simultaneously and surfaces *behavioral* divergence.

## Project Structure

- **driftmap-probe**: eBPF programs (kernel space)
- **driftmap-probe-common**: Shared types between eBPF and userspace
- **driftmap-core**: Userspace logic, reassembly, parsing, matching, scoring
- **driftmap-tui**: Terminal UI dashboard
- **driftmap-plugin-sdk**: SDK for WASM plugin authors
- **driftmap-cli**: Command-line entry point

See [ROADMAP.md](./ROADMAP.md) and [SPEC.md](./SPEC.md) for details.
