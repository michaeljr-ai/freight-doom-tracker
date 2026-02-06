// ═══════════════════════════════════════════════════════════════
// METRICS COLLECTOR - Because if you can't measure it, it didn't happen
// ═══════════════════════════════════════════════════════════════
//
// Atomic counters for everything. Lock-free because we're THAT paranoid
// about contention. Exposes a tiny HTTP server on port 9090 so
// the Rails app can check engine health.
//
// This is massive overkill for a metrics system. We have:
// - Atomic counters (no locks, no mutexes, PURE ATOMICS)
// - Per-scanner breakdowns
// - Throughput calculations
// - A full HTTP server just for metrics
// - JSON serialization of every metric

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::watch;
use tracing::{info, error};
use serde::Serialize;

/// The metrics snapshot - what gets serialized to JSON
#[derive(Debug, Serialize, Clone)]
pub struct MetricsSnapshot {
    pub total_events_detected: u64,
    pub total_events_published: u64,
    pub total_events_deduplicated: u64,
    pub pacer_events: u64,
    pub edgar_events: u64,
    pub fmcsa_events: u64,
    pub court_listener_events: u64,
    pub pacer_errors: u64,
    pub edgar_errors: u64,
    pub fmcsa_errors: u64,
    pub court_listener_errors: u64,
    pub uptime_seconds: u64,
    pub events_per_minute: f64,
    pub circuit_breaker_trips: u64,
    pub bloom_filter_rotations: u64,
    pub redis_publish_failures: u64,
    pub status: String,
}

/// Thread-safe atomic metrics collector
/// Every counter is atomic because mutexes are for the weak
pub struct MetricsCollector {
    total_detected: AtomicU64,
    total_published: AtomicU64,
    total_deduplicated: AtomicU64,
    pacer_events: AtomicU64,
    edgar_events: AtomicU64,
    fmcsa_events: AtomicU64,
    court_listener_events: AtomicU64,
    pacer_errors: AtomicU64,
    edgar_errors: AtomicU64,
    fmcsa_errors: AtomicU64,
    court_listener_errors: AtomicU64,
    circuit_breaker_trips: AtomicU64,
    bloom_filter_rotations: AtomicU64,
    redis_publish_failures: AtomicU64,
    start_time: Instant,
}

impl MetricsCollector {
    pub fn new() -> Self {
        Self {
            total_detected: AtomicU64::new(0),
            total_published: AtomicU64::new(0),
            total_deduplicated: AtomicU64::new(0),
            pacer_events: AtomicU64::new(0),
            edgar_events: AtomicU64::new(0),
            fmcsa_events: AtomicU64::new(0),
            court_listener_events: AtomicU64::new(0),
            pacer_errors: AtomicU64::new(0),
            edgar_errors: AtomicU64::new(0),
            fmcsa_errors: AtomicU64::new(0),
            court_listener_errors: AtomicU64::new(0),
            circuit_breaker_trips: AtomicU64::new(0),
            bloom_filter_rotations: AtomicU64::new(0),
            redis_publish_failures: AtomicU64::new(0),
            start_time: Instant::now(),
        }
    }

    pub fn increment_detected(&self) {
        self.total_detected.fetch_add(1, Ordering::Relaxed);
    }

    pub fn increment_published(&self) {
        self.total_published.fetch_add(1, Ordering::Relaxed);
    }

    pub fn increment_deduplicated(&self) {
        self.total_deduplicated.fetch_add(1, Ordering::Relaxed);
    }

    pub fn increment_scanner_events(&self, source: &str) {
        match source {
            "pacer" => { self.pacer_events.fetch_add(1, Ordering::Relaxed); }
            "edgar" => { self.edgar_events.fetch_add(1, Ordering::Relaxed); }
            "fmcsa" => { self.fmcsa_events.fetch_add(1, Ordering::Relaxed); }
            "court_listener" => { self.court_listener_events.fetch_add(1, Ordering::Relaxed); }
            _ => {}
        }
    }

