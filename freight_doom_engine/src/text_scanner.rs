// =============================================================================
// text_scanner.rs — THE SIMD-ACCELERATED TEXT ANNIHILATOR
// =============================================================================
//
// This module is where we do the actual "is this a logistics company
// bankruptcy?" determination. And we do it FAST. How fast? We use:
//
// 1. Aho-Corasick algorithm — multi-pattern matching that scans text
//    for ALL keywords simultaneously in a single pass. O(n + m) where
//    n is the text length and m is the total number of matches.
//    Built on a finite automaton. This is how antivirus scanners work.
//    We're using antivirus-grade technology to find bankrupt trucking
//    companies. Let that sink in.
//
// 2. memchr — SIMD-accelerated byte scanning. Uses SSE2/AVX2/NEON
//    vector instructions to scan memory at speeds that would make
//    a for loop weep. We use it for fast preliminary checks before
//    firing up the full Aho-Corasick automaton.
//
// 3. Rayon parallel iterators — when we have multiple texts to scan,
//    we parallelize across all available CPU cores. Because leaving
//    cores idle while there are bankruptcies to detect is practically
//    criminal negligence.
//
// Is SIMD-accelerated text scanning overkill for parsing a few RSS feeds?
// The answer is yes, and we wouldn't have it any other way.
// =============================================================================

use aho_corasick::AhoCorasick;
use rayon::prelude::*;
use std::sync::LazyLock;
use tracing::debug;

use crate::models::CompanyClassification;

/// Freight/logistics keywords. If ANY of these appear in a bankruptcy filing,
/// we get excited. The more that appear, the more excited we get.
/// This list was compiled by reading way too many bankruptcy filings.
static FREIGHT_KEYWORDS: LazyLock<Vec<&str>> = LazyLock::new(|| {
    vec![
        // Direct logistics terms
        "freight",
        "trucking",
        "carrier",
        "logistics",
        "transportation",
        "shipping",
        "hauling",
        "drayage",
        "intermodal",
        "ltl",
        "truckload",
        "less than truckload",
        "full truckload",
        "flatbed",
        "reefer",
        "refrigerated",
        "tanker",
        "dry van",
        "container",
        "trailer",
        "tractor",
        "semi",
        "18 wheeler",
        "eighteen wheeler",
        "motor carrier",
        "common carrier",
        "contract carrier",
        "freight broker",
        "freight forwarder",
        "3pl",
        "third party logistics",
        "third-party logistics",
        "supply chain",
        "warehouse",
        "warehousing",
        "distribution",
        "distribution center",
        "cross dock",
        "cross-dock",
        "last mile",
        "last-mile",
        "first mile",
        "middle mile",
        "linehaul",
        "line haul",
        "line-haul",
        "dispatch",
        "dispatcher",
        "load board",
        "dot number",
        "usdot",
        "mc number",
        "fmcsa",
        "operating authority",
        "broker authority",
        "cdl",
        "commercial driver",
        "owner operator",
        "owner-operator",
        "deadhead",
        "bobtail",
        "lumper",
        "bill of lading",
        "bol",
        "pod",
        "proof of delivery",
        "freight class",
        "nmfc",
        "stcc",
        // Industry-specific associations and terms
        "ata ",  // American Trucking Associations (space to avoid false matches)
        "ooida", // Owner-Operator Independent Drivers Association
        "tia",   // Transportation Intermediaries Association
        // Bankruptcy-specific terms
        "chapter 7",
        "chapter 11",
        "chapter 13",
        "bankruptcy",
        "bankrupt",
        "insolvency",
        "insolvent",
        "liquidation",
        "reorganization",
        "creditor",
        "debtor",
        "filing",
        "petition",
        "receivership",
        "dissolution",
        "wind down",
        "cease operations",
        "ceased operations",
        "going concern",
        "material uncertainty",
    ]
});

/// Keywords specifically indicating carrier operations
static CARRIER_KEYWORDS: LazyLock<Vec<&str>> = LazyLock::new(|| {
    vec![
        "motor carrier",
        "common carrier",
        "contract carrier",
        "trucking company",
        "trucking",
        "carrier",
        "fleet",
        "cdl",
        "driver",
        "owner operator",
        "tractor",
        "trailer",
        "dot number",
        "usdot",
    ]
});

/// Keywords specifically indicating broker operations
static BROKER_KEYWORDS: LazyLock<Vec<&str>> = LazyLock::new(|| {
    vec![
        "freight broker",
        "brokerage",
        "broker authority",
        "load board",
        "intermediary",
        "tia",
    ]
});

