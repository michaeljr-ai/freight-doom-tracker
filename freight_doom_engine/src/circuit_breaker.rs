// =============================================================================
// circuit_breaker.rs — THE RESILIENCE GUARDIAN
// =============================================================================
//
// The Circuit Breaker pattern, as applied to government API monitoring.
//
// When PACER goes down (and it WILL go down — it's a government website),
// we don't want to keep hammering it with requests. That would be:
// 1. Pointless (the server is down)
// 2. Rude (they have enough problems)
// 3. Potentially grounds for getting IP-banned
//
// Instead, we use a circuit breaker that "trips" after N consecutive failures
// and stops making requests for a cooldown period. After the cooldown, we
// send one tentative request (the "half-open" state). If it works, great,
// we resume normal operations. If it fails, back to timeout purgatory.
//
// This is the same pattern Netflix uses for their microservices.
// Are we Netflix? No. Do we have the same infrastructure challenges as
// Netflix? Also no. But do we implement the same resilience patterns?
// YOU BET YOUR SWEET LOAD BOARD WE DO.
// =============================================================================

use parking_lot::RwLock;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{info, warn};

/// The three states of a circuit breaker, mirroring the three states
/// of a trucker's relationship with dispatch:
///
/// - Closed: Everything is fine, requests flow freely (dispatch is sending loads)
/// - Open: Everything is broken, no requests allowed (dispatch ghosted you)
/// - HalfOpen: Cautiously testing if things are working again (dispatch texted "u up?")
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum CircuitState {
    /// Normal operation. Requests flow through. Life is good.
    Closed,
    /// Circuit is tripped. No requests allowed. We're in timeout.
    Open,
    /// Testing the waters. One request allowed to see if the API is back.
    HalfOpen,
}

impl std::fmt::Display for CircuitState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CircuitState::Closed => write!(f, "CLOSED"),
            CircuitState::Open => write!(f, "OPEN"),
            CircuitState::HalfOpen => write!(f, "HALF_OPEN"),
        }
    }
}

/// Internal mutable state for the circuit breaker.
struct CircuitBreakerInner {
    state: CircuitState,
    failure_count: u32,
    success_count: u32,
    last_failure_time: Option<Instant>,
    last_state_change: Instant,
    total_trips: u64,
}

/// The Circuit Breaker itself. Thread-safe, configurable, and ready to
/// protect our scanners from the harsh reality of unreliable government APIs.
pub struct CircuitBreaker {
    /// The name of this circuit breaker (e.g., "PACER", "EDGAR")
    /// Used for logging and metrics so we know WHICH API is misbehaving.
    name: String,

    /// The inner state, protected by a parking_lot RwLock because
    /// std::sync::RwLock is for people with patience we don't have.
    inner: Arc<RwLock<CircuitBreakerInner>>,

    /// Number of failures before the circuit trips.
    failure_threshold: u32,

    /// How long to wait before trying again after the circuit trips.
    reset_timeout: Duration,

    /// Number of successes in half-open state before fully closing.
    success_threshold: u32,
}

impl CircuitBreaker {
    /// Create a new circuit breaker with the given configuration.
    ///
    /// # Arguments
    /// * `name` - Human-readable name for logging
    /// * `failure_threshold` - Failures before tripping
    /// * `reset_timeout` - Cooldown duration when tripped
    /// * `success_threshold` - Successes in half-open before closing
    pub fn new(
        name: impl Into<String>,
        failure_threshold: u32,
        reset_timeout: Duration,
        success_threshold: u32,
    ) -> Self {
        let name = name.into();
        info!(
            name = %name,
            failure_threshold = failure_threshold,
            reset_timeout_secs = reset_timeout.as_secs(),
            success_threshold = success_threshold,
            "Circuit breaker initialized — standing guard against API failures"
        );

        Self {
            name,
            inner: Arc::new(RwLock::new(CircuitBreakerInner {
                state: CircuitState::Closed,
                failure_count: 0,
                success_count: 0,
                last_failure_time: None,
                last_state_change: Instant::now(),
                total_trips: 0,
            })),
            failure_threshold,
            reset_timeout,
            success_threshold,
        }
    }

    /// Check if a request is allowed to proceed.
    ///
    /// Returns `true` if the request can go through.
    /// Returns `false` if the circuit is open and we're in timeout.
    ///
    /// In the HalfOpen state, this transitions on the first call but
    /// still allows the request through for testing.
    pub fn allow_request(&self) -> bool {
        let mut inner = self.inner.write();

        match inner.state {
            CircuitState::Closed => {
                // Everything is fine. Let the request through.
                true
            }
            CircuitState::Open => {
                // Check if the timeout has expired
                if let Some(last_failure) = inner.last_failure_time {
                    if last_failure.elapsed() >= self.reset_timeout {
                        // Timeout expired! Transition to half-open.
                        // We'll allow ONE request through to test the waters.
                        info!(
                            name = %self.name,
                            "Circuit breaker transitioning OPEN -> HALF_OPEN — testing if API is back"
                        );
                        inner.state = CircuitState::HalfOpen;
                        inner.success_count = 0;
                        inner.last_state_change = Instant::now();
                        true
                    } else {
                        // Still in timeout. No requests allowed.
                        let remaining = self.reset_timeout - last_failure.elapsed();
                        warn!(
                            name = %self.name,
                            remaining_secs = remaining.as_secs(),
                            "Circuit breaker OPEN — request blocked, {} seconds until retry",
                            remaining.as_secs()
                        );
                        false
                    }
                } else {
                    // This shouldn't happen (open without a failure time)
                    // but let's be defensive and allow the request
                    true
                }
            }
            CircuitState::HalfOpen => {
                // Allow the test request through.
                true
            }
        }
    }

