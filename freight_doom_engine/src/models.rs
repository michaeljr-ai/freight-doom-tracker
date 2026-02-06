// =============================================================================
// models.rs — THE SACRED DATA STRUCTURES OF FINANCIAL DOOM
// =============================================================================
//
// These structs represent the fundamental building blocks of our bankruptcy
// detection system. Each field has been carefully chosen to capture every
// conceivable piece of information about a logistics company's descent into
// Chapter 7/11 oblivion.
//
// Is it overkill to have a confidence_score on a bankruptcy filing?
// Yes. Do we care? Absolutely not.
// =============================================================================

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// The source from which we detected the bankruptcy event.
/// Each source has its own scanner, its own circuit breaker, its own
/// existential crisis when the API goes down.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Source {
    /// PACER — Public Access to Court Electronic Records
    /// The OG of bankruptcy detection. RSS feeds from the actual courts.
    /// If it's on PACER, it's real. It's happening. The trucks have stopped.
    Pacer,

    /// SEC EDGAR — Electronic Data Gathering, Analysis, and Retrieval
    /// For when publicly traded freight companies file their final 10-K
    /// and it includes the words "going concern" and "material uncertainty"
    Edgar,

    /// FMCSA — Federal Motor Carrier Safety Administration
    /// When a carrier's authority goes from "ACTIVE" to "REVOKED",
    /// something has gone terribly wrong. We want to know about it
    /// approximately 0.003 seconds after it happens.
    Fmcsa,

    /// CourtListener — Free Law Project's court opinion database
    /// Open source, open data, open season on bankrupt freight companies.
    CourtListener,
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Source::Pacer => write!(f, "PACER"),
            Source::Edgar => write!(f, "SEC_EDGAR"),
            Source::Fmcsa => write!(f, "FMCSA"),
            Source::CourtListener => write!(f, "COURT_LISTENER"),
        }
    }
}

/// The type of bankruptcy chapter filed.
/// Because not all financial doom is created equal.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum BankruptcyChapter {
    /// Chapter 7 — Liquidation. The "sell everything including the office chairs" option.
    /// For freight companies, this means the trucks are getting auctioned off.
    Chapter7,

    /// Chapter 11 — Reorganization. The "we can fix this, we swear" option.
    /// For freight companies, this means they'll try to restructure while
    /// their drivers wonder if they should update their resumes.
    Chapter11,

    /// Chapter 13 — Individual debt adjustment (rare for companies but possible
    /// for owner-operators who are technically sole proprietors)
    Chapter13,

    /// We found a bankruptcy filing but couldn't determine the chapter.
    /// This happens more often than you'd think with government data.
    Unknown,
}

impl fmt::Display for BankruptcyChapter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BankruptcyChapter::Chapter7 => write!(f, "Chapter 7"),
            BankruptcyChapter::Chapter11 => write!(f, "Chapter 11"),
            BankruptcyChapter::Chapter13 => write!(f, "Chapter 13"),
            BankruptcyChapter::Unknown => write!(f, "Unknown"),
        }
    }
}

/// The classification of the logistics company.
/// Because "freight company" is about as specific as "food" at a restaurant.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CompanyClassification {
    /// Motor carrier — the ones with the trucks
    Carrier,
    /// Freight broker — the ones who connect shippers with carriers
    /// and take a cut for... existing, basically
    Broker,
    /// Third-party logistics provider — the ones who do everything
    /// and somehow still can't find your shipment
    ThirdPartyLogistics,
    /// Freight forwarder — the international ones who deal with customs
    /// and make sure your container doesn't end up in the wrong ocean
    FreightForwarder,
    /// Could be any of the above. The filing didn't specify.
    Unclassified,
}

impl fmt::Display for CompanyClassification {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompanyClassification::Carrier => write!(f, "Carrier"),
            CompanyClassification::Broker => write!(f, "Broker"),
            CompanyClassification::ThirdPartyLogistics => write!(f, "3PL"),
            CompanyClassification::FreightForwarder => write!(f, "Freight Forwarder"),
            CompanyClassification::Unclassified => write!(f, "Unclassified"),
        }
    }
}

/// The main event struct. This is what gets published to Redis and consumed
/// by the Rails app. Every field here represents a piece of the puzzle
/// in our quest to detect freight company bankruptcy before the trucks
/// even finish their last delivery.
///
/// Is having 12 fields on a bankruptcy event struct overkill?
/// The answer is no. We could easily justify 30.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BankruptcyEvent {
    /// A UUID v4 for this specific event. Because even in bankruptcy,
    /// every event deserves to feel unique and special.
    pub id: String,

    /// The name of the company that has met its financial demise.
    /// May include DBA names, trade names, and the tears of investors.
    pub company_name: String,

    /// USDOT Number — the unique identifier assigned by FMCSA.
    /// If we have this, we can cross-reference with SAFER data.
    /// Optional because not all filings include it (looking at you, PACER).
    pub dot_number: Option<String>,

    /// MC Number — Motor Carrier number for interstate commerce authority.
    /// Another FMCSA identifier. Yes, the government needs two numbers
    /// to track one trucking company. Peak efficiency.
    pub mc_number: Option<String>,

    /// When the bankruptcy was filed. Not when we detected it —
    /// that's detected_at. This is the actual filing date from the court.
    pub filing_date: Option<DateTime<Utc>>,

    /// Which court the filing was made in. For PACER sources, this
    /// will be something like "S.D.N.Y." or "N.D. Ill."
    pub court: Option<String>,

    /// Chapter 7, 11, 13, or "we have no idea but something bad happened"
    pub chapter: BankruptcyChapter,

    /// Where we found this information. See the Source enum above
    /// for the full existential breakdown.
    pub source: Source,

    /// When OUR system detected this event. This is the timestamp
    /// of when our scanner first noticed the filing.
    /// In a perfect world, detected_at - filing_date < 1 second.
    /// In reality, it's usually hours or days. Government websites, man.
    pub detected_at: DateTime<Utc>,

    /// A confidence score from 0.0 to 1.0 indicating how confident
    /// we are that this is actually a logistics company bankruptcy.
    /// 1.0 = "this is definitely a trucking company going under"
    /// 0.5 = "maybe? the filing mentioned trucks once"
    /// 0.1 = "someone at a law firm Googled 'freight' and it ended up in the filing"
    pub confidence_score: f64,

    /// What type of logistics company this is.
    /// Carrier, broker, 3PL, freight forwarder, or "beats me"
    pub classification: CompanyClassification,

    /// The raw URL where we found this filing, so humans can verify
    /// that our robot overlord didn't hallucinate a bankruptcy.
    pub source_url: Option<String>,
}

