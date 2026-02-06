// =============================================================================
// edgar_scanner.rs — THE SEC'S INVOLUNTARY INFORMANT
// =============================================================================
//
// SEC EDGAR (Electronic Data Gathering, Analysis, and Retrieval) is the SEC's
// system for collecting, validating, and distributing financial filings from
// publicly traded companies. It has a full-text search API. A REAL one.
// That returns JSON. From a government agency. Miracles do happen.
//
// The EDGAR Full-Text Search System (EFTS) endpoint:
//   https://efts.sec.gov/LATEST/search-index?q=QUERY&dateRange=custom&...
//
// We search for bankruptcy-related filings (8-K, 10-K, 10-Q) that mention
// logistics, freight, trucking, carrier, and other transportation keywords.
// When a publicly traded freight company files an 8-K mentioning
// "material uncertainty" and "going concern," that's SEC-speak for
// "the trucks are about to stop."
//
// SEC EDGAR has one important rule: you MUST send a descriptive User-Agent
// header with your contact information. This is their actual policy, not
// us being extra. Failing to do so gets you rate-limited into oblivion,
// which is ironic because rate-limiting is basically what happens to
// freight companies before they go bankrupt.
//
// We rotate through 10 different search queries to cover every conceivable
// angle of logistics bankruptcy. "bankruptcy freight carrier" might miss
// a filing that "chapter 11 transportation services" would catch. By
// rotating queries, we cast a wide net. Some might say too wide. Those
// people don't understand the gravity of detecting a freight company's
// descent into Chapter 11 approximately 30 seconds faster than everyone else.
//
// Is querying the SEC full-text search API every 30 seconds for variations
// of "bankrupt trucking company" a proportionate response to tracking
// freight industry health? The answer depends on how much you care about
// freight. We care a lot.
// =============================================================================

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use chrono::{NaiveDate, Utc};
use crossbeam_channel::Sender;
use tokio::sync::watch;
use tracing::{debug, error, info, warn};

use crate::circuit_breaker::CircuitBreaker;
use crate::config::Config;
use crate::dedup::DedupEngine;
use crate::models::{
    BankruptcyChapter, BankruptcyEvent, EdgarSearchResult, Source,
};
use crate::text_scanner;

// =============================================================================
// EDGAR EFTS Search Queries
// =============================================================================
// We rotate through these queries to maximize coverage. Each query targets
// a different combination of bankruptcy and logistics keywords. Think of it
// as casting 10 fishing lines into the SEC's data ocean, each baited with
// a different flavor of financial distress.
//
// The rotation happens on every poll cycle, so we cycle through all 10
// queries roughly every 5 minutes at the default 30-second interval.
// That's 10 different angles of attack on the question "did a freight
// company just implode?"
// =============================================================================
const SEARCH_QUERIES: &[&str] = &[
    "bankruptcy freight carrier",
    "bankruptcy trucking company",
    "chapter 11 logistics",
    "chapter 7 freight",
    "chapter 11 carrier transportation",
    "bankruptcy broker freight",
    "insolvency logistics carrier",
    "bankruptcy 3PL warehouse",
    "chapter 11 transportation services",
    "going concern motor carrier",
];

