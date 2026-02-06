// =============================================================================
// config.rs — THE GRAND CONFIGURATION CATHEDRAL
// =============================================================================
//
// Every system needs configuration, but not every system needs THIS MUCH
// configuration. We have knobs for knobs. Dials for dials. Thresholds for
// thresholds that control other thresholds.
//
// All values can be overridden via environment variables, because hardcoding
// configuration is how you end up on the front page of Hacker News for the
// wrong reasons.
//
// Default values have been carefully chosen through a rigorous process of
// "that seems about right" and "the API will probably rate-limit us if we
// go faster than this."
// =============================================================================

use std::env;
use std::time::Duration;

/// The Grand Configuration Struct. Every tunable parameter in the entire
/// engine lives here. If you need to change something, this is where you
/// come. Think of it as the cockpit of a fighter jet, except instead of
/// controlling weapons systems, you're controlling how aggressively we
/// poll government websites for signs of freight industry collapse.
#[derive(Debug, Clone)]
pub struct Config {
    // =========================================================================
    // REDIS CONFIGURATION
    // =========================================================================

    /// Redis connection URL. This is where we scream bankruptcy events into
    /// the void. The Rails app listens on the other end.
    /// Default: redis://127.0.0.1:6379
    pub redis_url: String,

    /// The Redis channel we publish bankruptcy events to.
    /// The Rails app subscribes to this channel and processes events.
    pub redis_channel: String,

    /// The Redis sorted set where we persist events with timestamps.
    /// Because pub/sub is fire-and-forget, and we don't want to forget.
    pub redis_sorted_set: String,

    // =========================================================================
    // POLLING CONFIGURATION
    // Because checking once per second is barely adequate, but checking
    // faster would get us IP-banned from every government website.
    // =========================================================================

    /// How often to poll PACER RSS feeds. Default: 60 seconds.
    /// PACER RSS feeds update roughly every few minutes, so polling
    /// every second would be wasteful. But we COULD. We have the technology.
    pub pacer_poll_interval: Duration,

    /// How often to poll SEC EDGAR. Default: 30 seconds.
    /// EDGAR is surprisingly responsive for a government website.
    pub edgar_poll_interval: Duration,

    /// How often to poll FMCSA. Default: 120 seconds.
    /// FMCSA data doesn't change that frequently, and they're more
    /// likely to rate-limit aggressive polling.
    pub fmcsa_poll_interval: Duration,

    /// How often to poll CourtListener. Default: 45 seconds.
    /// They're a non-profit. Let's be nice to their servers.
    pub court_listener_poll_interval: Duration,

    // =========================================================================
    // API ENDPOINTS
    // These are REAL public government URLs. No mocks. No fakes.
    // Just pure, unfiltered access to the machinery of financial doom.
    // =========================================================================

    /// PACER RSS feed base URL. Individual court feeds are appended to this.
    /// The actual RSS feeds follow the pattern:
    /// https://ecf.{court}.uscourts.gov/cgi-bin/rss_outside.pl
    pub pacer_base_url: String,

    /// SEC EDGAR full-text search API endpoint.
    /// This is the REAL EDGAR full-text search endpoint.
    pub edgar_search_url: String,

    /// FMCSA SAFER Web base URL for carrier lookups.
    /// The public QC (Quick Company) search.
    pub fmcsa_base_url: String,

    /// CourtListener API base URL.
    /// Free, open, and glorious.
    pub court_listener_base_url: String,

    // =========================================================================
    // BLOOM FILTER PARAMETERS
    // For when "probably unique" is good enough.
    // =========================================================================

    /// Expected number of items in the bloom filter before rotation.
    /// Higher = more memory, fewer false positives.
    /// Lower = less memory, more false positives.
    /// Default: 100_000 because we're optimists about the volume of
    /// freight bankruptcies we'll detect.
    pub bloom_expected_items: u64,

