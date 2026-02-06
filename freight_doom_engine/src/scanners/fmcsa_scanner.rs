// =============================================================================
// fmcsa_scanner.rs — THE TRUCKING INDUSTRY DEATH WATCH
// =============================================================================
//
// FMCSA (Federal Motor Carrier Safety Administration) maintains the SAFER
// (Safety and Fitness Electronic Records) system and the QCMobile API.
// These are the definitive databases of every motor carrier, freight broker,
// and freight forwarder operating in the United States.
//
// When a trucking company goes bankrupt, one of the first things that happens
// is their operating authority gets revoked or goes inactive. Insurance lapses.
// Out-of-service orders pile up. The FMCSA database reflects all of this,
// often before the bankruptcy filing even hits PACER.
//
// Real API endpoints:
//   QCMobile API: https://mobile.fmcsa.dot.gov/qc/services/carriers/{DOT_NUMBER}
//   SAFER Web:    https://safer.fmcsa.dot.gov/query.asp
//   SMS Data:     https://ai.fmcsa.dot.gov/SMS/Carrier/{DOT_NUMBER}
//
// We maintain a watchlist of known freight carriers and poll the QCMobile API
// for each one, checking for status changes. When a carrier goes from "ACTIVE"
// to "INACTIVE" or "REVOKED," we generate a bankruptcy event with high
// confidence because there are really only two reasons a carrier's authority
// gets revoked: 1) they went bankrupt, or 2) they committed enough safety
// violations to make the government take their keys away. Either way,
// it's newsworthy.
//
// We also check for insurance lapses, because a carrier that lets its
// insurance lapse is a carrier that's either bankrupt or about to be.
// Insurance companies don't cancel policies on carriers that can pay their
// premiums. That's not how capitalism works.
//
// Is monitoring individual DOT numbers via a government API to detect
// status changes that might indicate bankruptcy an appropriate use of
// async Rust with circuit breakers and bloom filter deduplication?
// The question answers itself.
// =============================================================================

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crossbeam_channel::Sender;
use tokio::sync::watch;
use tracing::{debug, error, info, warn};

use crate::circuit_breaker::CircuitBreaker;
use crate::config::Config;
use crate::dedup::DedupEngine;
use crate::models::{
    BankruptcyChapter, BankruptcyEvent, CompanyClassification, Source,
};
use crate::text_scanner;

// =============================================================================
// Monitored Carrier DOT Numbers
// =============================================================================
// These are REAL USDOT numbers of major freight carriers and brokers.
// In a production system, this list would be loaded from a database and
// contain thousands of entries. For our purposes, we monitor a curated
// list of well-known carriers so we can detect when the big ones go down.
//
// Each entry is (DOT_NUMBER, COMPANY_NAME). The company name is for logging
// and fallback purposes — we always prefer the name from the API response
// because FMCSA knows better than us what a company is called.
//
// Fun fact: every single one of these companies has been through at least
// one near-death financial experience. The freight industry is basically
// a continuous cycle of "things are great" and "we're all going to die."
// =============================================================================
const MONITORED_CARRIERS: &[(&str, &str)] = &[
    ("2247208", "XPO Logistics"),
    ("2222636", "Echo Global Logistics"),
    ("2209198", "Coyote Logistics"),
    ("1018962", "Werner Enterprises"),
    ("125100",  "JB Hunt Transport"),
    ("69643",   "Schneider National"),
    ("122098",  "Heartland Express"),
    ("298894",  "Swift Transportation"),
    ("1065988", "USA Truck"),
    ("2239788", "Convoy Inc"),
    ("624957",  "Old Dominion Freight Line"),
    ("2016493", "TForce Freight"),
    ("354113",  "ABF Freight System"),
    ("584586",  "Estes Express Lines"),
    ("259823",  "Southeastern Freight Lines"),
];

/// FMCSA QCMobile API carrier response wrapper.
///
/// The QCMobile API wraps everything in a { content: { carrier: { ... } } }
/// structure because apparently one level of nesting wasn't enough.
/// We define our own deserialization types here because the FmcsaCarrierRecord
/// in models.rs is the flattened version we ultimately work with.
#[derive(Debug, serde::Deserialize)]
struct QcMobileResponse {
    content: Option<QcMobileContent>,
}

