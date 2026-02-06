FROM rust:1.77-slim-bookworm AS builder

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src/ src/

RUN cargo build --release

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/freight_doom_engine /usr/local/bin/freight_doom_engine

ENV RUST_LOG=info
ENV FREIGHT_DOOM_REDIS_URL=redis://localhost:6379
ENV FREIGHT_DOOM_REDIS_CHANNEL=bankruptcy:events
ENV FREIGHT_DOOM_MIN_CONFIDENCE=0.3

EXPOSE 9090

CMD ["freight_doom_engine"]