impl BankruptcyEvent {
    /// Create a new BankruptcyEvent with a fresh UUID and current timestamp.
    /// Because every bankruptcy deserves a birthday.
    pub fn new(
        company_name: String,
        source: Source,
        confidence_score: f64,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            company_name,
            dot_number: None,
            mc_number: None,
            filing_date: None,
            court: None,
            chapter: BankruptcyChapter::Unknown,
            source,
            detected_at: Utc::now(),
            confidence_score,
            classification: CompanyClassification::Unclassified,
            source_url: None,
        }
    }

    /// Generate a deduplication key for this event.
    /// We combine company name + source to create a unique-ish key.
    /// The bloom filter and LRU cache will use this to decide if we've
    /// already screamed about this particular bankruptcy into the Redis void.
    pub fn dedup_key(&self) -> String {
        format!(
            "{}:{}:{}",
            self.company_name.to_lowercase().trim(),
            self.source,
            self.chapter
        )
    }
}

impl fmt::Display for BankruptcyEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}] {} ({}) — {} via {} (confidence: {:.1}%)",
            self.id,
            self.company_name,
            self.classification,
            self.chapter,
            self.source,
            self.confidence_score * 100.0
        )
    }
}

/// Health status for each scanner. Because monitoring the monitors
/// is how you achieve true operational nirvana.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScannerHealth {
    /// Which scanner this health report is for
    pub source: Source,

    /// Is the scanner currently running? If not, something has gone wrong
    /// and you should probably check the logs. Or the internet. Or both.
    pub is_running: bool,

    /// How many events this scanner has found since startup.
    /// If this number is 0 after hours of running, either the freight
    /// industry is doing great or your scanner is broken.
    pub events_found: u64,

    /// How many errors this scanner has encountered.
    /// A few is normal. Hundreds means the API is having a bad day.
    pub errors: u64,

    /// When the scanner last successfully polled its source.
    pub last_poll: Option<DateTime<Utc>>,

    /// Current circuit breaker state as a human-readable string.
    pub circuit_breaker_state: String,
}

/// Represents a raw RSS item from PACER before we process it
/// into a proper BankruptcyEvent. Think of this as the "ugly duckling"
/// stage of our data pipeline.
#[derive(Debug, Clone, Deserialize)]
pub struct PacerRssItem {
    pub title: Option<String>,
    pub link: Option<String>,
    pub description: Option<String>,
    #[serde(rename = "pubDate")]
    pub pub_date: Option<String>,
}

/// Represents a search result from SEC EDGAR full-text search.
/// EDGAR returns JSON (thank god) unlike PACER's XML fetish.
#[derive(Debug, Clone, Deserialize)]
pub struct EdgarSearchResult {
    pub hits: Option<EdgarHits>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EdgarHits {
    pub total: Option<EdgarTotal>,
    pub hits: Option<Vec<EdgarHit>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EdgarTotal {
    pub value: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EdgarHit {
    #[serde(rename = "_source")]
    pub source: Option<EdgarSource>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EdgarSource {
    pub file_date: Option<String>,
    pub entity_name: Option<String>,
    pub file_description: Option<String>,
    pub file_type: Option<String>,
}

/// FMCSA carrier record — the government's way of tracking
/// every trucking company in America. Your tax dollars at work.
#[derive(Debug, Clone, Deserialize)]
pub struct FmcsaCarrierRecord {
    pub dot_number: Option<String>,
    pub legal_name: Option<String>,
    pub dba_name: Option<String>,
    pub carrier_operation: Option<String>,
    pub operating_status: Option<String>,
    pub mc_number: Option<String>,
}

/// CourtListener search result. The Free Law Project is doing
/// God's work by making court data accessible. We're using it
/// to track freight bankruptcies. They'd probably be fine with that.
#[derive(Debug, Clone, Deserialize)]
pub struct CourtListenerResult {
    pub count: Option<u64>,
    pub results: Option<Vec<CourtListenerOpinion>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CourtListenerOpinion {
    pub id: Option<u64>,
    pub case_name: Option<String>,
    pub court: Option<String>,
    pub date_filed: Option<String>,
    pub snippet: Option<String>,
    pub absolute_url: Option<String>,
}