#[derive(Debug, serde::Deserialize)]
struct QcMobileContent {
    carrier: Option<QcMobileCarrier>,
}

/// The actual carrier data from the QCMobile API.
/// Field names use camelCase because the API was designed by people who
/// apparently prefer JavaScript naming conventions in their government JSON.
#[derive(Debug, serde::Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct QcMobileCarrier {
    legal_name: Option<String>,
    dba_name: Option<String>,
    dot_number: Option<String>,
    mc_number: Option<String>,
    carrier_operation: Option<String>,
    status_code: Option<String>,
    oos_date: Option<String>,
    #[serde(alias = "bipd_insurance_required")]
    insurance_required: Option<String>,
    #[serde(alias = "bipd_insurance_on_file")]
    insurance_on_file: Option<String>,
    phy_city: Option<String>,
    phy_state: Option<String>,
    total_drivers: Option<String>,
    total_power_units: Option<String>,
}

/// The main entry point for the FMCSA scanner.
///
/// This function runs an infinite loop, rotating through monitored carriers
/// and checking their status with the FMCSA QCMobile API. It's like having
/// a fleet manager who does nothing but refresh the SAFER website all day,
/// except this fleet manager is an async Rust function with a circuit breaker
/// and can check 15 carriers in the time it takes a human to load one page.
///
/// # Arguments
/// * `config` - Global configuration with fmcsa_base_url and fmcsa_poll_interval.
/// * `event_tx` - Crossbeam channel sender for detected events.
/// * `dedup` - Bloom filter + LRU deduplication engine.
/// * `shutdown` - Watch channel for graceful shutdown.
pub async fn run(
    config: Arc<Config>,
    event_tx: Sender<BankruptcyEvent>,
    dedup: Arc<DedupEngine>,
    shutdown: &mut watch::Receiver<bool>,
) {
    info!("FMCSA Scanner initializing — preparing to stalk the operating authority status of every major carrier in America");

    // Build HTTP client. FMCSA doesn't have strict User-Agent requirements
    // like the SEC, but we identify ourselves anyway because we were raised right.
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent("FreightDoomEngine/1.0 (carrier-monitoring; educational-project)")
        .build()
        .expect("Failed to build FMCSA HTTP client — the Department of Transportation will never know");

    // Circuit breaker for FMCSA endpoints.
    // FMCSA APIs can be temperamental, especially the QCMobile endpoint
    // which occasionally decides that HTTP 500 is an acceptable response
    // to a perfectly valid request.
    let circuit_breaker = CircuitBreaker::new(
        "FMCSA",
        config.circuit_breaker_failure_threshold,
        config.circuit_breaker_reset_timeout,
        config.circuit_breaker_success_threshold,
    );

    // Atomic index for rotating through the carrier watchlist.
    // We check a batch of 3 carriers per cycle to spread the load
    // and avoid hammering FMCSA with 15 simultaneous requests.
    // We're obsessive, not rude.
    let carrier_index = AtomicUsize::new(0);

    let poll_interval = config.fmcsa_poll_interval;
    let fmcsa_base_url = config.fmcsa_base_url.clone();
    let min_confidence = config.min_confidence_threshold;

    info!(
        poll_interval_secs = poll_interval.as_secs(),
        monitored_carriers = MONITORED_CARRIERS.len(),
        base_url = fmcsa_base_url.as_str(),
        "FMCSA Scanner online — monitoring {} carriers like a very concerned insurance adjuster",
        MONITORED_CARRIERS.len()
    );

    loop {
        tokio::select! {
            _ = tokio::time::sleep(poll_interval) => {
                if !circuit_breaker.allow_request() {
                    debug!("FMCSA: circuit breaker is OPEN — FMCSA needs time to recover from our affection");
                    continue;
                }

                // Check a batch of carriers per cycle.
                // We rotate through the list so every carrier gets checked
                // eventually. With 15 carriers and batches of 3, we check
                // the entire list every 5 cycles. At a 120-second interval,
                // that's a full sweep every 10 minutes.
                //
                // In a production system with thousands of carriers, you'd
                // want much larger batches and possibly parallel requests.
                // But for 15 carriers, sequential is fine and much friendlier
                // to FMCSA's servers.
                let batch_size = 3;
                let start_idx = carrier_index.fetch_add(batch_size, Ordering::Relaxed);

                for i in 0..batch_size {
                    let idx = (start_idx + i) % MONITORED_CARRIERS.len();
                    let (dot_number, fallback_name) = MONITORED_CARRIERS[idx];

                    check_carrier(
                        &client,
                        &circuit_breaker,
                        &fmcsa_base_url,
                        dot_number,
                        fallback_name,
                        &event_tx,
                        &dedup,
                        min_confidence,
                    )
                    .await;
                }
            }

            _ = shutdown.changed() => {
                info!("FMCSA Scanner received shutdown signal — our operating authority has been voluntarily revoked");
                break;
            }
        }
    }

    info!("FMCSA Scanner has exited — the carriers are on their own now");
}

