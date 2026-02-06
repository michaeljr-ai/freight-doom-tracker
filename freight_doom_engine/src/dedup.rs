// =============================================================================
// dedup.rs — THE DEDUPLICATION FORTRESS
// =============================================================================
//
// This module implements a hybrid Bloom Filter + LRU Cache deduplication
// engine. Because seeing the same bankruptcy event twice would be like
// getting dumped by the same person twice — once is bad enough.
//
// The architecture is intentionally overkill:
//
// 1. First, we check the Bloom filter (O(k) where k is the number of hash
//    functions, which is basically O(1)). If the Bloom filter says "never
//    seen it", we KNOW it's new. Bloom filters never have false negatives.
//
// 2. If the Bloom filter says "maybe seen it" (because Bloom filters DO
//    have false positives), we check the LRU cache for a definitive answer.
//
// 3. The Bloom filter auto-rotates every hour to prevent saturation.
//    A saturated Bloom filter says "yes" to everything, which is about
//    as useful as a chocolate teapot.
//
// 4. Everything is thread-safe with parking_lot RwLock, because we have
//    multiple scanner threads all trying to deduplicate simultaneously,
//    and data races are not a feature we're looking to implement.
//
// Is this overkill for deduplicating maybe 100 events per day? YES.
// Could we just use a HashSet? YES.
// Are we going to use a HashSet? ABSOLUTELY NOT.
// =============================================================================

use bloomfilter::Bloom;
use lru::LruCache;
use parking_lot::RwLock;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, info, warn};

/// The Deduplication Engine. A monument to over-engineering.
///
/// Thread-safe, probabilistic, self-rotating, and completely unnecessary
/// for the volume of data we're processing. But boy, does it feel good.
pub struct DedupEngine {
    /// The Bloom filter — our first line of defense against duplicates.
    /// Wrapped in an RwLock because we need to rotate it periodically,
    /// and wrapped in an Arc because multiple threads need access.
    bloom: Arc<RwLock<Bloom<String>>>,

    /// The LRU cache — our second line of defense.
    /// When the Bloom filter says "maybe", the LRU cache says "definitely."
    /// Bounded in size so we don't eat all the RAM.
    lru_cache: Arc<RwLock<LruCache<String, bool>>>,

    /// When the Bloom filter was last rotated.
    /// We track this to know when it's time for a fresh one.
    last_rotation: Arc<RwLock<Instant>>,

    /// How often to rotate the Bloom filter, in seconds.
    rotation_interval_secs: u64,

    /// Parameters for creating new Bloom filters on rotation.
    bloom_expected_items: u64,
    bloom_fp_rate: f64,

    /// Counters for metrics. Because if we can't measure it,
    /// did the deduplication even happen?
    pub stats: Arc<DedupStats>,
}

/// Statistics about deduplication operations.
/// All counters are atomic because we're allergic to mutexes.
pub struct DedupStats {
    /// How many items were checked against the dedup engine
    pub checks: portable_atomic::AtomicU64,
    /// How many items were identified as new (not duplicates)
    pub unique: portable_atomic::AtomicU64,
    /// How many items were identified as duplicates
    pub duplicates: portable_atomic::AtomicU64,
    /// How many times the Bloom filter was rotated
    pub rotations: portable_atomic::AtomicU64,
    /// How many times the Bloom filter said "maybe" and we had to
    /// check the LRU cache (the "false positive rescue" counter)
    pub bloom_maybe_hits: portable_atomic::AtomicU64,
}

impl DedupStats {
    fn new() -> Self {
        Self {
            checks: portable_atomic::AtomicU64::new(0),
            unique: portable_atomic::AtomicU64::new(0),
            duplicates: portable_atomic::AtomicU64::new(0),
            rotations: portable_atomic::AtomicU64::new(0),
            bloom_maybe_hits: portable_atomic::AtomicU64::new(0),
        }
    }
}

impl DedupEngine {
    /// Create a new DedupEngine with the specified parameters.
    ///
    /// # Arguments
    /// * `expected_items` - How many items we expect before rotation
    /// * `fp_rate` - Target false positive rate (0.01 = 1%)
    /// * `lru_capacity` - Maximum items in the LRU cache
    /// * `rotation_interval_secs` - Seconds between Bloom filter rotations
    ///
    /// # Returns
    /// A freshly minted DedupEngine, ready to crush duplicates with
    /// extreme prejudice.
    pub fn new(
        expected_items: u64,
        fp_rate: f64,
        lru_capacity: usize,
        rotation_interval_secs: u64,
    ) -> Self {
        info!(
            expected_items = expected_items,
            fp_rate = fp_rate,
            lru_capacity = lru_capacity,
            rotation_secs = rotation_interval_secs,
            "Initializing Deduplication Engine — duplicates will be ELIMINATED"
        );

        let bloom = Bloom::new_for_fp_rate(expected_items as usize, fp_rate);
        let lru_size = NonZeroUsize::new(lru_capacity).unwrap_or(NonZeroUsize::new(1000).unwrap());
        let lru_cache = LruCache::new(lru_size);

        Self {
            bloom: Arc::new(RwLock::new(bloom)),
            lru_cache: Arc::new(RwLock::new(lru_cache)),
            last_rotation: Arc::new(RwLock::new(Instant::now())),
            rotation_interval_secs,
            bloom_expected_items: expected_items,
            bloom_fp_rate: fp_rate,
            stats: Arc::new(DedupStats::new()),
        }
    }

