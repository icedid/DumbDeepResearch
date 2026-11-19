use serde::{Deserialize, Serialize};
use uuid::Uuid;

fn default_confidence() -> f32 {
    0.8
}

fn default_true() -> bool {
    true
}

/// Lifecycle state of a Knowledge Node on the canvas and in the DAG.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NodeStatus {
    Proposed,
    Queued,
    Searching,
    Scraping,
    Complete,
    Pruned,
}

/// Analytical slot archetypes for enforcing query orthogonality.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FacetSlot {
    Mechanistic,
    Quantitative,
    Economic,
    Comparative,
    Exploratory,
}

/// Stance polarity of an extracted assertion for 50/50 perspective balancing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Polarity {
    Bull,
    Bear,
    NeutralMetric,
}

/// An atomic factual assertion or empirical metric extracted from a scraped source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtomicClaim {
    pub id: Uuid,
    pub source_url: String,
    pub dom_sha256: String,
    #[serde(default)]
    pub subject: String,
    pub statement: String,
    pub polarity: Polarity,
    #[serde(default = "default_confidence")]
    pub confidence: f32,
    #[serde(default)]
    pub cluster_id: Option<usize>,
    #[serde(default = "default_true")]
    pub is_selected: bool,
}

/// Provenance-stamped source document attached to a Knowledge Node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceMetadata {
    pub url: String,
    pub domain: String,
    pub title: String,
    pub snippet: String,
    pub dom_sha256: String,
    pub hop_depth: u8,
}

/// A proposed follow-up branch generated via Cascaded PRF + Windowed Contrast.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestedBranch {
    pub topic: String,
    pub query: String,
    pub slot: FacetSlot,
    pub significance_score: f32,
}

/// Primary unit of the Knowledge Graph and Visual Canvas.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeNode {
    pub id: Uuid,
    pub parent_id: Option<Uuid>,
    pub depth: u8,
    pub topic: String,
    pub query: String,
    pub slot: FacetSlot,
    pub status: NodeStatus,
    #[serde(default)]
    pub prompt_override: Option<String>,
    #[serde(default)]
    pub probe_reason: Option<String>,
    #[serde(default)]
    pub asymmetry_flag: Option<String>,
    pub findings: Vec<AtomicClaim>,
    pub suggested_branches: Vec<SuggestedBranch>,
    pub sources: Vec<SourceMetadata>,
}

/// Deterministic verification report ensuring every dossier `[CLAIM-ID]` citation
/// traces back to an authentic scraped URL and DOM SHA-256 snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CitationAuditResult {
    pub total_citations: usize,
    pub verified_citations: usize,
    pub unverified_citations: Vec<String>,
    pub passed: bool,
}

/// Single lineage row tracing Claim -> Query -> PageNode URL + SHA-256.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DossierLineageEntry {
    pub claim_id: Uuid,
    pub short_id: String,
    pub node_id: Uuid,
    pub node_topic: String,
    pub query: String,
    pub polarity: Polarity,
    pub statement: String,
    pub source_url: String,
    pub dom_sha256: String,
    pub hop_depth: u8,
}

/// Audit-ready dossier compiled across the current Knowledge Graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditDossier {
    pub root_topic: String,
    pub node_count: usize,
    pub source_count: usize,
    pub claim_count: usize,
    pub bull_count: usize,
    pub bear_count: usize,
    pub neutral_count: usize,
    pub markdown: String,
    pub lineage: Vec<DossierLineageEntry>,
    pub citation_audit: CitationAuditResult,
}
