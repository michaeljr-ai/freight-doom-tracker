// =============================================================================
// scanners/mod.rs — THE WAR ROOM
// =============================================================================
//
// This module is the command center for our four-headed hydra of bankruptcy
// detection. Each scanner runs in its own tokio task, with its own circuit
// breaker, its own HTTP client, and its own deeply concerning level of
// enthusiasm for finding companies in financial distress.
//
// We could have used one scanner. We built four. Because redundancy isn't
// just a design pattern — it's a lifestyle choice.
//
// Each scanner polls a different government data source, parses whatever
// format that particular agency decided was "standard" (XML, JSON, HTML,
// interpretive dance), and funnels everything through our SIMD-accelerated
// text scanner before pushing events into the crossbeam channel.
//
// Think of it as four bloodhounds, each trained on a different scent,
// all chasing the same quarry: bankrupt freight companies.
// =============================================================================

pub mod pacer_scanner;
pub mod edgar_scanner;
pub mod fmcsa_scanner;
pub mod court_listener_scanner;
