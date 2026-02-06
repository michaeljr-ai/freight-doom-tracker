# =============================================================
#  FREIGHT DOOM TRACKER - Unified Dockerfile
#  Runs Redis + Rust Engine + Rails in ONE container
#  Optimized for Railway.app free tier (single service)
# =============================================================

# ---- Stage 1: Build Rust Engine ----
FROM rust:latest AS rust-builder

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /rust-app
COPY freight_doom_engine/Cargo.toml freight_doom_engine/Cargo.lock ./
COPY freight_doom_engine/src/ src/

RUN cargo build --release

# ---- Stage 2: Build Rails App ----
FROM ruby:3.2-slim-bookworm AS rails-builder

RUN apt-get update && apt-get install -y \
    build-essential \
    libpq-dev \
    libsqlite3-dev \
    git \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /rails-app
COPY rails_app/Gemfile rails_app/Gemfile.lock ./
RUN bundle install --without development test --jobs 4 --retry 3

COPY rails_app/ .

# ---- Stage 3: Final Runtime Image ----
FROM ruby:3.2-slim-bookworm

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    libpq5 \
    libsqlite3-0 \
    redis-server \
    curl \
    nodejs \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy Rust binary
COPY --from=rust-builder /rust-app/target/release/freight_doom_engine /usr/local/bin/freight_doom_engine

# Copy Rails app with bundled gems
COPY --from=rails-builder /usr/local/bundle /usr/local/bundle
COPY --from=rails-builder /rails-app /app/rails_app

# Copy startup script
COPY start-railway.sh /app/start-railway.sh
RUN chmod +x /app/start-railway.sh

ENV RAILS_ENV=production
ENV RAILS_LOG_TO_STDOUT=true
ENV RUST_LOG=info
ENV FREIGHT_DOOM_REDIS_URL=redis://localhost:6379
ENV FREIGHT_DOOM_REDIS_CHANNEL=bankruptcy:events
ENV FREIGHT_DOOM_MIN_CONFIDENCE=0.3

EXPOSE 3000 9090

CMD ["/app/start-railway.sh"]
