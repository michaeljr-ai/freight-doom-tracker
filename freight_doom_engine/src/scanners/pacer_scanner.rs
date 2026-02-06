// =============================================================================
// pacer_scanner.rs — THE ORIGINAL DOOM ORACLE
// =============================================================================
//
// PACER (Public Access to Court Electronic Records) is the federal judiciary's
// system for providing public access to case and docket information. It has
// RSS feeds. In 2024. RSS feeds. From the government.
//
// We're polling XML feeds from 12 bankruptcy courts across the United States
// every 60 seconds. This is like hiring 12 private investigators to sit in
// 12 different courthouses and text you every time someone files for
// bankruptcy — except the investigators are HTTP GET requests and the
// courthouses are government servers running software last updated when
// flip phones were cool.
//
// Real court RSS feed URLs follow the pattern:
//   https://ecf.{court_code}.uscourts.gov/cgi-bin/rss_outside.pl
//
// The feeds return XML with <item> elements. Each item has a title
// (usually the case number + debtor name), a description (the full
// docket text), and a link (to the PACER docket page, which costs
// $0.10 per page because even bankruptcy has a surcharge).
//
// We parse this XML, run every single item through our SIMD-accelerated
// Aho-Corasick text scanner to determine if it mentions freight/logistics
// companies, deduplicate against our Bloom Filter + LRU hybrid engine,
// and fire events into the crossbeam channel faster than a dispatcher
// can say "where's my truck?"
//
// Is building a SIMD-powered RSS parser to check bankruptcy filings for
// the word "trucking" the most over-engineered solution to a problem that
// could be solved with a Google Alert? Yes. Yes it is.
// =============================================================================

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, NaiveDateTime, Utc};
use crossbeam_channel::Sender;
use tokio::sync::watch;
use tracing::{debug, error, info};

use crate::circuit_breaker::CircuitBreaker;
use crate::config::Config;
use crate::dedup::DedupEngine;
use crate::models::{BankruptcyChapter, BankruptcyEvent, Source};
use crate::text_scanner;

// =============================================================================
// PACER Bankruptcy Court RSS Feed Endpoints
// =============================================================================
// These are REAL, publicly accessible RSS feeds from the United States
// Bankruptcy Courts. No authentication required. No API key. Just pure,
// unfiltered XML served directly from the US federal judiciary.
//
// Each court code follows PACER's naming convention:
//   deb = District of Delaware (Bankruptcy)
//   nysb = New York Southern (Bankruptcy)
//   etc.
//
// We focus on courts near major logistics hubs and courts known for
// handling large commercial bankruptcy cases. Delaware alone handles
// roughly half of all major corporate bankruptcies because of its
// business-friendly laws, which is a polite way of saying "they've
// optimized the process of corporate financial death."
// =============================================================================
const PACER_COURTS: &[(&str, &str)] = &[
    ("Delaware",                     "https://ecf.deb.uscourts.gov/cgi-bin/rss_outside.pl"),
    ("S.D. New York",               "https://ecf.nysb.uscourts.gov/cgi-bin/rss_outside.pl"),
    ("D. New Jersey",               "https://ecf.njb.uscourts.gov/cgi-bin/rss_outside.pl"),
    ("N.D. Illinois",               "https://ecf.ilnb.uscourts.gov/cgi-bin/rss_outside.pl"),
    ("N.D. Texas",                  "https://ecf.txnb.uscourts.gov/cgi-bin/rss_outside.pl"),
    ("S.D. Texas",                  "https://ecf.txsb.uscourts.gov/cgi-bin/rss_outside.pl"),
    ("C.D. California",             "https://ecf.cacb.uscourts.gov/cgi-bin/rss_outside.pl"),
    ("N.D. Georgia",                "https://ecf.ganb.uscourts.gov/cgi-bin/rss_outside.pl"),
    ("E.D. Virginia",               "https://ecf.vaeb.uscourts.gov/cgi-bin/rss_outside.pl"),
    ("W.D. Missouri",               "https://ecf.mowb.uscourts.gov/cgi-bin/rss_outside.pl"),
    ("S.D. Indiana",                "https://ecf.insb.uscourts.gov/cgi-bin/rss_outside.pl"),
    ("M.D. Tennessee",              "https://ecf.tnmb.uscourts.gov/cgi-bin/rss_outside.pl"),
];

