// ███████╗██████╗ ███████╗██╗ ██████╗ ██╗  ██╗████████╗
// ██╔════╝██╔══██╗██╔════╝██║██╔════╝ ██║  ██║╚══██╔══╝
// █████╗  ██████╔╝█████╗  ██║██║  ███╗███████║   ██║
// ██╔══╝  ██╔══██╗██╔══╝  ██║██║   ██║██╔══██║   ██║
// ██║     ██║  ██║███████╗██║╚██████╔╝██║  ██║   ██║
// ╚═╝     ╚═╝  ╚═╝╚══════╝╚═╝ ╚═════╝ ╚═╝  ╚═╝   ╚═╝
//
// ██████╗  ██████╗  ██████╗ ███╗   ███╗
// ██╔══██╗██╔═══██╗██╔═══██╗████╗ ████║
// ██║  ██║██║   ██║██║   ██║██╔████╔██║
// ██║  ██║██║   ██║██║   ██║██║╚██╔╝██║
// ██████╔╝╚██████╔╝╚██████╔╝██║ ╚═╝ ██║
// ╚═════╝  ╚═════╝  ╚═════╝ ╚═╝     ╚═╝
//
// E N G I N E
//
// The most overkill bankruptcy detection engine ever conceived.
// Rust + Tokio + Crossbeam + Bloom Filters + SIMD + Circuit Breakers
// All to detect when a trucking company files for Chapter 11.

mod config;
mod models;
mod scanners;
mod dedup;
mod circuit_breaker;
mod publisher;
mod text_scanner;
mod metrics;

use std::sync::Arc;
use tokio::sync::watch;
use tokio::signal;
use tracing::{info, warn, error};
use tracing_subscriber::{self, EnvFilter, fmt};

use crate::config::Config;
use crate::dedup::DedupEngine;
use crate::models::BankruptcyEvent;
use crate::publisher::RedisPublisher;
use crate::metrics::MetricsCollector;
use crate::scanners::{
    pacer_scanner,
    edgar_scanner,
    fmcsa_scanner,
    court_listener_scanner,
};

fn print_banner() {
    let banner = r#"

    ╔══════════════════════════════════════════════════════════════════╗
    ║                                                                  ║
    ║     ███████╗██████╗ ███████╗██╗ ██████╗ ██╗  ██╗████████╗       ║
    ║     ██╔════╝██╔══██╗██╔════╝██║██╔════╝ ██║  ██║╚══██╔══╝       ║
    ║     █████╗  ██████╔╝█████╗  ██║██║  ███╗███████║   ██║          ║
    ║     ██╔══╝  ██╔══██╗██╔══╝  ██║██║   ██║██╔══██║   ██║          ║
    ║     ██║     ██║  ██║███████╗██║╚██████╔╝██║  ██║   ██║          ║
    ║     ╚═╝     ╚═╝  ╚═╝╚══════╝╚═╝ ╚═════╝ ╚═╝  ╚═╝   ╚═╝          ║
    ║                                                                  ║
    ║           ██████╗  ██████╗  ██████╗ ███╗   ███╗                  ║
    ║           ██╔══██╗██╔═══██╗██╔═══██╗████╗ ████║                  ║
    ║           ██║  ██║██║   ██║██║   ██║██╔████╔██║                  ║
    ║           ██║  ██║██║   ██║██║   ██║██║╚██╔╝██║                  ║
    ║           ██████╔╝╚██████╔╝╚██████╔╝██║ ╚═╝ ██║                  ║
    ║           ╚═════╝  ╚═════╝  ╚═════╝ ╚═╝     ╚═╝                  ║
    ║                                                                  ║
    ║        ⚡ LOGISTICS BANKRUPTCY DETECTION ENGINE ⚡               ║
    ║                                                                  ║
    ║   Sources:  PACER | SEC EDGAR | FMCSA | CourtListener            ║
    ║   Dedup:    Bloom Filter + LRU Cache Hybrid                      ║
    ║   Speed:    SIMD-Accelerated Aho-Corasick Text Scanning          ║
    ║   Channels: Lock-Free Crossbeam                                  ║
    ║   Resilience: Circuit Breakers on ALL endpoints                  ║
    ║                                                                  ║
    ║   "When freight companies die, we know first."                   ║
    ║                                                                  ║
    ╚══════════════════════════════════════════════════════════════════╝

    "#;
    println!("{}", banner);
}

