# Getting Started with DriftMap

DriftMap watches two live environments (e.g., Staging vs Production) simultaneously and surfaces *behavioral* divergence.

## Installation

### Using Docker (Recommended)
```bash
docker pull ghcr.io/adharshitt/driftmap:latest
docker run --cap-add=NET_ADMIN --cap-add=SYS_ADMIN --network=host driftmap:latest watch --target-a 10.0.0.1:80 --target-b 10.0.0.2:80
```

### From Source
```bash
cargo build --release
sudo ./target/release/driftmap watch --target-a 127.0.0.1:3000 --target-b 127.0.0.1:3001
```

## Running the Terminal UI

DriftMap automatically launches a rich Terminal UI (TUI) to visualize drift scores in real-time. Use `j`/`k` to navigate and `s`/`n`/`r` to sort by Score, Name, or Request volume.