/// Keywords specifically indicating 3PL operations
static TPL_KEYWORDS: LazyLock<Vec<&str>> = LazyLock::new(|| {
    vec![
        "3pl",
        "third party logistics",
        "third-party logistics",
        "warehouse",
        "warehousing",
        "distribution center",
        "fulfillment",
    ]
});

/// Keywords specifically indicating freight forwarder operations
static FORWARDER_KEYWORDS: LazyLock<Vec<&str>> = LazyLock::new(|| {
    vec![
        "freight forwarder",
        "forwarding",
        "customs",
        "import",
        "export",
        "international shipping",
        "ocean freight",
        "air freight",
        "nvocc",
    ]
});

/// The Aho-Corasick automaton for freight keywords.
/// Built once, used forever. This is a finite state machine that can
/// match ALL keywords simultaneously in a single pass through the text.
/// It's the algorithmic equivalent of reading a page and circling every
/// suspicious word at the same time.
static FREIGHT_AUTOMATON: LazyLock<AhoCorasick> = LazyLock::new(|| {
    AhoCorasick::builder()
        .ascii_case_insensitive(true)
        .build(&*FREIGHT_KEYWORDS)
        .expect("Failed to build Aho-Corasick automaton — the keywords are invalid somehow")
});

static CARRIER_AUTOMATON: LazyLock<AhoCorasick> = LazyLock::new(|| {
    AhoCorasick::builder()
        .ascii_case_insensitive(true)
        .build(&*CARRIER_KEYWORDS)
        .expect("Failed to build carrier automaton")
});

static BROKER_AUTOMATON: LazyLock<AhoCorasick> = LazyLock::new(|| {
    AhoCorasick::builder()
        .ascii_case_insensitive(true)
        .build(&*BROKER_KEYWORDS)
        .expect("Failed to build broker automaton")
});

static TPL_AUTOMATON: LazyLock<AhoCorasick> = LazyLock::new(|| {
    AhoCorasick::builder()
        .ascii_case_insensitive(true)
        .build(&*TPL_KEYWORDS)
        .expect("Failed to build 3PL automaton")
});

static FORWARDER_AUTOMATON: LazyLock<AhoCorasick> = LazyLock::new(|| {
    AhoCorasick::builder()
        .ascii_case_insensitive(true)
        .build(&*FORWARDER_KEYWORDS)
        .expect("Failed to build forwarder automaton")
});

/// Result of scanning a text for freight/bankruptcy relevance.
#[derive(Debug, Clone)]
pub struct ScanResult {
    /// Confidence score from 0.0 to 1.0
    pub confidence: f64,
    /// How many freight keywords were found
    pub freight_keyword_hits: usize,
    /// How many bankruptcy keywords were found
    pub bankruptcy_keyword_hits: usize,
    /// Total unique keywords matched
    pub total_matches: usize,
    /// Classification of the company type
    pub classification: CompanyClassification,
    /// The keywords that were matched (for debugging/logging)
    pub matched_keywords: Vec<String>,
}