/// The main entry point for the SEC EDGAR scanner.
///
/// This function loops forever, searching SEC EDGAR's full-text search API
/// for bankruptcy filings mentioning freight/logistics companies. It's like
/// having a securities lawyer on retainer who does nothing but read 10-K
/// filings all day looking for the words "trucking" and "liquidation" in
/// the same paragraph.
///
/// # Arguments
/// * `config` - Global configuration with edgar_search_url and edgar_poll_interval.
/// * `event_tx` - Crossbeam channel sender for detected bankruptcy events.
/// * `dedup` - The Bloom filter + LRU deduplication engine.
/// * `shutdown` - Watch channel for graceful shutdown.
pub async fn run(
    config: Arc<Config>,
    event_tx: Sender<BankruptcyEvent>,
    dedup: Arc<DedupEngine>,
    shutdown: &mut watch::Receiver<bool>,
) {
    info!("EDGAR Scanner initializing — preparing to data-mine the SEC like a very polite, very persistent securities analyst");

    // Build an HTTP client with SEC-compliant User-Agent.
    // The SEC requires a descriptive User-Agent with contact information.
    // This is the one government API requirement that actually makes sense.
    // If you don't include contact info, they throttle you to 10 requests
    // per second, which for us would be like putting a speed governor on
    // a Formula 1 car. We comply not because we must, but because we
    // respect the SEC's surprisingly functional API infrastructure.
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent("FreightDoomEngine/1.0 (bankruptcy-tracker@research.dev; educational-project)")
        .build()
        .expect("Failed to build EDGAR HTTP client — the SEC will never know we existed");

    // Circuit breaker for EDGAR.
    // EDGAR is surprisingly reliable for a government API, but when it goes
    // down, it tends to stay down for a while. We use a lower failure
    // threshold because EDGAR errors usually mean something is genuinely wrong.
    let circuit_breaker = CircuitBreaker::new(
        "EDGAR",
        config.circuit_breaker_failure_threshold,
        config.circuit_breaker_reset_timeout,
        config.circuit_breaker_success_threshold,
    );

    // Atomic counter for rotating through search queries.
    // AtomicUsize because we're allergic to mutexes in this codebase.
    let query_index = AtomicUsize::new(0);

    let poll_interval = config.edgar_poll_interval;
    let search_url = config.edgar_search_url.clone();
    let min_confidence = config.min_confidence_threshold;

    info!(
        poll_interval_secs = poll_interval.as_secs(),
        search_url = search_url.as_str(),
        queries = SEARCH_QUERIES.len(),
        "EDGAR Scanner online — monitoring SEC filings with the enthusiasm of a forensic accountant at an Enron reunion"
    );

    loop {
        tokio::select! {
            _ = tokio::time::sleep(poll_interval) => {
                if !circuit_breaker.allow_request() {
                    debug!("EDGAR: circuit breaker is OPEN — the SEC needs a moment");
                    continue;
                }

                // Rotate to the next search query.
                // fetch_add wraps around naturally with the modulo below.
                let idx = query_index.fetch_add(1, Ordering::Relaxed) % SEARCH_QUERIES.len();
                let query = SEARCH_QUERIES[idx];

                // Build the EDGAR EFTS search URL.
                // We search for today's filings to minimize data volume and
                // maximize freshness. The API supports date range filtering,
                // which we use to focus on the most recent filings.
                //
                // The EFTS API returns JSON (praise be) with an Elasticsearch-style
                // response format: { hits: { total: { value: N }, hits: [...] } }
                let today = Utc::now().format("%Y-%m-%d").to_string();
                let url = format!(
                    "{}?q={}&dateRange=custom&startdt={}&enddt={}&forms=8-K,10-K,10-Q&from=0&size=40",
                    search_url,
                    urlencoding::encode(query),
                    today,
                    today,
                );

                debug!(
                    query = query,
                    date = today.as_str(),
                    "EDGAR: executing search query {}/{} — hunting for freight company filings like a truffle pig in a forest of 10-Ks",
                    idx + 1,
                    SEARCH_QUERIES.len()
                );

                // Make the request. EDGAR is usually fast (< 2 seconds)
                // but occasionally takes a scenic route through their infrastructure.
                let response = match client.get(&url).send().await {
                    Ok(resp) => resp,
                    Err(e) => {
                        circuit_breaker.record_failure();
                        warn!(
                            error = %e,
                            query = query,
                            "EDGAR: request failed — the SEC's servers are experiencing a material adverse event"
                        );
                        continue;
                    }
                };

                let status = response.status();
                if !status.is_success() {
                    if status.as_u16() == 429 {
                        // Rate limited. The SEC is telling us to calm down.
                        // We should listen. They have lawyers.
                        warn!("EDGAR: rate limited (HTTP 429) — the SEC is telling us to take a breather");
                        circuit_breaker.record_failure();
                    } else {
                        debug!("EDGAR: non-success HTTP status: {} — filing this under 'not our problem'", status);
                    }
                    continue;
                }

                circuit_breaker.record_success();

                let body = match response.text().await {
                    Ok(b) => b,
                    Err(e) => {
                        debug!(error = %e, "EDGAR: failed to read response body");
                        continue;
                    }
                };

                // Parse the EDGAR JSON response using the EdgarSearchResult
                // types defined in models.rs. These mirror the actual EFTS
                // response schema, which is Elasticsearch under the hood.
                let search_result: EdgarSearchResult = match serde_json::from_str(&body) {
                    Ok(r) => r,
                    Err(_) => {
                        // Sometimes EDGAR returns HTML error pages instead of JSON.
                        // In those cases, we do a quick freight check on the raw text
                        // just to be thorough, because we're nothing if not thorough.
                        if text_scanner::quick_freight_check(&body) {
                            debug!("EDGAR: got non-JSON response that mentions freight — interesting but not actionable");
                        }
                        continue;
                    }
                };

                // Extract total hit count for logging
                let total_hits = search_result
                    .hits
                    .as_ref()
                    .and_then(|h| h.total.as_ref())
                    .and_then(|t| t.value)
                    .unwrap_or(0);

                if total_hits > 0 {
                    debug!(
                        total_hits = total_hits,
                        query = query,
                        "EDGAR: {} total hits — let's see how many are freight companies circling the drain",
                        total_hits
                    );
                }

                // Process each hit
                let hits = search_result
                    .hits
                    .as_ref()
                    .and_then(|h| h.hits.as_ref());

                let empty_vec = Vec::new();
                let hits = hits.unwrap_or(&empty_vec);

                let mut new_events = 0u64;

                for hit in hits {
                    let source = match &hit.source {
                        Some(s) => s,
                        None => continue,
                    };

                    // Combine all available text fields for scanning
                    let entity_name = source.entity_name.as_deref().unwrap_or("");
                    let file_description = source.file_description.as_deref().unwrap_or("");
                    let file_type = source.file_type.as_deref().unwrap_or("");

                    let combined = format!("{} {} {}", entity_name, file_description, file_type);

                    // Quick freight check — SIMD-accelerated pre-filter
                    if !text_scanner::quick_freight_check(&combined) {
                        continue;
                    }

                    // Full Aho-Corasick scan for confidence scoring
                    let scan_result = text_scanner::scan_text(&combined);

                    if scan_result.confidence < min_confidence {
                        continue;
                    }

                    // Dedup using entity name + file type as key.
                    // EDGAR filings have unique accession numbers but those
                    // aren't always in the search response, so we use what we have.
                    let dedup_key = format!("edgar:{}:{}", entity_name, file_type);

                    if !dedup.check_and_insert(&dedup_key) {
                        debug!(
                            entity = entity_name,
                            "EDGAR: duplicate filing — our Bloom filter remembers this one"
                        );
                        continue;
                    }

                    // Build the event
                    let company_name = if entity_name.is_empty() {
                        "Unknown Entity".to_string()
                    } else {
                        entity_name.to_string()
                    };

                    let mut event = BankruptcyEvent::new(
                        company_name,
                        Source::Edgar,
                        scan_result.confidence,
                    );
                    event.court = Some("SEC EDGAR".to_string());
                    event.chapter = detect_chapter(&combined);
                    event.classification = scan_result.classification;
                    event.source_url = Some(format!(
                        "https://www.sec.gov/cgi-bin/browse-edgar?company={}&CIK=&type={}&dateb=&owner=include&count=40&search_text=&action=getcompany",
                        urlencoding::encode(entity_name),
                        urlencoding::encode(file_type),
                    ));

                    // Parse filing date from EDGAR's file_date field
                    if let Some(date_str) = &source.file_date {
                        if let Ok(naive) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
                            event.filing_date = Some(naive.and_hms_opt(0, 0, 0).unwrap().and_utc());
                        }
                    }

                    // Try to extract DOT/MC numbers from the filing text
                    event.dot_number = extract_dot_number(&combined);
                    event.mc_number = extract_mc_number(&combined);

                    match event_tx.try_send(event) {
                        Ok(()) => {
                            new_events += 1;
                            info!(
                                entity = entity_name,
                                file_type = file_type,
                                confidence = format!("{:.1}%", scan_result.confidence * 100.0),
                                "EDGAR: SEC FILING DETECTED — {} filed a {} that smells like financial distress",
                                entity_name,
                                file_type
                            );
                        }
                        Err(e) => {
                            error!(error = %e, "EDGAR: failed to send event to channel");
                        }
                    }
                }

                if new_events > 0 {
                    info!(
                        new_events = new_events,
                        query = query,
                        "EDGAR scan cycle complete — {} new freight-related filings detected",
                        new_events
                    );
                }
            }

            _ = shutdown.changed() => {
                info!("EDGAR Scanner received shutdown signal — filing our final 8-K: 'Material Event: Scanner Termination'");
                break;
            }
        }
    }

    info!("EDGAR Scanner has exited — the SEC will miss our traffic");
}

