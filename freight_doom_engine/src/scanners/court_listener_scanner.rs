// =============================================================================
// court_listener_scanner.rs — THE FREE LAW PROJECT'S BIGGEST FAN
// =============================================================================
//
// CourtListener is a free, open-source platform maintained by the Free Law
// Project. It aggregates court opinions, oral arguments, and RECAP docket
// data from federal courts across the United States. It's basically the
// Wikipedia of American jurisprudence, except it's actually accurate.
//
// Real API: https://www.courtlistener.com/api/rest/v3/
// Docs:     https://www.courtlistener.com/help/api/rest/
//
// The API is free to use (no API key required for basic searches), though
// they do rate-limit to around 100 requests per day for unauthenticated
// users. Since we're a good citizen of the internet (and because they're
// a non-profit doing God's work), we poll conservatively — every 45 seconds
// by default.
//
// We search CourtListener's RECAP archive (type=r) for docket entries
// mentioning both bankruptcy and freight/logistics keywords. The RECAP
// archive is particularly valuable because it contains actual docket entries
// from PACER, uploaded by the RECAP browser extension. This means we get
// PACER data without paying PACER prices. It's like having a friend with
// a Costco membership — you get the bulk pricing without the annual fee.
//
// We rotate through 10 search queries to cover different keyword
// combinations. Each query targets a different intersection of bankruptcy
// terminology and logistics jargon. "bankruptcy freight carrier" catches
// the obvious ones, while "insolvency third party logistics" catches the
// more subtle ones. Together, they form a dragnet so comprehensive that
// even a freight forwarder filing Chapter 13 in rural Montana couldn't
// slip through.
//
// Is using a non-profit's free court opinion API to build a SIMD-powered
// freight bankruptcy detection engine with bloom filter deduplication and
// circuit breaker resilience a reasonable thing to do? The Free Law Project
// says their mission is to make legal data freely available. We're making
// it freely available... to our Redis pub/sub channel. Same energy.
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
    BankruptcyChapter, BankruptcyEvent, CourtListenerResult, Source,
};
use crate::text_scanner;

// =============================================================================
// CourtListener Search Queries
// =============================================================================
// Each query is designed to catch a different slice of the freight bankruptcy
// universe. We rotate through them because the CourtListener search API
// returns at most 20 results per query, and we don't want to miss anything.
//
// The query design philosophy:
// - Always include a bankruptcy-related term (bankruptcy, chapter, insolvency)
// - Always include a freight/logistics term (freight, carrier, trucking, etc.)
// - Vary the specificity to catch both broad and narrow matches
// - Include some industry-specific terms (drayage, intermodal, LTL) that
//   would only appear in filings involving actual freight companies
//
// The result: a query rotation that covers everything from "big trucking
// company files Chapter 11" to "small drayage operator in Chapter 7."
// =============================================================================
const CL_QUERIES: &[&str] = &[
    "bankruptcy freight carrier",
    "bankruptcy trucking logistics",
    "chapter 11 freight broker",
    "chapter 7 carrier transportation",
    "bankruptcy motor carrier",
    "insolvency third party logistics",
    "bankruptcy intermodal freight",
    "chapter 11 less than truckload",
    "bankruptcy drayage carrier",
    "chapter 7 freight forwarder",
];