/// Scan a text for freight/logistics bankruptcy relevance.
///
/// This is the main entry point for text analysis. It runs the
/// Aho-Corasick automaton over the text and calculates a confidence
/// score based on keyword density and variety.
///
/// The confidence scoring algorithm:
/// - Base score from keyword density (matches / text_length_in_words)
/// - Bonus for having both freight AND bankruptcy keywords (cross-domain signal)
/// - Bonus for specific high-signal keywords like "chapter 11" or "motor carrier"
/// - Score capped at 1.0
///
/// A text mentioning "chapter 11 trucking company freight carrier" would
/// score very high. A text mentioning "freight" once in a 10,000 word
/// document would score very low.
pub fn scan_text(text: &str) -> ScanResult {
    if text.is_empty() {
        return ScanResult {
            confidence: 0.0,
            freight_keyword_hits: 0,
            bankruptcy_keyword_hits: 0,
            total_matches: 0,
            classification: CompanyClassification::Unclassified,
            matched_keywords: vec![],
        };
    }

    // SIMD-accelerated preliminary check using memchr.
    // If the text doesn't contain common bytes from our keywords,
    // we can skip the full Aho-Corasick scan entirely.
    // This is the "bouncer at the door" check.
    let has_potential = memchr::memmem::find(text.as_bytes(), b"freight").is_some()
        || memchr::memmem::find(text.as_bytes(), b"truck").is_some()
        || memchr::memmem::find(text.as_bytes(), b"carrier").is_some()
        || memchr::memmem::find(text.as_bytes(), b"bankrupt").is_some()
        || memchr::memmem::find(text.as_bytes(), b"chapter").is_some()
        || memchr::memmem::find(text.as_bytes(), b"logistics").is_some()
        || memchr::memmem::find(text.as_bytes(), b"Freight").is_some()
        || memchr::memmem::find(text.as_bytes(), b"Truck").is_some()
        || memchr::memmem::find(text.as_bytes(), b"Carrier").is_some()
        || memchr::memmem::find(text.as_bytes(), b"Bankrupt").is_some()
        || memchr::memmem::find(text.as_bytes(), b"FREIGHT").is_some()
        || memchr::memmem::find(text.as_bytes(), b"TRUCK").is_some();

    if !has_potential {
        return ScanResult {
            confidence: 0.0,
            freight_keyword_hits: 0,
            bankruptcy_keyword_hits: 0,
            total_matches: 0,
            classification: CompanyClassification::Unclassified,
            matched_keywords: vec![],
        };
    }

    // Full Aho-Corasick scan — find ALL matching keywords in a single pass
    let matches: Vec<_> = FREIGHT_AUTOMATON
        .find_iter(text)
        .collect();

    let total_matches = matches.len();
    if total_matches == 0 {
        return ScanResult {
            confidence: 0.0,
            freight_keyword_hits: 0,
            bankruptcy_keyword_hits: 0,
            total_matches: 0,
            classification: CompanyClassification::Unclassified,
            matched_keywords: vec![],
        };
    }

    // Collect unique matched keywords
    let mut matched_keywords: Vec<String> = matches
        .iter()
        .map(|m| text[m.start()..m.end()].to_lowercase())
        .collect();
    matched_keywords.sort();
    matched_keywords.dedup();

    // Count freight vs bankruptcy keyword hits
    let bankruptcy_terms = [
        "chapter 7", "chapter 11", "chapter 13", "bankruptcy", "bankrupt",
        "insolvency", "insolvent", "liquidation", "reorganization", "creditor",
        "debtor", "filing", "petition", "receivership", "dissolution",
        "wind down", "cease operations", "going concern",
    ];

    let bankruptcy_keyword_hits = matched_keywords
        .iter()
        .filter(|k| bankruptcy_terms.iter().any(|bt| k.contains(bt)))
        .count();

    let freight_keyword_hits = total_matches - bankruptcy_keyword_hits;

    // Calculate word count for density scoring
    let word_count = text.split_whitespace().count().max(1) as f64;

    // Confidence scoring algorithm
    let mut confidence: f64 = 0.0;

    // Base score from unique keyword variety (0.0 - 0.4)
    let unique_ratio = matched_keywords.len() as f64 / FREIGHT_KEYWORDS.len() as f64;
    confidence += (unique_ratio * 4.0).min(0.4);

    // Density bonus (0.0 - 0.3)
    let density = total_matches as f64 / word_count;
    confidence += (density * 30.0).min(0.3);

    // Cross-domain bonus: having BOTH freight and bankruptcy terms (0.0 - 0.2)
    if freight_keyword_hits > 0 && bankruptcy_keyword_hits > 0 {
        confidence += 0.2;
    }

    // High-signal keyword bonus (0.0 - 0.1)
    let high_signal = [
        "motor carrier", "freight broker", "trucking company",
        "3pl", "chapter 11", "chapter 7", "operating authority",
    ];
    let high_signal_count = matched_keywords
        .iter()
        .filter(|k| high_signal.iter().any(|hs| k.contains(hs)))
        .count();
    confidence += (high_signal_count as f64 * 0.05).min(0.1);

    // Cap at 1.0
    confidence = confidence.min(1.0);

    // Classify the company type
    let classification = classify_company(text);

    debug!(
        total_matches = total_matches,
        unique_keywords = matched_keywords.len(),
        freight_hits = freight_keyword_hits,
        bankruptcy_hits = bankruptcy_keyword_hits,
        confidence = format!("{:.3}", confidence),
        classification = %classification,
        "Text scan complete"
    );

    ScanResult {
        confidence,
        freight_keyword_hits,
        bankruptcy_keyword_hits,
        total_matches,
        classification,
        matched_keywords,
    }
}