    /// Target false positive rate for the bloom filter.
    /// 0.01 = 1% chance of falsely thinking we've seen an event before.
    /// In practice, this means we might miss 1 in 100 bankruptcies.
    /// Given how many there are, this is acceptable.
    pub bloom_false_positive_rate: f64,

    /// How often to rotate the bloom filter (in seconds).
    /// Rotation prevents the filter from saturating and rejecting everything.
    /// Default: 3600 (1 hour)
    pub bloom_rotation_interval: Duration,

    /// Maximum number of items in the LRU cache backup.
    /// The LRU cache catches what the bloom filter might miss.
    pub lru_cache_size: usize,

    // =========================================================================
    // CIRCUIT BREAKER PARAMETERS
    // Because government APIs go down more often than you'd think.
    // =========================================================================

    /// Number of consecutive failures before the circuit breaker trips.
    /// Default: 5, because everyone deserves five chances.
    pub circuit_breaker_failure_threshold: u32,

    /// How long the circuit breaker stays open before allowing a test request.
    /// Default: 60 seconds. Long enough for the API to catch its breath.
    pub circuit_breaker_reset_timeout: Duration,

    /// Number of successful requests in half-open state before closing circuit.
    /// Default: 2, because fool me once, shame on you...
    pub circuit_breaker_success_threshold: u32,

    // =========================================================================
    // METRICS SERVER
    // =========================================================================

    /// Port for the metrics HTTP server.
    /// Default: 9090, because Prometheus conventions are conventions.
    pub metrics_port: u16,

    // =========================================================================
    // TEXT SCANNER PARAMETERS
    // =========================================================================

    /// Minimum confidence score to consider an event worth publishing.
    /// Below this threshold, we consider it noise.
    /// Default: 0.3 (30%) — we'd rather have false positives than miss
    /// a real bankruptcy.
    pub min_confidence_threshold: f64,
}

impl Config {
    /// Load configuration from environment variables with sensible defaults.
    /// "Sensible" here meaning "will work out of the box without any env vars
    /// but will also respect your wishes if you set them."
    ///
    /// Every parameter can be overridden via environment variables prefixed
    /// with FREIGHT_DOOM_. Because namespacing your env vars is what separates
    /// the professionals from the amateurs.
    pub fn from_env() -> Self {
        // Try to load .env file if it exists. Fail silently if it doesn't,
        // because not everyone has their life together enough to create
        // a .env file.
        let _ = dotenvy::dotenv();

        Config {
            // Redis
            redis_url: env_or_default("FREIGHT_DOOM_REDIS_URL", "redis://127.0.0.1:6379"),
            redis_channel: env_or_default("FREIGHT_DOOM_REDIS_CHANNEL", "bankruptcy:events"),
            redis_sorted_set: env_or_default("FREIGHT_DOOM_REDIS_SORTED_SET", "bankruptcy:events:history"),

            // Poll intervals (in seconds, converted to Duration)
            pacer_poll_interval: Duration::from_secs(
                env_or_default("FREIGHT_DOOM_PACER_POLL_SECS", "60").parse().unwrap_or(60)
            ),
            edgar_poll_interval: Duration::from_secs(
                env_or_default("FREIGHT_DOOM_EDGAR_POLL_SECS", "30").parse().unwrap_or(30)
            ),
            fmcsa_poll_interval: Duration::from_secs(
                env_or_default("FREIGHT_DOOM_FMCSA_POLL_SECS", "120").parse().unwrap_or(120)
            ),
            court_listener_poll_interval: Duration::from_secs(
                env_or_default("FREIGHT_DOOM_COURTLISTENER_POLL_SECS", "45").parse().unwrap_or(45)
            ),

            // API Endpoints — these are the REAL deal
            pacer_base_url: env_or_default(
                "FREIGHT_DOOM_PACER_BASE_URL",
                "https://ecf.uscourts.gov"
            ),
            edgar_search_url: env_or_default(
                "FREIGHT_DOOM_EDGAR_SEARCH_URL",
                "https://efts.sec.gov/LATEST/search-index"
            ),
            fmcsa_base_url: env_or_default(
                "FREIGHT_DOOM_FMCSA_BASE_URL",
                "https://mobile.fmcsa.dot.gov/qc/services/carriers"
            ),
            court_listener_base_url: env_or_default(
                "FREIGHT_DOOM_COURTLISTENER_BASE_URL",
                "https://www.courtlistener.com/api/rest/v3"
            ),

            // Bloom filter
            bloom_expected_items: env_or_default("FREIGHT_DOOM_BLOOM_ITEMS", "100000")
                .parse().unwrap_or(100_000),
            bloom_false_positive_rate: env_or_default("FREIGHT_DOOM_BLOOM_FP_RATE", "0.01")
                .parse().unwrap_or(0.01),
            bloom_rotation_interval: Duration::from_secs(
                env_or_default("FREIGHT_DOOM_BLOOM_ROTATION_SECS", "3600").parse().unwrap_or(3600)
            ),
            lru_cache_size: env_or_default("FREIGHT_DOOM_LRU_CACHE_SIZE", "10000")
                .parse().unwrap_or(10_000),

            // Circuit breaker
            circuit_breaker_failure_threshold: env_or_default(
                "FREIGHT_DOOM_CB_FAILURE_THRESHOLD", "5"
            ).parse().unwrap_or(5),
            circuit_breaker_reset_timeout: Duration::from_secs(
                env_or_default("FREIGHT_DOOM_CB_RESET_TIMEOUT_SECS", "60").parse().unwrap_or(60)
            ),
            circuit_breaker_success_threshold: env_or_default(
                "FREIGHT_DOOM_CB_SUCCESS_THRESHOLD", "2"
            ).parse().unwrap_or(2),

            // Metrics
            metrics_port: env_or_default("FREIGHT_DOOM_METRICS_PORT", "9090")
                .parse().unwrap_or(9090),

            // Text scanner
            min_confidence_threshold: env_or_default(
                "FREIGHT_DOOM_MIN_CONFIDENCE", "0.3"
            ).parse().unwrap_or(0.3),
        }
    }