// =============================================================================
// Helper Functions
// =============================================================================
// These functions extract structured data from the unstructured text soup
// that is SEC filing descriptions. It's like panning for gold in a river
// of legalese — tedious but occasionally rewarding.
// =============================================================================

/// Detect bankruptcy chapter from filing text.
///
/// SEC filings are generally more explicit about chapter numbers than
/// PACER filings because securities lawyers bill by the word and therefore
/// never abbreviate anything.
fn detect_chapter(text: &str) -> BankruptcyChapter {
    let upper = text.to_uppercase();
    if upper.contains("CHAPTER 7") || upper.contains("CHAPTER VII") {
        BankruptcyChapter::Chapter7
    } else if upper.contains("CHAPTER 11") || upper.contains("CHAPTER XI") {
        BankruptcyChapter::Chapter11
    } else if upper.contains("CHAPTER 13") || upper.contains("CHAPTER XIII") {
        BankruptcyChapter::Chapter13
    } else {
        BankruptcyChapter::Unknown
    }
}

/// Try to extract a DOT number from SEC filing text.
/// SEC filings occasionally reference USDOT numbers when the filer
/// is a motor carrier. Finding one is like finding a golden ticket
/// because it lets us cross-reference with FMCSA data.
fn extract_dot_number(text: &str) -> Option<String> {
    let upper = text.to_uppercase();
    let patterns = ["USDOT# ", "USDOT #", "USDOT ", "DOT# ", "DOT #", "DOT "];

    for pattern in patterns {
        if let Some(idx) = upper.find(pattern) {
            let start = idx + pattern.len();
            let num: String = upper[start..]
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if !num.is_empty() && num.len() <= 8 {
                return Some(num);
            }
        }
    }
    None
}

/// Try to extract an MC number from SEC filing text.
/// Same deal as DOT numbers — rare but valuable when found.
fn extract_mc_number(text: &str) -> Option<String> {
    let upper = text.to_uppercase();
    let patterns = ["MC# ", "MC #", "MC-", "MC "];

    for pattern in patterns {
        if let Some(idx) = upper.find(pattern) {
            let start = idx + pattern.len();
            let num: String = upper[start..]
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if !num.is_empty() && num.len() <= 7 {
                return Some(num);
            }
        }
    }
    None
}
