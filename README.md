```
 _____ ____  _____ ___ ____ _   _ _____   ____   ___   ___  __  __
|  ___|  _ \| ____|_ _/ ___| | | |_   _| |  _ \ / _ \ / _ \|  \/  |
| |_  | |_) |  _|  | | |  _| |_| | | |   | | | | | | | | | | |\/| |
|  _| |  _ <| |___ | | |_| |  _  | | |   | |_| | |_| | |_| | |  | |
|_|   |_| \_\_____|___\____|_| |_| |_|   |____/ \___/ \___/|_|  |_|

 _____ ____      _    ____ _  _______ ____
|_   _|  _ \    / \  / ___| |/ / ____|  _ \
  | | | |_) |  / _ \| |   | ' /|  _| | |_) |
  | | |  _ <  / ___ \ |___| . \| |___|  _ <
  |_| |_| \_\/_/   \_\____|_|\_\_____|_| \_\
```

# FREIGHT DOOM TRACKER

### The Nuclear Option for Logistics Bankruptcy Intelligence

---

> **"When the freight industry collapses, we don't just watch. We track every last tremor with military-grade precision."**

---

## WHAT IS THIS MONSTROSITY?

Freight Doom Tracker is a **weapons-grade, absurdly over-engineered** real-time bankruptcy and distress monitoring system for the freight and logistics industry. It combines a **Rust-powered scanning engine** with a **Rails command center dashboard** to deliver sub-second alerting on carrier failures, broker collapses, and supply chain implosions.

While a simple RSS feed reader would have sufficed, we instead built a **distributed event processing pipeline** with bloom filters, SIMD-accelerated text parsing, WebSocket push notifications, and a CSS theme that looks like you're commanding a nuclear submarine.

**This is not a toy. This is not reasonable. This is FREIGHT DOOM.**

---

## ARCHITECTURE

```
                          +---------------------------+
                          |    FREIGHT DOOM TRACKER    |
                          |   "The War Room Dashboard" |
                          +---------------------------+
                                      |
                    +-----------------+-----------------+
                    |                                   |
          +---------v----------+            +-----------v-----------+
          |   RAILS FRONTEND   |            |   RUST DOOM ENGINE    |
          |   (Command Center) |<---------->|   (The Beast Core)    |
          +--------------------+  WebSocket +----------+------------+
          | - Turbo Streams    |  + Redis   |          |
          | - Stimulus Ctrls   |  PubSub    |   +------v------+
          | - Action Cable     |            |   | SIMD Parser |
          | - Dark CRT Theme   |            |   +------+------+
          | - Live Counters    |            |          |
          +--------+-----------+            |   +------v------+
                   |                        |   | Bloom Filter|
                   |                        |   | (Dedup)     |
                   v                        |   +------+------+
          +--------+-----------+            |          |
          |      PUMA          |            |   +------v------+
          |  (Web Server)      |            |   | Confidence  |
          +--------------------+            |   | Scoring     |
                                            |   +------+------+
                                            |          |
                                            +----------+
                                                       |
                          +----------------------------+----+
                          |        DATA SOURCES              |
                          +----------------------------------+
                          |                                  |
            +-------------v--+  +----------v--+  +----------v--+  +--------v------+
            |    PACER       |  |   EDGAR     |  |   FMCSA     |  | CourtListener |
            | (Federal Court |  | (SEC Filings|  | (DOT Motor  |  | (Free Legal   |
            |  Records)      |  |  Database)  |  |  Carrier    |  |  Opinions &   |
            +----------------+  +-------------+  |  Safety     |  |  Filings)     |
                                                  |  Admin)     |  +---------------+
                                                  +-------------+
                          |                                  |
                          +----------------------------------+
                          |       REDIS (Event Bus)          |
                          |  - PubSub Channels               |
                          |  - Sorted Sets (Timeline)        |
                          |  - Streams (Event Log)            |
                          +----------------------------------+
```

---

## TECH STACK (YES, ALL OF IT)

| Layer | Technology | Why? |
|-------|-----------|------|
| **Scanning Engine** | Rust (tokio, reqwest, scraper) | Because C was too easy and Go wasn't painful enough |
| **Web Framework** | Ruby on Rails 7+ | Convention over configuration, baby |
| **Real-time** | Action Cable + Turbo Streams | WebSocket push without the JavaScript framework circus |
| **Frontend** | Stimulus + Turbo | The Basecamp way: HTML-over-the-wire |
| **Cache/PubSub** | Redis | The duct tape holding everything together |
| **Database** | SQLite (dev) / PostgreSQL (prod) | Yes, SQLite can handle your bankruptcy data |
| **Deduplication** | Bloom Filters (Rust) | Probabilistic data structures for 0.01% false positive rate |
| **Text Parsing** | SIMD-accelerated (Rust) | Because regex wasn't fast enough (it was) |
| **Process Mgmt** | Foreman / Procfile | One command to rule them all |
| **Containerization** | Docker Compose | For when Replit isn't overkill enough |
| **CSS Vibes** | Custom dark industrial theme | CRT scanlines, glowing red alerts, the whole war room |
| **Charts** | Chartkick + Chart.js | Sparklines for that Bloomberg terminal aesthetic |
| **Deployment** | Replit / Docker / Bare metal | Deploy anywhere. Fear is universal. |