/// The main entry point for the CourtListener scanner.
///
/// This function loops forever, searching CourtListener's REST API for
/// bankruptcy docket entries mentioning freight/logistics companies.
/// It's like having a law clerk who only reads bankruptcy dockets and
/// only cares about trucking companies, except this clerk works 24/7,
/// never takes a coffee break, and has SIMD-accelerated reading skills.
///
/// # Arguments
/// * `config` - Global configuration with court_listener_base_url and
///   court_listener_poll_interval.
/// * `event_tx` - Crossbeam channel sender for bankruptcy events.
/// * `dedup` - Bloom filter + LRU deduplication engine.
/// * `shutdown` - Watch channel for graceful shutdown.
pub async fn run(
    config: Arc<Config>,
    event_tx: Sender<BankruptcyEvent>,
    dedup: Arc<DedupEngine>,
    shutdown: &mut watch::Receiver<bool>,
) {
    info!("CourtListener Scanner initializing — preparing to mine the Free Law Project's data like a legal archaeologist with a mission");

    // Build HTTP client with a polite User-Agent.
    // CourtListener is run by the Free Law Project, a non-profit.
    // We identify ourselves clearly so they know we're using their
    // data for the noble cause of tracking freight company bankruptcy.
    // They'd probably approve. Probably.
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent("FreightDoomEngine/1.0 (legal-research@freight-doom.dev; educational-project)")
        .build()
        .expect("Failed to build CourtListener HTTP client — the Free Law Project deserved better from us");

    // Circuit breaker for CourtListener.
    // They're a non-profit with limited infrastructure. When their servers
    // struggle, we back off immediately because we're not monsters.
    // Well, we ARE building an overkill bankruptcy detection engine,
    // but at least we're polite about our API usage.
    let circuit_breaker = CircuitBreaker::new(
        "CourtListener",
        config.circuit_breaker_failure_threshold,
        config.circuit_breaker_reset_timeout,
        config.circuit_breaker_success_threshold,
    );

    // Atomic counter for rotating through search queries.
    let query_index = AtomicUsize::new(0);

    let poll_interval = config.court_listener_poll_interval;
    let base_url = config.court_listener_base_url.clone();
    let min_confidence = config.min_confidence_threshold;

    info!(
        poll_interval_secs = poll_interval.as_secs(),
        base_url = base_url.as_str(),
        queries = CL_QUERIES.len(),
        "CourtListener Scanner online — respectfully pillaging open legal data for signs of freight industry collapse"
    );

    loop {
        tokio::select! {
            _ = tokio::time::sleep(poll_interval) => {
                if !circuit_breaker.allow_request() {
                    debug!("CourtListener: circuit breaker is OPEN — giving the non-profit's servers some rest");
                    continue;
                }

                // Rotate through search queries.
                // With 10 queries and a 45-second interval, we complete
                // a full rotation every 7.5 minutes. This gives us
                // comprehensive coverage without overwhelming CourtListener's
                // rate limits.
                let idx = query_index.fetch_add(1, Ordering::Relaxed) % CL_QUERIES.len();
                let query = CL_QUERIES[idx];

                // Build the CourtListener search API URL.
                // We use type=r (RECAP/dockets) to search actual court filings.
                // type=o (opinions) would give us judicial opinions, which are
                // useful but come much later in the process. We want filings
                // because they show up first.
                //
                // The filed_after parameter limits results to today's filings,
                // keeping the data fresh and the response size manageable.
                // order_by=dateFiled+desc gives us newest first.
                let today = Utc::now().format("%Y-%m-%d").to_string();
                let url = format!(
                    "{}/search/?q={}&type=r&filed_after={}&order_by=dateFiled+desc&format=json",
                    base_url,
                    urlencoding::encode(query),
                    today,
                );

                debug!(
                    query = query,
                    date = today.as_str(),
                    "CourtListener: searching RECAP dockets — query {}/{}: '{}'",
                    idx + 1,
                    CL_QUERIES.len(),
                    query
                );

                // Make the request. CourtListener is generally responsive
                // but can be slow during high-traffic periods (like when
                // a major case drops and every law student in America
                // tries to read it simultaneously).
                let response = match client.get(&url).send().await {
                    Ok(resp) => resp,
                    Err(e) => {
                        circuit_breaker.record_failure();
                        warn!(
                            error = %e,
                            query = query,
                            "CourtListener: request failed — the Free Law Project's servers are taking a personal day"
                        );
                        continue;
                    }
                };

                let status = response.status();
                if !status.is_success() {
                    if status.as_u16() == 429 {
                        // Rate limited. We expected this eventually.
                        // CourtListener allows ~100 requests/day for
                        // unauthenticated users. We're being told to chill.
                        warn!(
                            "CourtListener: rate limited (HTTP 429) — we've been too enthusiastic, backing off"
                        );
                        circuit_breaker.record_failure();
                    } else {
                        debug!(
                            "CourtListener: non-success HTTP status: {} — the legal data will have to wait",
                            status
                        );
                    }
                    continue;
                }

                circuit_breaker.record_success();

                let body = match response.text().await {
                    Ok(b) => b,
                    Err(e) => {
                        debug!(error = %e, "CourtListener: failed to read response body");
                        continue;
                    }
                };

                // Parse the response using the CourtListenerResult types
                // from models.rs. The API returns:
                // { count: N, results: [...], next: "url_to_next_page" }
                let search_result: CourtListenerResult = match serde_json::from_str(&body) {
                    Ok(r) => r,
                    Err(e) => {
                        debug!(
                            error = %e,
                            "CourtListener: JSON parse error — they might have changed their API format, which would be very unlike them"
                        );
                        continue;
                    }
                };

                let total_count = search_result.count.unwrap_or(0);

                if total_count > 0 {
                    debug!(
                        count = total_count,
                        query = query,
                        "CourtListener: {} results — scanning for freight companies in legal peril",
                        total_count
                    );
                }

                let results = match &search_result.results {
                    Some(r) => r,
                    None => continue,
                };

                let mut new_events = 0u64;

                for opinion in results {
                    // Combine all available text fields for scanning.
                    // CourtListener results have:
                    // - case_name: "Acme Freight LLC v. Everyone"
                    // - snippet: "...Chapter 11 bankruptcy filing by motor carrier..."
                    // - court: "United States Bankruptcy Court for the District of Delaware"
                    let case_name = opinion.case_name.as_deref().unwrap_or("");
                    let snippet = opinion.snippet.as_deref().unwrap_or("");
                    let court_name = opinion.court.as_deref().unwrap_or("");

                    let combined = format!("{} {} {}", case_name, snippet, court_name);

                    // Quick freight check — SIMD-accelerated pre-filter.
                    // If none of our freight keywords appear, skip immediately.
                    // memchr-powered byte scanning means this check is nearly free.
                    if !text_scanner::quick_freight_check(&combined) {
                        continue;
                    }

                    // Full Aho-Corasick scan for confidence scoring and classification.
                    // This runs ALL keywords simultaneously in a single pass.
                    // O(n + m) time complexity. Overkill? Absolutely. Effective? Also absolutely.
                    let scan_result = text_scanner::scan_text(&combined);

                    if scan_result.confidence < min_confidence {
                        continue;
                    }

                    // Dedup using CourtListener result ID + case name.
                    // Each CourtListener result has a unique numeric ID,
                    // which is perfect for deduplication.
                    let cl_id = opinion.id.unwrap_or(0);
                    let dedup_key = format!("cl:{}:{}", cl_id, case_name);

                    if !dedup.check_and_insert(&dedup_key) {
                        debug!(
                            case = case_name,
                            "CourtListener: duplicate case — already in our Bloom filter"
                        );
                        continue;
                    }

                    // Build the bankruptcy event using the constructor
                    let company_name = if case_name.is_empty() {
                        "Unknown Case".to_string()
                    } else {
                        // CourtListener case names often look like:
                        // "In re: Acme Freight LLC" or "Acme v. Creditors"
                        // We try to extract just the company name.
                        extract_company_from_case_name(case_name)
                    };

                    let mut event = BankruptcyEvent::new(
                        company_name,
                        Source::CourtListener,
                        scan_result.confidence,
                    );
                    event.court = if court_name.is_empty() {
                        None
                    } else {
                        Some(court_name.to_string())
                    };
                    event.chapter = detect_chapter(&combined);
                    event.classification = scan_result.classification;

                    // Build source URL from CourtListener's absolute_url field
                    event.source_url = opinion
                        .absolute_url
                        .as_ref()
                        .map(|path| format!("https://www.courtlistener.com{}", path));

                    // Parse filing date
                    if let Some(date_str) = &opinion.date_filed {
                        if let Ok(naive) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
                            event.filing_date = Some(
                                naive.and_hms_opt(0, 0, 0).unwrap().and_utc()
                            );
                        }
                    }

                    // Try to extract DOT/MC numbers from the combined text
                    event.dot_number = extract_dot_number(&combined);
                    event.mc_number = extract_mc_number(&combined);

                    match event_tx.try_send(event) {
                        Ok(()) => {
                            new_events += 1;
                            info!(
                                case = case_name,
                                court = court_name,
                                confidence = format!("{:.1}%", scan_result.confidence * 100.0),
                                keywords = scan_result.matched_keywords.len(),
                                "CourtListener: BANKRUPTCY CASE DETECTED — '{}' filed in {} — our dragnet strikes again",
                                case_name,
                                court_name
                            );
                        }
                        Err(e) => {
                            error!(
                                error = %e,
                                "CourtListener: failed to send event to channel"
                            );
                        }
                    }
                }

                if new_events > 0 {
                    info!(
                        new_events = new_events,
                        query = query,
                        "CourtListener scan cycle complete — {} new freight bankruptcy cases discovered in the RECAP archive",
                        new_events
                    );
                }
            }

            _ = shutdown.changed() => {
                info!("CourtListener Scanner received shutdown signal — our pro bono legal research has concluded");
                break;
            }
        }
    }

    info!("CourtListener Scanner has exited — the Free Law Project continues without us");
}

