#!/usr/bin/env bash
set -e

# ============================================================
#  FREIGHT DOOM TRACKER - Master Startup Script
#  "When everything is on fire, at least we have metrics."
# ============================================================

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
BOLD='\033[1m'
DIM='\033[2m'
NC='\033[0m' # No Color

REDIS_PID=""
RUST_ENGINE_PID=""

# --- Cleanup on exit ---
cleanup() {
  echo ""
  echo -e "${RED}${BOLD}========================================${NC}"
  echo -e "${RED}${BOLD}  FREIGHT DOOM TRACKER - SHUTTING DOWN  ${NC}"
  echo -e "${RED}${BOLD}========================================${NC}"
  echo ""

  if [ -n "$RUST_ENGINE_PID" ] && kill -0 "$RUST_ENGINE_PID" 2>/dev/null; then
    echo -e "${YELLOW}[SHUTDOWN]${NC} Stopping Rust Doom Engine (PID: $RUST_ENGINE_PID)..."
    kill "$RUST_ENGINE_PID" 2>/dev/null || true
    wait "$RUST_ENGINE_PID" 2>/dev/null || true
    echo -e "${GREEN}[SHUTDOWN]${NC} Rust engine stopped."
  fi

  if [ -n "$REDIS_PID" ] && kill -0 "$REDIS_PID" 2>/dev/null; then
    echo -e "${YELLOW}[SHUTDOWN]${NC} Stopping Redis (PID: $REDIS_PID)..."
    kill "$REDIS_PID" 2>/dev/null || true
    wait "$REDIS_PID" 2>/dev/null || true
    echo -e "${GREEN}[SHUTDOWN]${NC} Redis stopped."
  fi

  echo ""
  echo -e "${DIM}All systems offline. The doom has been contained.${NC}"
  echo ""
  exit 0
}

trap cleanup SIGINT SIGTERM EXIT

# --- Print the glorious banner ---
print_banner() {
  echo ""
  echo -e "${RED}${BOLD}"
  cat << 'BANNER'
  _______ ____  _____ ___ ____ _   _ _____
 |  ___| _ \| ____|_ _/ ___| | | |_   _|
 | |_  | |_) |  _|  | | |  _| |_| | | |
 |  _| |  _ <| |___ | | |_| |  _  | | |
 |_|   |_| \_\_____|___\____|_| |_| |_|

  ____   ___   ___  __  __
 |  _ \ / _ \ / _ \|  \/  |
 | | | | | | | | | | |\/| |
 | |_| | |_| | |_| | |  | |
 |____/ \___/ \___/|_|  |_|

  _____ ____      _    ____ _  _______ ____
 |_   _|  _ \    / \  / ___| |/ / ____|  _ \
   | | | |_) |  / _ \| |   | ' /|  _| | |_) |
   | | |  _ <  / ___ \ |___| . \| |___|  _ <
   |_| |_| \_\/_/   \_\____|_|\_\_____|_| \_\

BANNER
  echo -e "${NC}"
  echo -e "${DIM}  Weapons-Grade Logistics Bankruptcy Intelligence${NC}"
  echo -e "${DIM}  ================================================${NC}"
  echo ""
}

print_banner

# ============================================================
# PHASE 1: Redis
# ============================================================
echo -e "${CYAN}${BOLD}[PHASE 1/5]${NC} Starting Redis event bus..."

if command -v redis-server &> /dev/null; then
  redis-server --daemonize no --port 6379 --loglevel warning &
  REDIS_PID=$!
  sleep 1

  if kill -0 "$REDIS_PID" 2>/dev/null; then
    echo -e "${GREEN}[OK]${NC} Redis online (PID: $REDIS_PID)"
  else
    echo -e "${RED}[FAIL]${NC} Redis failed to start!"
    exit 1
  fi
else
  echo -e "${YELLOW}[WARN]${NC} redis-server not found. Assuming external Redis at $REDIS_URL"
fi

# ============================================================
# PHASE 2: Build the Rust Doom Engine
# ============================================================
echo ""
echo -e "${CYAN}${BOLD}[PHASE 2/5]${NC} Compiling Rust Doom Engine..."
echo -e "${DIM}           (First build may take 2-3 minutes. Subsequent builds are cached.)${NC}"