---

## DATA SOURCES

### PACER (Public Access to Court Electronic Records)
Federal bankruptcy court filings. Chapter 7, Chapter 11, Chapter 13. The motherlode of carrier doom.

### EDGAR (SEC Electronic Data Gathering)
Public company filings. 10-K, 10-Q, 8-K filings that mention "going concern," "liquidity crisis," or "material adverse events." When publicly traded logistics companies start sweating.

### FMCSA (Federal Motor Carrier Safety Administration)
Motor carrier registration data. License revocations, insurance lapses, safety violations. The canary in the coal mine for carrier failures.

### CourtListener (Free Law Project)
Open-access court opinions, filings, and docket data. Cross-reference bankruptcy filings with litigation patterns.

---

## SETUP (REPLIT)

### Quick Start
1. Fork this Repl
2. Click **Run**
3. Watch the doom unfold

### What Happens When You Click Run
1. Redis starts in the background
2. The Rust Doom Engine compiles (first run takes ~2 min)
3. Rails migrates the database
4. The Rust scanner begins crawling data sources
5. Rails serves the war room dashboard on port 3000
6. You stare at the screen like a bond villain

### Environment Variables
| Variable | Default | Description |
|----------|---------|-------------|
| `REDIS_URL` | `redis://localhost:6379` | Redis connection string |
| `RAILS_ENV` | `production` | Rails environment |
| `SECRET_KEY_BASE` | (set in .replit) | Rails secret key |
| `RUST_LOG` | `info` | Rust log level (trace/debug/info/warn/error) |
| `PACER_API_KEY` | (none) | PACER API credentials (optional) |
| `EDGAR_USER_AGENT` | (none) | SEC EDGAR requires a User-Agent |

---

## RUNNING LOCALLY

```bash
# Clone the repo
git clone <repo-url> freight-doom-tracker
cd freight-doom-tracker

# Option A: Docker Compose (recommended for the faint of heart)
docker-compose up --build

# Option B: Bare metal (for the brave)
# Install: Ruby 3.2+, Rust 1.70+, Redis, Node.js 20+, SQLite
bash start.sh

# Option C: Foreman
gem install foreman
foreman start
```

---

## PROJECT STRUCTURE

```
bankruptcy-tracker/
+-- README.md                  # You are here. Welcome to the abyss.
+-- .replit                    # Replit configuration
+-- replit.nix                 # Nix packages for Replit
+-- start.sh                   # Master startup script
+-- Procfile                   # Process manager config
+-- docker-compose.yml         # Container orchestration
+--
+-- freight_doom_engine/       # THE RUST BEAST
|   +-- Cargo.toml
|   +-- src/
|       +-- main.rs            # Entry point
|       +-- scanner.rs         # Multi-source scanner
|       +-- bloom.rs           # Bloom filter deduplication
|       +-- parser.rs          # SIMD text analysis
|       +-- scoring.rs         # Confidence scoring engine
|       +-- api.rs             # Metrics & health endpoint
|
+-- rails_app/                 # THE COMMAND CENTER
    +-- Gemfile
    +-- app/
    |   +-- assets/stylesheets/
    |   |   +-- application.css    # Dark industrial CRT theme
    |   +-- javascript/controllers/
    |   |   +-- application.js     # Stimulus setup
    |   |   +-- index.js           # Controller registration
    |   |   +-- live_counter_controller.js
    |   |   +-- event_feed_controller.js
    |   |   +-- stats_controller.js
    |   |   +-- system_health_controller.js
    |   +-- models/
    |   +-- views/
    |   +-- channels/
    +-- config/
        +-- importmap.rb
```

---

## WARNING

```
+============================================================+
|                                                              |
|   *** WEAPONS-GRADE OVERKILL WARNING ***                     |
|                                                              |
|   This application is DRAMATICALLY over-engineered.          |
|   A Google Sheet with RSS feeds would accomplish 90%         |
|   of what this does.                                         |
|                                                              |
|   But where's the fun in that?                               |
|                                                              |
|   This project exists at the intersection of:                |
|   - "I wanted to learn Rust"                                 |
|   - "WebSockets are cool"                                    |
|   - "What if Bloomberg Terminal but for truck bankruptcies"  |
|   - "I have mass skill and mass time"                        |
|                                                              |
|   Proceed with appropriate levels of awe and concern.        |
|                                                              |
+============================================================+
```

---

## LICENSE

MIT. Use it. Fork it. Deploy it at your logistics company and watch your coworkers question your sanity.

---

*Built with mass overkill by humans who should know better.*
*Powered by mass caffeine and mass hubris.*
