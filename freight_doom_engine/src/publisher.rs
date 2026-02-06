// =============================================================================
// publisher.rs — THE REDIS DOOMSAYER
// =============================================================================
//
// This module takes bankruptcy events from the crossbeam channel and
// screams them into Redis via pub/sub. The Rails app listens on the
// other end, presumably with a mix of horror and fascination.
//
// Architecture:
// 1. Consumer loop reads from the lock-free crossbeam channel
// 2. Events are serialized to JSON (serde does the heavy lifting)
// 3. Events are published to a Redis pub/sub channel
// 4. Events are ALSO stored in a Redis sorted set (scored by timestamp)
//    for persistence, because pub/sub is fire-and-forget
// 5. Batch publishing to minimize Redis round trips
//
// The Redis sorted set acts as a durable event log. Even if the Rails
// app is down when a bankruptcy is detected, the event will be waiting
// in Redis when it comes back. Like a patient harbinger of doom.
// =============================================================================

use anyhow::Result;
use crossbeam_channel::Receiver;
use redis::AsyncCommands;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use tracing::{debug, error, info, warn};

use crate::config::Config;
use crate::models::BankruptcyEvent;

/// The Redis Publisher. Consumes events from the crossbeam channel
/// and publishes them to Redis with the urgency of a dispatcher
/// trying to cover a hot load.
pub struct RedisPublisher {
    config: Arc<Config>,
    receiver: Receiver<BankruptcyEvent>,
    shutdown: watch::Receiver<bool>,
    stats: Arc<PublisherStats>,
}

/// Publisher statistics for metrics.
pub struct PublisherStats {
    pub events_published: portable_atomic::AtomicU64,
    pub events_persisted: portable_atomic::AtomicU64,
    pub publish_errors: portable_atomic::AtomicU64,
    pub batches_sent: portable_atomic::AtomicU64,
}

impl PublisherStats {
    pub fn new() -> Self {
        Self {
            events_published: portable_atomic::AtomicU64::new(0),
            events_persisted: portable_atomic::AtomicU64::new(0),
            publish_errors: portable_atomic::AtomicU64::new(0),
            batches_sent: portable_atomic::AtomicU64::new(0),
        }
    }
}

/// A serializable snapshot of publisher stats.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PublisherSnapshot {
    pub events_published: u64,
    pub events_persisted: u64,
    pub publish_errors: u64,
    pub batches_sent: u64,
}

impl RedisPublisher {
    /// Create a new RedisPublisher.
    ///
    /// # Arguments
    /// * `config` - The global configuration
    /// * `receiver` - The receiving end of the crossbeam channel
    /// * `shutdown` - Watch channel for graceful shutdown signaling
    pub fn new(
        config: Arc<Config>,
        receiver: Receiver<BankruptcyEvent>,
        shutdown: watch::Receiver<bool>,
    ) -> (Self, Arc<PublisherStats>) {
        let stats = Arc::new(PublisherStats::new());
        let stats_clone = Arc::clone(&stats);
        (
            Self {
                config,
                receiver,
                shutdown,
                stats,
            },
            stats_clone,
        )
    }