    pub fn increment_scanner_errors(&self, source: &str) {
        match source {
            "pacer" => { self.pacer_errors.fetch_add(1, Ordering::Relaxed); }
            "edgar" => { self.edgar_errors.fetch_add(1, Ordering::Relaxed); }
            "fmcsa" => { self.fmcsa_errors.fetch_add(1, Ordering::Relaxed); }
            "court_listener" => { self.court_listener_errors.fetch_add(1, Ordering::Relaxed); }
            _ => {}
        }
    }

    pub fn increment_circuit_breaker_trips(&self) {
        self.circuit_breaker_trips.fetch_add(1, Ordering::Relaxed);
    }

    pub fn increment_bloom_rotations(&self) {
        self.bloom_filter_rotations.fetch_add(1, Ordering::Relaxed);
    }

    pub fn increment_redis_failures(&self) {
        self.redis_publish_failures.fetch_add(1, Ordering::Relaxed);
    }

    /// Take a snapshot of all metrics (lock-free reads)
    pub fn snapshot(&self) -> MetricsSnapshot {
        let uptime = self.start_time.elapsed().as_secs();
        let total_detected = self.total_detected.load(Ordering::Relaxed);
        let events_per_minute = if uptime > 0 {
            (total_detected as f64 / uptime as f64) * 60.0
        } else {
            0.0
        };

        MetricsSnapshot {
            total_events_detected: total_detected,
            total_events_published: self.total_published.load(Ordering::Relaxed),
            total_events_deduplicated: self.total_deduplicated.load(Ordering::Relaxed),
            pacer_events: self.pacer_events.load(Ordering::Relaxed),
            edgar_events: self.edgar_events.load(Ordering::Relaxed),
            fmcsa_events: self.fmcsa_events.load(Ordering::Relaxed),
            court_listener_events: self.court_listener_events.load(Ordering::Relaxed),
            pacer_errors: self.pacer_errors.load(Ordering::Relaxed),
            edgar_errors: self.edgar_errors.load(Ordering::Relaxed),
            fmcsa_errors: self.fmcsa_errors.load(Ordering::Relaxed),
            court_listener_errors: self.court_listener_errors.load(Ordering::Relaxed),
            uptime_seconds: uptime,
            events_per_minute,
            circuit_breaker_trips: self.circuit_breaker_trips.load(Ordering::Relaxed),
            bloom_filter_rotations: self.bloom_filter_rotations.load(Ordering::Relaxed),
            redis_publish_failures: self.redis_publish_failures.load(Ordering::Relaxed),
            status: "operational".to_string(),
        }
    }
}

/// Run a tiny HTTP server on port 9090 that serves metrics as JSON
/// This is the Rust equivalent of mounting a turret on a skateboard
pub async fn run_metrics_server(
    metrics: Arc<MetricsCollector>,
    shutdown: &mut watch::Receiver<bool>,
) {
    use tokio::net::TcpListener;
    use tokio::io::AsyncWriteExt;

    let listener = match TcpListener::bind("0.0.0.0:9090").await {
        Ok(l) => l,
        Err(e) => {
            error!("Failed to bind metrics server on :9090: {}", e);
            return;
        }
    };

    info!("📊 Metrics server listening on http://0.0.0.0:9090");

    loop {
        tokio::select! {
            accept_result = listener.accept() => {
                match accept_result {
                    Ok((mut stream, _addr)) => {
                        let snapshot = metrics.snapshot();
                        let json = serde_json::to_string_pretty(&snapshot)
                            .unwrap_or_else(|_| "{}".to_string());

                        let response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\nContent-Length: {}\r\n\r\n{}",
                            json.len(),
                            json,
                        );

                        let _ = stream.write_all(response.as_bytes()).await;
                    }
                    Err(e) => {
                        error!("Metrics server accept error: {}", e);
                    }
                }
            }
            _ = shutdown.changed() => {
                info!("Metrics server: shutting down");
                break;
            }
        }
    }
}
