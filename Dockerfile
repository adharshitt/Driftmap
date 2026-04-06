FROM rust:1.80 as builder
WORKDIR /usr/src/driftmap
COPY . .
RUN cargo build --release -p driftmap-cli

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y libssl3 ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/src/driftmap/target/release/driftmap /usr/local/bin/driftmap
ENTRYPOINT ["driftmap"]