/// Check a single carrier's status via the FMCSA QCMobile API.
///
/// This function hits the QCMobile API for a specific DOT number, parses
/// the response, and evaluates whether the carrier's status indicates
/// financial distress (INACTIVE, REVOKED, OUT OF SERVICE, insurance lapse).
///
/// If a carrier's status looks bad, we generate a BankruptcyEvent and
/// fire it into the crossbeam channel. The confidence score is based on
/// how bad the status is:
/// - REVOKED: 0.90 confidence (this is pretty definitive)
/// - INACTIVE: 0.80 confidence (could be voluntary, could be bad)
/// - OUT OF SERVICE: 0.85 confidence (the government took their keys)
/// - Insurance lapsed: 0.70 confidence (the death spiral has begun)
///
/// We also run the carrier's name through the text scanner to classify
/// their operation type (carrier vs broker vs 3PL vs freight forwarder).
async fn check_carrier(
    client: &reqwest::Client,
    circuit_breaker: &CircuitBreaker,
    base_url: &str,
    dot_number: &str,
    fallback_name: &str,
    event_tx: &Sender<BankruptcyEvent>,
    dedup: &Arc<DedupEngine>,
    min_confidence: f64,
) {
    // Build the QCMobile API URL.
    // The real endpoint is: https://mobile.fmcsa.dot.gov/qc/services/carriers/{DOT}
    // It returns JSON with the carrier's full registration details.
    let url = format!("{}/{}", base_url, dot_number);

    debug!(
        dot_number = dot_number,
        carrier = fallback_name,
        "FMCSA: checking carrier status — praying for ACTIVE, bracing for REVOKED"
    );

    let response = match client
        .get(&url)
        .header("Accept", "application/json")
        .send()
        .await
    {
        Ok(resp) => {
            circuit_breaker.record_success();
            resp
        }
        Err(e) => {
            circuit_breaker.record_failure();
            debug!(
                dot_number = dot_number,
                error = %e,
                "FMCSA: failed to fetch carrier data — the DOT's servers are napping"
            );
            return;
        }
    };

    if !response.status().is_success() {
        debug!(
            dot_number = dot_number,
            status = %response.status(),
            "FMCSA: non-success response for DOT# {} — carrier may not exist or API is grumpy",
            dot_number
        );
        return;
    }

    let body = match response.text().await {
        Ok(b) => b,
        Err(e) => {
            debug!(error = %e, "FMCSA: failed to read response body");
            return;
        }
    };

    // Try to parse the QCMobile JSON response.
    // The API wraps carrier data in { content: { carrier: { ... } } }
    // because simplicity is the enemy of government API design.
    let qc_response: QcMobileResponse = match serde_json::from_str(&body) {
        Ok(r) => r,
        Err(_) => {
            // If JSON parsing fails, try scanning the raw text.
            // Sometimes the API returns HTML or XML instead of JSON
            // because consistency is overrated.
            scan_raw_carrier_text(&body, dot_number, fallback_name, event_tx, dedup, min_confidence);
            return;
        }
    };

    // Extract the carrier record from the nested response
    let carrier = match qc_response
        .content
        .and_then(|c| c.carrier)
    {
        Some(c) => c,
        None => {
            debug!(
                dot_number = dot_number,
                "FMCSA: no carrier data in response for DOT# {} — carrier might be a ghost",
                dot_number
            );
            return;
        }
    };

    // Determine the carrier's display name
    let carrier_name = carrier
        .legal_name
        .as_deref()
        .filter(|n| !n.is_empty())
        .or(carrier.dba_name.as_deref().filter(|n| !n.is_empty()))
        .unwrap_or(fallback_name);

    let status = carrier
        .status_code
        .as_deref()
        .unwrap_or("")
        .to_uppercase();

    // =========================================================================
    // THE DEATH SIGNAL EVALUATION
    // =========================================================================
    // We check for several indicators that a carrier is in financial distress:
    //
    // 1. Authority status: INACTIVE, REVOKED, NOT AUTHORIZED
    //    These are the big ones. If FMCSA says you can't operate, you can't operate.
    //
    // 2. Out-of-service date present
    //    If there's an OOS date, something went very wrong.
    //
    // 3. Insurance lapse
    //    Required insurance on file but nothing actually filed.
    //    Insurance companies pull coverage when premiums aren't paid.
    //    Premiums aren't paid when there's no money.
    //    There's no money when... well, you can see where this is going.
    // =========================================================================

    let is_status_dead = status == "INACTIVE"
        || status == "REVOKED"
        || status == "OUT OF SERVICE"
        || status == "NOT AUTHORIZED";

    let has_oos_date = carrier
        .oos_date
        .as_deref()
        .map(|d| !d.is_empty())
        .unwrap_or(false);

    let insurance_lapsed = carrier
        .insurance_required
        .as_deref()
        .map(|r| r.to_uppercase() == "Y")
        .unwrap_or(false)
        && carrier
            .insurance_on_file
            .as_deref()
            .map(|f| f.to_uppercase() == "N" || f.is_empty())
            .unwrap_or(true);

    if !is_status_dead && !has_oos_date && !insurance_lapsed {
        // Carrier is fine. Status is ACTIVE, insurance is current.
        // Nothing to see here. Move along. The trucks are still rolling.
        debug!(
            dot_number = dot_number,
            carrier = carrier_name,
            status = status.as_str(),
            "FMCSA: {} is ACTIVE — still trucking along",
            carrier_name
        );
        return;
    }

    // Something is wrong. Build a dedup key and check if we've already reported this.
    let dedup_key = format!("fmcsa:{}:{}", dot_number, status);

    if !dedup.check_and_insert(&dedup_key) {
        debug!(
            dot_number = dot_number,
            "FMCSA: already reported status change for DOT# {} — our Bloom filter has a good memory",
            dot_number
        );
        return;
    }

    // Calculate confidence score based on the type of death signal.
    let confidence = if is_status_dead {
        match status.as_str() {
            "REVOKED" => 0.90,
            "OUT OF SERVICE" => 0.85,
            "INACTIVE" => 0.80,
            "NOT AUTHORIZED" => 0.85,
            _ => 0.75,
        }
    } else if insurance_lapsed {
        0.70
    } else {
        0.65
    };

    if confidence < min_confidence {
        return;
    }

    // Classify the carrier operation type
    let classification = classify_carrier_operation(
        carrier.carrier_operation.as_deref().unwrap_or(""),
    );

    // Build the bankruptcy event
    let mut event = BankruptcyEvent::new(
        carrier_name.to_string(),
        Source::Fmcsa,
        confidence,
    );
    event.dot_number = Some(dot_number.to_string());
    event.mc_number = carrier.mc_number.clone().filter(|mc| !mc.is_empty());
    event.chapter = BankruptcyChapter::Unknown; // FMCSA doesn't know about chapters
    event.classification = classification;
    event.source_url = Some(format!(
        "https://safer.fmcsa.dot.gov/query.asp?searchtype=ANY&query_type=queryCarrierSnapshot&query_param=USDOT&query_string={}",
        dot_number
    ));
    event.court = Some(format!(
        "FMCSA — Status: {} | {}",
        status,
        if insurance_lapsed { "INSURANCE LAPSED" } else { "Authority Change" }
    ));

    // Build a rich description for logging
    let city = carrier.phy_city.as_deref().unwrap_or("Unknown");
    let state = carrier.phy_state.as_deref().unwrap_or("??");
    let drivers = carrier.total_drivers.as_deref().unwrap_or("?");
    let units = carrier.total_power_units.as_deref().unwrap_or("?");

    match event_tx.try_send(event) {
        Ok(()) => {
            info!(
                dot_number = dot_number,
                carrier = carrier_name,
                status = status.as_str(),
                city = city,
                state = state,
                drivers = drivers,
                power_units = units,
                confidence = format!("{:.1}%", confidence * 100.0),
                "FMCSA: CARRIER STATUS CHANGE DETECTED — {} (DOT# {}) is now {} — {} drivers, {} power units, based in {}, {}",
                carrier_name, dot_number, status, drivers, units, city, state
            );
        }
        Err(e) => {
            error!(
                error = %e,
                dot_number = dot_number,
                "FMCSA: failed to send event to channel — the bankruptcy news will have to wait"
            );
        }
    }
}