    /// Record a successful request.
    ///
    /// In Closed state: resets the failure counter (consecutive failures broken).
    /// In HalfOpen state: increments success counter, may close the circuit.
    /// In Open state: shouldn't happen, but we handle it gracefully.
    pub fn record_success(&self) {
        let mut inner = self.inner.write();

        match inner.state {
            CircuitState::Closed => {
                // Good news! Reset the failure counter.
                inner.failure_count = 0;
            }
            CircuitState::HalfOpen => {
                // A success in half-open state! We're making progress.
                inner.success_count += 1;

                if inner.success_count >= self.success_threshold {
                    // Enough successes to close the circuit again.
                    info!(
                        name = %self.name,
                        successes = inner.success_count,
                        "Circuit breaker transitioning HALF_OPEN -> CLOSED — API is healthy again!"
                    );
                    inner.state = CircuitState::Closed;
                    inner.failure_count = 0;
                    inner.success_count = 0;
                    inner.last_state_change = Instant::now();
                }
            }
            CircuitState::Open => {
                // This shouldn't happen, but hey, good news is good news.
                warn!(
                    name = %self.name,
                    "Success recorded while circuit is OPEN — this is unexpected but welcome"
                );
            }
        }
    }

    /// Record a failed request.
    ///
    /// In Closed state: increments failure counter, may trip the circuit.
    /// In HalfOpen state: immediately trips back to Open.
    /// In Open state: shouldn't happen, but we note it.
    pub fn record_failure(&self) {
        let mut inner = self.inner.write();

        match inner.state {
            CircuitState::Closed => {
                inner.failure_count += 1;
                inner.last_failure_time = Some(Instant::now());

                if inner.failure_count >= self.failure_threshold {
                    // Too many failures. Trip the circuit!
                    warn!(
                        name = %self.name,
                        failures = inner.failure_count,
                        "Circuit breaker TRIPPED — transitioning CLOSED -> OPEN"
                    );
                    inner.state = CircuitState::Open;
                    inner.total_trips += 1;
                    inner.last_state_change = Instant::now();
                } else {
                    warn!(
                        name = %self.name,
                        failures = inner.failure_count,
                        threshold = self.failure_threshold,
                        "Failure recorded — {}/{} before circuit trips",
                        inner.failure_count,
                        self.failure_threshold
                    );
                }
            }
            CircuitState::HalfOpen => {
                // The test request failed. Back to Open state.
                warn!(
                    name = %self.name,
                    "Test request failed in HALF_OPEN — transitioning back to OPEN"
                );
                inner.state = CircuitState::Open;
                inner.failure_count = self.failure_threshold; // Keep it maxed
                inner.last_failure_time = Some(Instant::now());
                inner.total_trips += 1;
                inner.last_state_change = Instant::now();
            }
            CircuitState::Open => {
                // Already open. Update the failure time to extend the timeout.
                inner.last_failure_time = Some(Instant::now());
            }
        }
    }

    /// Get the current state of the circuit breaker.
    pub fn state(&self) -> CircuitState {
        self.inner.read().state.clone()
    }

    /// Get the name of this circuit breaker.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get a snapshot of circuit breaker stats for metrics.
    pub fn snapshot(&self) -> CircuitBreakerSnapshot {
        let inner = self.inner.read();
        CircuitBreakerSnapshot {
            name: self.name.clone(),
            state: inner.state.clone(),
            failure_count: inner.failure_count,
            success_count: inner.success_count,
            total_trips: inner.total_trips,
            time_in_current_state_secs: inner.last_state_change.elapsed().as_secs(),
        }
    }
}

/// A serializable snapshot of circuit breaker state for the metrics endpoint.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CircuitBreakerSnapshot {
    pub name: String,
    pub state: CircuitState,
    pub failure_count: u32,
    pub success_count: u32,
    pub total_trips: u64,
    pub time_in_current_state_secs: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_starts_closed() {
        let cb = CircuitBreaker::new("test", 3, Duration::from_secs(5), 2);
        assert_eq!(cb.state(), CircuitState::Closed);
        assert!(cb.allow_request());
    }

    #[test]
    fn test_trips_after_threshold_failures() {
        let cb = CircuitBreaker::new("test", 3, Duration::from_secs(5), 2);
        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Closed);
        cb.record_failure(); // This should trip it
        assert_eq!(cb.state(), CircuitState::Open);
        assert!(!cb.allow_request());
    }

    #[test]
    fn test_success_resets_failure_count() {
        let cb = CircuitBreaker::new("test", 3, Duration::from_secs(5), 2);
        cb.record_failure();
        cb.record_failure();
        cb.record_success(); // Reset!
        cb.record_failure(); // Only 1 failure now, not 3
        assert_eq!(cb.state(), CircuitState::Closed);
    }
}