    /// Check if an item has been seen before, and if not, mark it as seen.
    ///
    /// Returns `true` if the item is NEW (not a duplicate).
    /// Returns `false` if the item has been seen before (duplicate).
    ///
    /// The logic flow:
    /// 1. Check if Bloom filter rotation is needed
    /// 2. Check Bloom filter for fast "definitely new" answer
    /// 3. If Bloom says "maybe seen", check LRU cache
    /// 4. If truly new, add to both Bloom filter and LRU cache
    ///
    /// This entire operation is thread-safe, which is good because
    /// we have scanners racing each other to report bankruptcies.
    pub fn check_and_insert(&self, key: &str) -> bool {
        use portable_atomic::Ordering;

        self.stats.checks.fetch_add(1, Ordering::Relaxed);

        // Step 0: Maybe rotate the bloom filter if it's getting stale
        self.maybe_rotate();

        // Step 1: Check the Bloom filter
        // Read lock only — multiple threads can check simultaneously
        let bloom_says_maybe_seen = {
            let bloom = self.bloom.read();
            bloom.check(&key.to_string())
        };

        if bloom_says_maybe_seen {
            // The Bloom filter thinks it's seen this before.
            // But Bloom filters lie (false positives). Let's check the LRU.
            self.stats.bloom_maybe_hits.fetch_add(1, Ordering::Relaxed);

            let mut lru = self.lru_cache.write();
            if lru.get(&key.to_string()).is_some() {
                // LRU confirms: this is a genuine duplicate.
                // Move along, nothing to see here.
                self.stats.duplicates.fetch_add(1, Ordering::Relaxed);
                debug!(key = key, "Duplicate detected — Bloom + LRU confirmed");
                return false;
            }

            // Bloom said "maybe" but LRU said "nope".
            // This was a Bloom filter false positive! The event is actually new.
            // Add it to both filters and let it through.
            debug!(
                key = key,
                "Bloom false positive rescued by LRU — event is actually new"
            );
        }

        // Step 2: This is a genuinely new item. Add it everywhere.
        {
            let mut bloom = self.bloom.write();
            bloom.set(&key.to_string());
        }
        {
            let mut lru = self.lru_cache.write();
            lru.put(key.to_string(), true);
        }

        self.stats.unique.fetch_add(1, Ordering::Relaxed);
        debug!(key = key, "New unique item accepted into the dedup engine");
        true
    }

    /// Check if it's time to rotate the Bloom filter and do so if needed.
    ///
    /// Rotation means creating a brand new, empty Bloom filter and
    /// discarding the old one. This prevents the filter from becoming
    /// saturated over time (where it starts saying "yes" to everything).
    ///
    /// The LRU cache is NOT rotated — it self-evicts old entries naturally.
    fn maybe_rotate(&self) {
        let should_rotate = {
            let last = self.last_rotation.read();
            last.elapsed().as_secs() >= self.rotation_interval_secs
        };

        if should_rotate {
            let mut bloom = self.bloom.write();
            let mut last = self.last_rotation.write();

            // Double-check after acquiring write lock (another thread might
            // have rotated while we were waiting for the lock)
            if last.elapsed().as_secs() >= self.rotation_interval_secs {
                *bloom = Bloom::new_for_fp_rate(
                    self.bloom_expected_items as usize,
                    self.bloom_fp_rate,
                );
                *last = Instant::now();

                self.stats.rotations.fetch_add(1, portable_atomic::Ordering::Relaxed);
                info!(
                    "Bloom filter rotated — fresh filter installed, old duplicates forgotten"
                );
            }
        }
    }

    /// Get a snapshot of the current dedup statistics.
    /// Useful for the metrics endpoint.
    pub fn snapshot(&self) -> DedupSnapshot {
        use portable_atomic::Ordering;
        DedupSnapshot {
            total_checks: self.stats.checks.load(Ordering::Relaxed),
            unique_items: self.stats.unique.load(Ordering::Relaxed),
            duplicates_caught: self.stats.duplicates.load(Ordering::Relaxed),
            bloom_rotations: self.stats.rotations.load(Ordering::Relaxed),
            bloom_false_positive_rescues: self.stats.bloom_maybe_hits.load(Ordering::Relaxed),
            lru_cache_size: self.lru_cache.read().len(),
        }
    }
}

/// A snapshot of dedup engine statistics at a point in time.
/// Serializable for the metrics endpoint.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DedupSnapshot {
    pub total_checks: u64,
    pub unique_items: u64,
    pub duplicates_caught: u64,
    pub bloom_rotations: u64,
    pub bloom_false_positive_rescues: u64,
    pub lru_cache_size: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_items_are_accepted() {
        let engine = DedupEngine::new(1000, 0.01, 100, 3600);
        assert!(engine.check_and_insert("bankruptcy:acme_freight:chapter_11"));
    }

    #[test]
    fn test_duplicate_items_are_rejected() {
        let engine = DedupEngine::new(1000, 0.01, 100, 3600);
        assert!(engine.check_and_insert("bankruptcy:acme_freight:chapter_11"));
        assert!(!engine.check_and_insert("bankruptcy:acme_freight:chapter_11"));
    }

    #[test]
    fn test_different_items_are_accepted() {
        let engine = DedupEngine::new(1000, 0.01, 100, 3600);
        assert!(engine.check_and_insert("bankruptcy:acme_freight:chapter_11"));
        assert!(engine.check_and_insert("bankruptcy:big_truck_co:chapter_7"));
    }
}
