# ── Build stage ───────────────────────────────────────────────────────────────
FROM rust:1.85-slim AS builder

WORKDIR /app
COPY . .

# Build only the server binary
RUN cargo build --release -p sshx-server

# ── Runtime stage ─────────────────────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/sshx-server /usr/local/bin/sshx-server

# Control port
EXPOSE 7835
# Tunnel port range
EXPOSE 2000-9000

ENTRYPOINT ["sshx-server"]
