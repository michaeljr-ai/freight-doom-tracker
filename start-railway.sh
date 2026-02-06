#!/usr/bin/env bash
set -e

# =============================================================
#  FREIGHT DOOM TRACKER - Railway Startup
#  Runs all services in a single container for free tier
# =============================================================

RED='\033[0;31m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
BOLD='\033[1m'
DIM='\033[2m'
NC='\033[0m'

echo ""
echo -e "${RED}${BOLD}  FREIGHT DOOM TRACKER - BOOTING UP  ${NC}"
echo ""

# --- Phase 1: Start Redis (embedded) ---
echo -e "${CYAN}[1/4]${NC} Starting embedded Redis..."
redis-server --daemonize yes --port 6379 --loglevel warning --save "" --appendonly no
sleep 1

if redis-cli ping > /dev/null 2>&1; then
  echo -e "${GREEN}[OK]${NC} Redis online"
else
  echo -e "${RED}[WARN]${NC} Redis failed, continuing anyway..."
fi

# Set internal Redis URL for both services
export REDIS_URL="redis://localhost:6379/1"
export FREIGHT_DOOM_REDIS_URL="redis://localhost:6379"

# --- Phase 2: Run Rails migrations ---
echo -e "${CYAN}[2/4]${NC} Running database migrations..."
cd /app/rails_app
bundle exec rails db:migrate 2>&1 || echo "Migration note: may need DATABASE_URL"
cd /app

# --- Phase 3: Start Rust Engine (background) ---
echo -e "${CYAN}[3/4]${NC} Launching Rust Doom Engine..."
RUST_LOG="${RUST_LOG:-info}" freight_doom_engine &
RUST_PID=$!
sleep 1

if kill -0 "$RUST_PID" 2>/dev/null; then
  echo -e "${GREEN}[OK]${NC} Rust engine scanning (PID: $RUST_PID)"
else
  echo -e "${RED}[WARN]${NC} Rust engine exited. Dashboard runs without live scanning."
fi

# --- Phase 4: Start Rails (foreground) ---
echo ""
echo -e "${RED}${BOLD}  ======================================  ${NC}"
echo -e "${RED}${BOLD}    FREIGHT DOOM TRACKER IS LIVE         ${NC}"
echo -e "${RED}${BOLD}    Scanning: PACER + EDGAR + FMCSA      ${NC}"
echo -e "${RED}${BOLD}  ======================================  ${NC}"
echo ""

cd /app/rails_app
exec bundle exec puma -C config/puma.rb