if [ -d "freight_doom_engine" ] && [ -f "freight_doom_engine/Cargo.toml" ]; then
  cd freight_doom_engine
  cargo build --release 2>&1 | while IFS= read -r line; do
    echo -e "${DIM}  [cargo] ${line}${NC}"
  done
  CARGO_EXIT=${PIPESTATUS[0]}
  cd ..

  if [ "$CARGO_EXIT" -eq 0 ]; then
    echo -e "${GREEN}[OK]${NC} Rust Doom Engine compiled successfully."
  else
    echo -e "${RED}[FAIL]${NC} Rust compilation failed! Check errors above."
    echo -e "${YELLOW}[WARN]${NC} Continuing without the Rust engine..."
  fi
else
  echo -e "${YELLOW}[SKIP]${NC} freight_doom_engine/ not found or missing Cargo.toml."
  echo -e "${DIM}         The scanner will not run, but the dashboard will still work.${NC}"
fi

# ============================================================
# PHASE 3: Rails Database Migration
# ============================================================
echo ""
echo -e "${CYAN}${BOLD}[PHASE 3/5]${NC} Running Rails database migrations..."

if [ -d "rails_app" ]; then
  cd rails_app

  if [ -f "Gemfile" ]; then
    bundle install --quiet 2>/dev/null || echo -e "${YELLOW}[WARN]${NC} Bundle install had issues. Continuing..."
    bundle exec rails db:migrate 2>&1 | while IFS= read -r line; do
      echo -e "${DIM}  [rails] ${line}${NC}"
    done
    echo -e "${GREEN}[OK]${NC} Database migrations complete."
  else
    echo -e "${YELLOW}[SKIP]${NC} No Gemfile found. Skipping migrations."
  fi

  cd ..
else
  echo -e "${YELLOW}[SKIP]${NC} rails_app/ directory not found."
fi

# ============================================================
# PHASE 4: Start the Rust Doom Engine
# ============================================================
echo ""
echo -e "${CYAN}${BOLD}[PHASE 4/5]${NC} Launching Rust Doom Engine..."

if [ -f "freight_doom_engine/target/release/freight_doom_engine" ]; then
  RUST_LOG="${RUST_LOG:-info}" ./freight_doom_engine/target/release/freight_doom_engine &
  RUST_ENGINE_PID=$!
  sleep 1

  if kill -0 "$RUST_ENGINE_PID" 2>/dev/null; then
    echo -e "${GREEN}[OK]${NC} Rust Doom Engine online (PID: $RUST_ENGINE_PID)"
    echo -e "${DIM}         Metrics endpoint: http://localhost:9090/metrics${NC}"
    echo -e "${DIM}         Health endpoint:  http://localhost:9090/health${NC}"
  else
    echo -e "${YELLOW}[WARN]${NC} Rust engine exited unexpectedly. Dashboard will run without live scanning."
    RUST_ENGINE_PID=""
  fi
else
  echo -e "${YELLOW}[SKIP]${NC} Rust binary not found. Dashboard will run in demo mode."
fi

# ============================================================
# PHASE 5: Start Rails with Puma
# ============================================================
echo ""
echo -e "${CYAN}${BOLD}[PHASE 5/5]${NC} Starting Rails Command Center..."
echo ""
echo -e "${RED}${BOLD}  ========================================${NC}"
echo -e "${RED}${BOLD}  =    FREIGHT DOOM TRACKER IS LIVE      =${NC}"
echo -e "${RED}${BOLD}  =    Dashboard: http://0.0.0.0:3000    =${NC}"
echo -e "${RED}${BOLD}  ========================================${NC}"
echo ""
echo -e "${DIM}  Press Ctrl+C to initiate shutdown sequence.${NC}"
echo ""

if [ -d "rails_app" ] && [ -f "rails_app/Gemfile" ]; then
  cd rails_app
  exec bundle exec puma -C config/puma.rb -p 3000 -b "0.0.0.0"
else
  echo -e "${YELLOW}[WARN]${NC} Rails app not fully configured. Holding process open..."
  echo -e "${DIM}         Freight Doom Tracker is in standby mode.${NC}"
  # Keep the script running so Replit doesn't exit
  while true; do
    sleep 60
  done
fi