/// Fallback: scan raw response text when JSON parsing fails.
///
/// Sometimes the FMCSA API returns HTML or XML instead of JSON,
/// because government API consistency is a myth. In those cases,
/// we do a raw text scan for status keywords. It's not as reliable
/// as proper JSON parsing, but it's better than nothing.
fn scan_raw_carrier_text(
    text: &str,
    dot_number: &str,
    fallback_name: &str,
    event_tx: &Sender<BankruptcyEvent>,
    dedup: &Arc<DedupEngine>,
    min_confidence: f64,
) {
    // First check if this text is even about freight/logistics
    if !text_scanner::quick_freight_check(text) {
        return;
    }

    let scan_result = text_scanner::scan_text(text);
    if scan_result.confidence < min_confidence {
        return;
    }

    let upper = text.to_uppercase();
    let has_death_signal = upper.contains("REVOKED")
        || upper.contains("INACTIVE")
        || upper.contains("OUT OF SERVICE")
        || upper.contains("NOT AUTHORIZED");

    if !has_death_signal {
        return;
    }

    let dedup_key = format!("fmcsa:raw:{}", dot_number);
    if !dedup.check_and_insert(&dedup_key) {
        return;
    }

    let mut event = BankruptcyEvent::new(
        fallback_name.to_string(),
        Source::Fmcsa,
        scan_result.confidence,
    );
    event.dot_number = Some(dot_number.to_string());
    event.classification = scan_result.classification;
    event.court = Some("FMCSA (raw text parse)".to_string());
    event.source_url = Some(format!(
        "https://safer.fmcsa.dot.gov/query.asp?searchtype=ANY&query_type=queryCarrierSnapshot&query_param=USDOT&query_string={}",
        dot_number
    ));

    if let Err(e) = event_tx.try_send(event) {
        error!(error = %e, "FMCSA: failed to send raw-text event");
    } else {
        warn!(
            dot_number = dot_number,
            carrier = fallback_name,
            "FMCSA: raw text indicates status change for DOT# {} — parsed from non-JSON response like a true detective",
            dot_number
        );
    }
}

/// Classify a carrier's operation type based on FMCSA's carrier_operation field.
///
/// FMCSA categorizes carriers into operation types like "Interstate" or
/// "Intrastate" and broker/carrier designations. We map these to our
/// CompanyClassification enum, which the Rails app uses to categorize
/// bankruptcies. Because even in death, we must be organized.
fn classify_carrier_operation(operation: &str) -> CompanyClassification {
    let upper = operation.to_uppercase();
    if upper.contains("BROKER") {
        CompanyClassification::Broker
    } else if upper.contains("FREIGHT FORWARDER") || upper.contains("FORWARDER") {
        CompanyClassification::FreightForwarder
    } else if upper.contains("CARRIER") || upper.contains("MOTOR") || upper.contains("INTERSTATE") {
        CompanyClassification::Carrier
    } else {
        // Default to Carrier because most FMCSA-registered entities are carriers.
        // It's like defaulting to "truck" when you're not sure what vehicle
        // someone is talking about — you'll be right more often than not.
        CompanyClassification::Carrier
    }
}