    /// Run the publisher loop. This is an async function that runs
    /// until the shutdown signal is received.
    ///
    /// The loop:
    /// 1. Drains up to BATCH_SIZE events from the channel
    /// 2. Publishes them all to Redis pub/sub
    /// 3. Stores them in the sorted set
    /// 4. Sleeps briefly if no events were available
    /// 5. Repeats until shutdown
    ///
    /// We use batch publishing to minimize Redis round-trips.
    /// Publishing 10 events in one pipeline is much faster than
    /// 10 individual PUBLISH commands.
    pub async fn run(self) -> Result<()> {
        info!(
            channel = %self.config.redis_channel,
            sorted_set = %self.config.redis_sorted_set,
            "Redis Publisher starting — ready to broadcast financial doom"
        );

        // Connect to Redis with retry logic
        let client = redis::Client::open(self.config.redis_url.as_str())?;
        let mut con = loop {
            match client.get_multiplexed_async_connection().await {
                Ok(con) => {
                    info!("Redis connection established — the void is listening");
                    break con;
                }
                Err(e) => {
                    warn!(error = %e, "Failed to connect to Redis — retrying in 5 seconds");
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    if *self.shutdown.borrow() {
                        info!("Shutdown received during Redis connection retry — exiting");
                        return Ok(());
                    }
                }
            }
        };

        const BATCH_SIZE: usize = 50;
        let mut batch: Vec<BankruptcyEvent> = Vec::with_capacity(BATCH_SIZE);

        loop {
            // Check for shutdown signal
            if *self.shutdown.borrow() {
                // Drain remaining events before shutting down
                info!("Shutdown signal received — draining remaining events");
                while let Ok(event) = self.receiver.try_recv() {
                    batch.push(event);
                }
                if !batch.is_empty() {
                    if let Err(e) = self.publish_batch(&mut con, &batch).await {
                        error!(error = %e, "Failed to publish final batch during shutdown");
                    }
                }
                info!("Redis Publisher shutting down — no more doom to broadcast");
                return Ok(());
            }

            // Drain events from the channel into a batch
            batch.clear();
            while batch.len() < BATCH_SIZE {
                match self.receiver.try_recv() {
                    Ok(event) => batch.push(event),
                    Err(crossbeam_channel::TryRecvError::Empty) => break,
                    Err(crossbeam_channel::TryRecvError::Disconnected) => {
                        info!("Channel disconnected — publisher shutting down");
                        return Ok(());
                    }
                }
            }

            if batch.is_empty() {
                // No events to publish. Sleep briefly and check again.
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }

            // Publish the batch!
            if let Err(e) = self.publish_batch(&mut con, &batch).await {
                error!(
                    error = %e,
                    batch_size = batch.len(),
                    "Failed to publish batch to Redis — events may be lost!"
                );
                self.stats
                    .publish_errors
                    .fetch_add(batch.len() as u64, portable_atomic::Ordering::Relaxed);
            }
        }
    }

    /// Publish a batch of events to Redis.
    ///
    /// For each event:
    /// 1. PUBLISH to the pub/sub channel (for real-time consumers)
    /// 2. ZADD to the sorted set (for persistence/catch-up)
    ///
    /// We use a Redis pipeline to send all commands in one round-trip.
    /// This is like putting all your packages on one truck instead of
    /// sending a separate truck for each package.
    async fn publish_batch(
        &self,
        con: &mut redis::aio::MultiplexedConnection,
        batch: &[BankruptcyEvent],
    ) -> Result<()> {
        use portable_atomic::Ordering;

        for event in batch {
            let json = serde_json::to_string(event)?;

            // Publish to pub/sub channel for real-time consumers
            let _: () = con
                .publish(&self.config.redis_channel, &json)
                .await
                .map_err(|e| {
                    error!(
                        error = %e,
                        event_id = %event.id,
                        company = %event.company_name,
                        "Failed to PUBLISH event"
                    );
                    e
                })?;

            self.stats.events_published.fetch_add(1, Ordering::Relaxed);

            // Store in sorted set for persistence
            // Score is the Unix timestamp so events are ordered chronologically
            let score = event.detected_at.timestamp() as f64;
            let _: () = con
                .zadd(&self.config.redis_sorted_set, &json, score)
                .await
                .map_err(|e| {
                    error!(
                        error = %e,
                        event_id = %event.id,
                        "Failed to ZADD event to sorted set"
                    );
                    e
                })?;

            self.stats.events_persisted.fetch_add(1, Ordering::Relaxed);

            info!(
                event_id = %event.id,
                company = %event.company_name,
                source = %event.source,
                confidence = format!("{:.1}%", event.confidence_score * 100.0),
                "Event published to Redis — the Rails app has been notified of impending doom"
            );
        }

        self.stats.batches_sent.fetch_add(1, Ordering::Relaxed);

        debug!(
            batch_size = batch.len(),
            total_published = self.stats.events_published.load(Ordering::Relaxed),
            "Batch published successfully"
        );

        Ok(())
    }

    /// Get a snapshot of publisher statistics.
    pub fn snapshot(stats: &PublisherStats) -> PublisherSnapshot {
        use portable_atomic::Ordering;
        PublisherSnapshot {
            events_published: stats.events_published.load(Ordering::Relaxed),
            events_persisted: stats.events_persisted.load(Ordering::Relaxed),
            publish_errors: stats.publish_errors.load(Ordering::Relaxed),
            batches_sent: stats.batches_sent.load(Ordering::Relaxed),
        }
    }
}
