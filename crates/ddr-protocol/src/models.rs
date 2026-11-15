use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
    pub statement: String,
    pub polarity: Polarity,
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
    pub findings: Vec<AtomicClaim>,
    pub suggested_branches: Vec<SuggestedBranch>,
    pub sources: Vec<SourceMetadata>,
}