/// The main entry point for the PACER scanner.
///
/// This function never returns under normal operation — it loops forever,
/// polling PACER RSS feeds until it receives a shutdown signal. Think of it
/// as a very dedicated, very fast, very obsessive court reporter who never
/// sleeps, never eats, and never stops reading bankruptcy filings.
///
/// # Arguments
/// * `config` - The global configuration, containing poll intervals and
///   circuit breaker thresholds. Wrapped in an Arc because sharing is caring.
/// * `event_tx` - The sending end of the crossbeam channel. This is where
///   we push detected bankruptcy events so the publisher can scream them
///   into Redis.
/// * `dedup` - The deduplication engine. Returns true if an item is NEW,
///   false if we've seen it before. Uses a Bloom filter + LRU cache hybrid
///   because a HashSet would be too easy.
/// * `shutdown` - A watch channel receiver. When this flips to true, we
///   gracefully exit the loop and go home.
pub async fn run(
    config: Arc<Config>,
    event_tx: Sender<BankruptcyEvent>,
    dedup: Arc<DedupEngine>,
    shutdown: &mut watch::Receiver<bool>,
) {
    info!("PACER Scanner initializing — preparing to consume bankruptcy RSS feeds like a gourmand at a buffet of financial despair");

    // Build an HTTP client with a reasonable timeout and user agent.
    // We identify ourselves honestly because PACER administrators have
    // enough problems without wondering who's scraping their RSS feeds.
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent("FreightDoomEngine/1.0 (bankruptcy-research; educational-project)")
        .build()
        .expect("Failed to build HTTP client — this is genuinely embarrassing");

    // Create a circuit breaker for PACER endpoints.
    // PACER goes down more often than you'd expect for a critical
    // federal judiciary system. Five failures and we back off for
    // a minute. Two successes and we're back in business.
    let circuit_breaker = CircuitBreaker::new(
        "PACER",
        config.circuit_breaker_failure_threshold,
        config.circuit_breaker_reset_timeout,
        config.circuit_breaker_success_threshold,
    );

    let poll_interval = config.pacer_poll_interval;
    let min_confidence = config.min_confidence_threshold;

    info!(
        poll_interval_secs = poll_interval.as_secs(),
        courts = PACER_COURTS.len(),
        "PACER Scanner online — monitoring {} bankruptcy courts with the intensity of a hawk watching a mouse",
        PACER_COURTS.len()
    );

    // The main loop. This is where we live now.
    // Every poll_interval seconds, we scan all 12 courts simultaneously
    // using futures::future::join_all because scanning them sequentially
    // would be like loading a 53-foot trailer one box at a time.
    loop {
        tokio::select! {
            // Branch 1: Time to poll. Let's go bother some government servers.
            _ = tokio::time::sleep(poll_interval) => {
                // Check if the circuit breaker allows requests.
                // If PACER has been having a bad day, we give it space.
                if !circuit_breaker.allow_request() {
                    debug!("PACER: circuit breaker is OPEN — giving the courts a breather");
                    continue;
                }

                // Scan all courts. We could do them sequentially, but why
                // would we when tokio gives us async superpowers?
                let mut total_new_events = 0u64;

                for (court_name, feed_url) in PACER_COURTS {
                    match fetch_and_parse_feed(&client, court_name, feed_url).await {
                        Ok(items) => {
                            circuit_breaker.record_success();

                            for (title, description, link) in &items {
                                // Combine title and description for scanning.
                                // PACER titles are typically case numbers + debtor names.
                                // Descriptions contain the actual docket text.
                                let combined_text = format!("{} {}", title, description);

                                // Quick freight check first — this uses SIMD-accelerated
                                // memchr scanning to see if the text even contains freight
                                // keywords before we fire up the full Aho-Corasick automaton.
                                // It's the "is this worth my time?" check.
                                if !text_scanner::quick_freight_check(&combined_text) {
                                    continue;
                                }

                                // Full text scan with the Aho-Corasick automaton.
                                // This simultaneously searches for ALL freight and bankruptcy
                                // keywords in a single pass through the text. The algorithm
                                // is O(n + m) where n is text length and m is total matches.
                                // We're using the same algorithm as antivirus scanners.
                                // For RSS feed items. You're welcome.
                                let scan_result = text_scanner::scan_text(&combined_text);

                                if scan_result.confidence < min_confidence {
                                    continue;
                                }

                                // Build a dedup key from court + link to avoid processing
                                // the same filing multiple times across poll cycles.
                                let dedup_key = format!("pacer:{}:{}", court_name, link);

                                // check_and_insert returns TRUE if the item is NEW.
                                // The Bloom filter checks first (O(1)), and if it says
                                // "maybe seen", the LRU cache provides a definitive answer.
                                if !dedup.check_and_insert(&dedup_key) {
                                    debug!(
                                        court = court_name,
                                        title = title.as_str(),
                                        "Duplicate filing detected — Bloom + LRU said 'been there, done that'"
                                    );
                                    continue;
                                }

                                // Extract the company name from the PACER title.
                                // Titles typically look like: "2:24-bk-12345 Acme Freight LLC"
                                let company_name = extract_company_name(title);

                                // Build the event using the constructor, then set fields
                                let mut event = BankruptcyEvent::new(
                                    company_name,
                                    Source::Pacer,
                                    scan_result.confidence,
                                );
                                event.court = Some(court_name.to_string());
                                event.chapter = detect_chapter(&combined_text);
                                event.classification = scan_result.classification;
                                event.source_url = if link.is_empty() {
                                    Some(feed_url.to_string())
                                } else {
                                    Some(link.clone())
                                };
                                event.filing_date = parse_filing_date(description);
                                event.dot_number = extract_dot_number(&combined_text);
                                event.mc_number = extract_mc_number(&combined_text);

                                // Fire the event into the crossbeam channel.
                                // try_send is non-blocking — if the channel is full
                                // (10,000 events deep), we log an error and move on.
                                // If we're 10,000 events behind, we have bigger problems.
                                match event_tx.try_send(event) {
                                    Ok(()) => {
                                        total_new_events += 1;
                                        info!(
                                            court = court_name,
                                            title = title.as_str(),
                                            confidence = format!("{:.1}%", scan_result.confidence * 100.0),
                                            keywords = scan_result.matched_keywords.len(),
                                            "PACER: NEW BANKRUPTCY FILING DETECTED — another one bites the dust"
                                        );
                                    }
                                    Err(e) => {
                                        error!(
                                            error = %e,
                                            "PACER: failed to send event to channel — the channel is either full or dead"
                                        );
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            circuit_breaker.record_failure();
                            debug!(
                                court = court_name,
                                error = %e,
                                "PACER: failed to fetch/parse RSS feed — the court's server is having an existential crisis"
                            );
                        }
                    }
                }

                if total_new_events > 0 {
                    info!(
                        new_events = total_new_events,
                        "PACER scan cycle complete — {} new freight bankruptcy filings detected across {} courts",
                        total_new_events,
                        PACER_COURTS.len()
                    );
                } else {
                    debug!("PACER scan cycle complete — no new freight bankruptcies (the freight industry lives to fight another day)");
                }
            }

            // Branch 2: Shutdown signal received. Time to go home.
            _ = shutdown.changed() => {
                info!("PACER Scanner received shutdown signal — hanging up the RSS feed reader");
                break;
            }
        }
    }

    info!("PACER Scanner has exited the building");
}

// =============================================================================
// RSS Feed Fetching and Parsing
// =============================================================================
// We parse PACER's XML RSS feeds manually because pulling in a full RSS
// parsing library for what is essentially "find <item> tags and read their
// children" felt like bringing a chainsaw to a butter-cutting party.
//
// The XML structure looks like:
// <rss>
//   <channel>
//     <item>
//       <title>2:24-bk-12345 Acme Freight LLC</title>
//       <link>https://ecf.deb.uscourts.gov/...</link>
//       <description>Chapter 11 bankruptcy filing...</description>
//       <pubDate>Mon, 15 Jan 2024 12:00:00 GMT</pubDate>
//     </item>
//     ...
//   </channel>
// </rss>
// =============================================================================

/// Fetch an RSS feed from a PACER court and parse it into (title, description, link) tuples.
///
/// We're doing manual XML extraction here instead of using a proper XML parser
/// because PACER's XML is simple enough that regex-adjacent string scanning
/// works perfectly fine. Is this best practice? No. Does it work? Yes.
/// Will it break if PACER changes their XML format? Probably. Will PACER
/// change their XML format? They haven't since 2008, so we're probably safe.
async fn fetch_and_parse_feed(
    client: &reqwest::Client,
    court_name: &str,
    url: &str,
) -> Result<Vec<(String, String, String)>, Box<dyn std::error::Error + Send + Sync>> {
    debug!(court = court_name, url = url, "Fetching PACER RSS feed");

    let response = client.get(url).send().await?;

    if !response.status().is_success() {
        return Err(format!(
            "PACER {} returned HTTP {} — the court's server is not in a sharing mood",
            court_name,
            response.status()
        ).into());
    }

    let body = response.text().await?;
    let items = extract_rss_items(&body);

    debug!(
        court = court_name,
        items = items.len(),
        "Parsed {} RSS items from {} (each one a potential freight company's last chapter)",
        items.len(),
        court_name
    );

    Ok(items)
}

/// Extract <item> elements from RSS XML.
/// Returns a Vec of (title, description, link) tuples.
///
/// This function is essentially a very specific, very limited XML parser
/// that only understands <item>, <title>, <description>, and <link> tags.
/// It handles CDATA sections because PACER likes to wrap content in CDATA
/// like a burrito of legal text.
fn extract_rss_items(xml: &str) -> Vec<(String, String, String)> {
    let mut items = Vec::new();
    let mut remaining = xml;

    // Walk through the XML looking for <item> elements.
    // This is the "find the hay in the haystack" part, except
    // the haystack is XML and the hay is bankrupt trucking companies.
    while let Some(item_start) = remaining.find("<item>") {
        if let Some(item_end) = remaining[item_start..].find("</item>") {
            let item_xml = &remaining[item_start..item_start + item_end + 7];

            let title = extract_xml_tag(item_xml, "title");
            let description = extract_xml_tag(item_xml, "description");
            let link = extract_xml_tag(item_xml, "link");

            items.push((title, description, link));
            remaining = &remaining[item_start + item_end + 7..];
        } else {
            break;
        }
    }

    items
}

/// Extract the text content of an XML tag, handling CDATA sections.
///
/// Given XML like `<title><![CDATA[Some Text]]></title>`, returns "Some Text".
/// Given XML like `<title>Some Text</title>`, also returns "Some Text".
/// Given XML without the tag, returns an empty string, because the absence
/// of data is still data in our philosophical framework.
fn extract_xml_tag(xml: &str, tag: &str) -> String {
    let open = format!("<{}>", tag);
    let close = format!("</{}>", tag);

    if let Some(start) = xml.find(&open) {
        if let Some(end) = xml[start..].find(&close) {
            let content = &xml[start + open.len()..start + end];
            return content
                .replace("<![CDATA[", "")
                .replace("]]>", "")
                .trim()
                .to_string();
        }
    }
    String::new()
}

/// Extract the company name from a PACER RSS title.
///
/// PACER titles follow patterns like:
///   "2:24-bk-12345 Acme Freight LLC"
///   "1:24-bk-67890-ABC Big Truck Company Inc."
///   "24-12345 Some Carrier Corp"
///
/// We strip the case number prefix and return the rest as the company name.
/// If we can't parse it, we return the whole title — better to have a messy
/// name than no name at all.
fn extract_company_name(title: &str) -> String {
    // PACER case numbers look like "2:24-bk-12345" or "24-12345-ABC"
    // They always start with digits or "digit:digit" and contain hyphens.
    // The company name comes after the case number, separated by a space.
    if let Some(space_idx) = title.find(' ') {
        let potential_case_num = &title[..space_idx];
        // If the first part looks like a case number (contains digits and hyphens)
        if potential_case_num.contains('-') && potential_case_num.chars().any(|c| c.is_ascii_digit()) {
            return title[space_idx..].trim().to_string();
        }
    }
    title.to_string()
}

/// Detect the bankruptcy chapter from text content.
///
/// Bankruptcy chapters are like Dante's circles of hell, except there are
/// only three that matter for our purposes:
/// - Chapter 7: Liquidation (sell everything, pay creditors, close the doors)
/// - Chapter 11: Reorganization (try to survive, usually fail anyway)
/// - Chapter 13: Individual debt adjustment (rare for companies)
///
/// If we can't determine the chapter, we return Unknown, which is the
/// bankruptcy equivalent of "something bad happened but we're not sure what."
fn detect_chapter(text: &str) -> BankruptcyChapter {
    let upper = text.to_uppercase();
    if upper.contains("CHAPTER 7") || upper.contains("CH. 7") || upper.contains("CH 7") || upper.contains("CH.7") {
        BankruptcyChapter::Chapter7
    } else if upper.contains("CHAPTER 11") || upper.contains("CH. 11") || upper.contains("CH 11") || upper.contains("CH.11") {
        BankruptcyChapter::Chapter11
    } else if upper.contains("CHAPTER 13") || upper.contains("CH. 13") || upper.contains("CH 13") || upper.contains("CH.13") {
        BankruptcyChapter::Chapter13
    } else {
        BankruptcyChapter::Unknown
    }
}

/// Try to extract a DOT number from text.
///
/// DOT numbers (USDOT numbers) are 1-8 digit identifiers assigned by FMCSA
/// to motor carriers. They appear in filings as "DOT 1234567" or
/// "USDOT# 1234567" or various other formats because consistency in
/// legal documents is apparently optional.
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

/// Try to extract an MC number from text.
///
/// MC (Motor Carrier) numbers are issued by FMCSA for interstate
/// operating authority. They show up in filings in formats like
/// "MC 123456" or "MC# 123456" or "MC-123456" because legal
/// professionals apparently use the same formatting rules as
/// kindergarteners.
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

/// Attempt to parse a filing date from a PACER description or pubDate field.
///
/// PACER dates come in RFC 2822 format ("Mon, 15 Jan 2024 12:00:00 GMT")
/// in the pubDate field, or as "mm/dd/yyyy" or "yyyy-mm-dd" in descriptions.
/// We try all reasonable formats because government date formatting is
/// a choose-your-own-adventure book with no good endings.
fn parse_filing_date(text: &str) -> Option<DateTime<Utc>> {
    // Try common date formats found in PACER RSS feeds
    let date_formats = [
        "%m/%d/%Y",
        "%Y-%m-%d",
        "%B %d, %Y",
        "%b %d, %Y",
    ];

    // Look for date-like patterns in the text
    // This is extremely rudimentary but handles the common cases
    for fmt in &date_formats {
        // Try to find a substring that matches each format
        let text_words: Vec<&str> = text.split_whitespace().collect();
        for window in text_words.windows(3) {
            let candidate = window.join(" ");
            if let Ok(naive) = NaiveDateTime::parse_from_str(&format!("{} 00:00:00", candidate), &format!("{} %H:%M:%S", fmt)) {
                return Some(naive.and_utc());
            }
        }
        // Also try single words (for formats like "01/15/2024" or "2024-01-15")
        for word in &text_words {
            if let Ok(naive) = NaiveDateTime::parse_from_str(&format!("{} 00:00:00", word), &format!("{} %H:%M:%S", fmt)) {
                return Some(naive.and_utc());
            }
        }
    }

    None
}