#[tokio::main(flavor = "multi_thread", worker_threads = 8)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info"))
        )
        .with_target(true)
        .with_thread_ids(true)
        .with_thread_names(true)
        .with_file(true)
        .with_line_number(true)
        .with_ansi(true)
        .init();

    print_banner();

    info!("🚛 FREIGHT DOOM ENGINE initializing...");

    // Load configuration
    let config = Arc::new(Config::from_env());
    info!("✅ Configuration loaded: redis_url={}", config.redis_url);

    // Lock-free crossbeam channel for events (capacity: 10,000)
    let (event_tx, event_rx) = crossbeam_channel::bounded::<BankruptcyEvent>(10_000);
    info!("✅ Lock-free crossbeam channel created (capacity: 10,000)");

    // Deduplication engine: Bloom filter + LRU cache
    let dedup_engine = Arc::new(DedupEngine::new(
        config.bloom_expected_items,
        config.bloom_false_positive_rate,
        config.lru_cache_size,
        config.bloom_rotation_interval.as_secs(),
    ));
    info!("✅ Deduplication engine online");

    // Metrics collector
    let metrics_collector = Arc::new(MetricsCollector::new());
    info!("✅ Metrics collector initialized");

    // Shutdown signal
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    // ═══════════════════════════════════════════
    // SPAWN SCANNERS
    // ═══════════════════════════════════════════

    info!("🚀 Spawning scanner tasks...");

    // PACER Scanner
    let pacer_config = config.clone();
    let pacer_tx = event_tx.clone();
    let pacer_dedup = dedup_engine.clone();
    let mut pacer_shutdown = shutdown_rx.clone();
    let pacer_handle = tokio::spawn(async move {
        info!("📡 PACER Scanner: ONLINE");
        pacer_scanner::run(pacer_config, pacer_tx, pacer_dedup, &mut pacer_shutdown).await;
        info!("📡 PACER Scanner: OFFLINE");
    });

    // SEC EDGAR Scanner
    let edgar_config = config.clone();
    let edgar_tx = event_tx.clone();
    let edgar_dedup = dedup_engine.clone();
    let mut edgar_shutdown = shutdown_rx.clone();
    let edgar_handle = tokio::spawn(async move {
        info!("📡 EDGAR Scanner: ONLINE");
        edgar_scanner::run(edgar_config, edgar_tx, edgar_dedup, &mut edgar_shutdown).await;
        info!("📡 EDGAR Scanner: OFFLINE");
    });

    // FMCSA Scanner
    let fmcsa_config = config.clone();
    let fmcsa_tx = event_tx.clone();
    let fmcsa_dedup = dedup_engine.clone();
    let mut fmcsa_shutdown = shutdown_rx.clone();
    let fmcsa_handle = tokio::spawn(async move {
        info!("📡 FMCSA Scanner: ONLINE");
        fmcsa_scanner::run(fmcsa_config, fmcsa_tx, fmcsa_dedup, &mut fmcsa_shutdown).await;
        info!("📡 FMCSA Scanner: OFFLINE");
    });

    // CourtListener Scanner
    let cl_config = config.clone();
    let cl_tx = event_tx.clone();
    let cl_dedup = dedup_engine.clone();
    let mut cl_shutdown = shutdown_rx.clone();
    let cl_handle = tokio::spawn(async move {
        info!("📡 CourtListener Scanner: ONLINE");
        court_listener_scanner::run(cl_config, cl_tx, cl_dedup, &mut cl_shutdown).await;
        info!("📡 CourtListener Scanner: OFFLINE");
    });

    // Drop our copy of event_tx so publisher knows when all senders are gone
    drop(event_tx);

    // ═══════════════════════════════════════════
    // SPAWN REDIS PUBLISHER
    // ═══════════════════════════════════════════
    let pub_config = config.clone();
    let pub_shutdown = shutdown_rx.clone();
    let (publisher, _pub_stats) = RedisPublisher::new(pub_config, event_rx, pub_shutdown);
    let publisher_handle = tokio::spawn(async move {
        info!("📤 Redis Publisher: ONLINE");
        if let Err(e) = publisher.run().await {
            error!("📤 Redis Publisher error: {}", e);
        }
        info!("📤 Redis Publisher: OFFLINE");
    });

    // ═══════════════════════════════════════════
    // SPAWN METRICS HTTP SERVER on port 9090
    // ═══════════════════════════════════════════
    let metrics_for_server = metrics_collector.clone();
    let mut metrics_shutdown = shutdown_rx.clone();
    let metrics_handle = tokio::spawn(async move {
        info!("📊 Metrics server starting on port 9090...");
        metrics::run_metrics_server(metrics_for_server, &mut metrics_shutdown).await;
        info!("📊 Metrics server: OFFLINE");
    });

    info!("═══════════════════════════════════════════════════════");
    info!("  🟢 ALL SYSTEMS ONLINE - FREIGHT DOOM ENGINE ACTIVE");
    info!("  📡 4 scanners active");
    info!("  📤 Publishing to Redis at {}", config.redis_url);
    info!("  📊 Metrics at http://0.0.0.0:9090/metrics");
    info!("  ⚡ Press Ctrl+C for graceful shutdown");
    info!("═══════════════════════════════════════════════════════");

    // Wait for shutdown signal
    match signal::ctrl_c().await {
        Ok(()) => {
            warn!("🛑 Shutdown signal received!");
            let _ = shutdown_tx.send(true);
        }
        Err(err) => {
            error!("❌ Signal listener error: {}", err);
            let _ = shutdown_tx.send(true);
        }
    }

    info!("⏳ Waiting for tasks to complete (timeout: 10s)...");
    let _ = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        async {
            let _ = tokio::join!(
                pacer_handle,
                edgar_handle,
                fmcsa_handle,
                cl_handle,
                publisher_handle,
                metrics_handle,
            );
        }
    ).await;

    info!("💀 FREIGHT DOOM ENGINE: OFFLINE");
    Ok(())
}