// =============================================================================
// Helper Functions
// =============================================================================
// These functions extract useful information from CourtListener result data.
// Court case data is surprisingly messy — case names follow no consistent
// format, dates appear in multiple formats, and DOT numbers might be buried
// in a 50-page filing snippet. We do our best.
// =============================================================================

/// Extract a company name from a CourtListener case name.
///
/// Case names in bankruptcy courts follow patterns like:
///   "In re: Acme Freight LLC"
///   "In the Matter of Big Truck Corp."
///   "Acme Logistics, Inc., Debtor"
///   "First Bank v. Acme Carrier Services"
///
/// We try to extract the debtor's name using common patterns.
/// If all else fails, we return the entire case name, because
/// a messy name is better than no name.
fn extract_company_from_case_name(case_name: &str) -> String {
    let lower = case_name.to_lowercase();

    // "In re: Company Name" or "In re Company Name"
    if let Some(idx) = lower.find("in re:") {
        return case_name[idx + 6..].trim().to_string();
    }
    if let Some(idx) = lower.find("in re ") {
        return case_name[idx + 6..].trim().to_string();
    }

    // "In the Matter of Company Name"
    if let Some(idx) = lower.find("in the matter of") {
        return case_name[idx + 17..].trim().to_string();
    }

    // "Company Name, Debtor" — strip the ", Debtor" suffix
    if let Some(idx) = lower.find(", debtor") {
        return case_name[..idx].trim().to_string();
    }

    // For "A v. B" cases, take the first party (often the debtor in bankruptcy)
    if let Some(idx) = lower.find(" v. ") {
        return case_name[..idx].trim().to_string();
    }
    if let Some(idx) = lower.find(" vs. ") {
        return case_name[..idx].trim().to_string();
    }

    // Give up and return the whole thing
    case_name.to_string()
}

/// Detect bankruptcy chapter from court filing text.
///
/// CourtListener snippets usually contain explicit chapter references
/// because that's kind of the whole point of a bankruptcy filing.
fn detect_chapter(text: &str) -> BankruptcyChapter {
    let upper = text.to_uppercase();
    if upper.contains("CHAPTER 7") || upper.contains("CH. 7") || upper.contains("CH 7") {
        BankruptcyChapter::Chapter7
    } else if upper.contains("CHAPTER 11") || upper.contains("CH. 11") || upper.contains("CH 11") {
        BankruptcyChapter::Chapter11
    } else if upper.contains("CHAPTER 13") || upper.contains("CH. 13") || upper.contains("CH 13") {
        BankruptcyChapter::Chapter13
    } else {
        BankruptcyChapter::Unknown
    }
}

/// Try to extract a DOT number from court filing text.
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

/// Try to extract an MC number from court filing text.
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