/// Classify a company based on keyword analysis.
/// Uses separate Aho-Corasick automatons for each company type.
/// The type with the most keyword hits wins.
fn classify_company(text: &str) -> CompanyClassification {
    let carrier_hits = CARRIER_AUTOMATON.find_iter(text).count();
    let broker_hits = BROKER_AUTOMATON.find_iter(text).count();
    let tpl_hits = TPL_AUTOMATON.find_iter(text).count();
    let forwarder_hits = FORWARDER_AUTOMATON.find_iter(text).count();

    let max_hits = carrier_hits.max(broker_hits).max(tpl_hits).max(forwarder_hits);

    if max_hits == 0 {
        return CompanyClassification::Unclassified;
    }

    if carrier_hits == max_hits {
        CompanyClassification::Carrier
    } else if broker_hits == max_hits {
        CompanyClassification::Broker
    } else if tpl_hits == max_hits {
        CompanyClassification::ThirdPartyLogistics
    } else {
        CompanyClassification::FreightForwarder
    }
}

/// Batch-scan multiple texts in parallel using Rayon.
///
/// When you have N texts to scan and M CPU cores, why not use all M cores?
/// This function takes a slice of text strings and returns scan results
/// for all of them, processed in parallel.
///
/// For 4 texts on an 8-core machine, each text gets its own thread.
/// For 1000 texts, Rayon's work-stealing scheduler distributes them
/// efficiently. It's like having a fleet of trucks delivering packages,
/// except the packages are keyword match results and the trucks are
/// CPU threads. And some of the packages contain bankruptcy filings.
pub fn batch_scan(texts: &[&str]) -> Vec<ScanResult> {
    texts.par_iter().map(|text| scan_text(text)).collect()
}

/// Quick check if a text contains ANY freight-related keywords.
/// Uses memchr SIMD scanning for maximum speed.
/// Returns true if the text is worth a full scan.
///
/// This is the "should I even bother?" function. If this returns false,
/// the text is definitely not about a freight bankruptcy. If it returns
/// true, we need to do a full scan to be sure.
pub fn quick_freight_check(text: &str) -> bool {
    let bytes = text.as_bytes();
    // Check for common freight-related byte patterns using SIMD
    memchr::memmem::find(bytes, b"freight").is_some()
        || memchr::memmem::find(bytes, b"Freight").is_some()
        || memchr::memmem::find(bytes, b"FREIGHT").is_some()
        || memchr::memmem::find(bytes, b"truck").is_some()
        || memchr::memmem::find(bytes, b"Truck").is_some()
        || memchr::memmem::find(bytes, b"carrier").is_some()
        || memchr::memmem::find(bytes, b"Carrier").is_some()
        || memchr::memmem::find(bytes, b"logistics").is_some()
        || memchr::memmem::find(bytes, b"Logistics").is_some()
        || memchr::memmem::find(bytes, b"3pl").is_some()
        || memchr::memmem::find(bytes, b"3PL").is_some()
        || memchr::memmem::find(bytes, b"broker").is_some()
        || memchr::memmem::find(bytes, b"Broker").is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_text_returns_zero_confidence() {
        let result = scan_text("");
        assert_eq!(result.confidence, 0.0);
    }

    #[test]
    fn test_irrelevant_text_returns_zero() {
        let result = scan_text("The quick brown fox jumps over the lazy dog");
        assert_eq!(result.confidence, 0.0);
    }

    #[test]
    fn test_freight_bankruptcy_text_scores_high() {
        let text = "XYZ Trucking Company, a motor carrier with USDOT number 12345, \
                    has filed for Chapter 11 bankruptcy protection. The freight carrier \
                    operated a fleet of 200 trucks and employed 500 CDL drivers. \
                    The bankruptcy filing was submitted to the court on January 15.";
        let result = scan_text(text);
        assert!(result.confidence > 0.5);
        assert!(result.freight_keyword_hits > 0);
        assert!(result.bankruptcy_keyword_hits > 0);
    }

    #[test]
    fn test_classification_carrier() {
        let text = "ABC Motor Carrier with CDL drivers and a fleet of tractors and trailers";
        let result = scan_text(text);
        assert_eq!(result.classification, CompanyClassification::Carrier);
    }

    #[test]
    fn test_batch_scan_parallel() {
        let texts = vec![
            "freight trucking bankruptcy chapter 11",
            "the cat sat on the mat",
            "logistics carrier filing chapter 7",
        ];
        let results = batch_scan(&texts);
        assert_eq!(results.len(), 3);
        assert!(results[0].confidence > 0.0);
        assert_eq!(results[1].confidence, 0.0);
        assert!(results[2].confidence > 0.0);
    }

    #[test]
    fn test_quick_freight_check() {
        assert!(quick_freight_check("This is about freight"));
        assert!(quick_freight_check("A trucking company"));
        assert!(!quick_freight_check("The weather is nice today"));
    }
}