    /// Returns the list of PACER bankruptcy court RSS feed URLs.
    /// These are REAL court RSS feeds from major bankruptcy courts
    /// across the United States. Each one is a firehose of financial despair.
    pub fn pacer_court_feeds(&self) -> Vec<(&'static str, String)> {
        // Major bankruptcy courts that handle significant commercial cases.
        // We focus on the courts where large logistics companies are likely
        // to file, which tends to be Delaware, Southern District of New York,
        // and other business-friendly jurisdictions.
        let courts = vec![
            ("Delaware", "deb"),                          // The bankruptcy capital of America
            ("Southern District of New York", "nysb"),    // Wall Street's backyard
            ("District of New Jersey", "njb"),            // Trucking company HQ central
            ("Northern District of Illinois", "ilnb"),    // Chicago, logistics hub
            ("Northern District of Texas", "txnb"),       // Dallas/Fort Worth freight corridor
            ("Southern District of Texas", "txsb"),       // Houston, energy + freight
            ("Central District of California", "cacb"),   // LA ports
            ("Northern District of Georgia", "ganb"),     // Atlanta logistics hub
            ("Eastern District of Virginia", "vaeb"),     // DC metro freight
            ("Western District of Missouri", "mowb"),     // Kansas City — trucking crossroads
            ("Southern District of Indiana", "insb"),     // Indianapolis freight hub
            ("Middle District of Tennessee", "tnmb"),     // Nashville logistics
        ];

        courts
            .into_iter()
            .map(|(name, code)| {
                let url = format!(
                    "https://ecf.{code}b.uscourts.gov/cgi-bin/rss_outside.pl"
                );
                (name, url)
            })
            .collect()
    }
}

/// Helper function to read an environment variable with a default fallback.
/// Because unwrap_or on env::var is ugly and we have standards.
fn env_or_default(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}
