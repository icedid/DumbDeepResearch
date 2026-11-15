use ddr_protocol::FacetSlot;
use uuid::Uuid;

/// Finite circuit-breaker states determining when a Knowledge Node probe ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeStatus {
    /// Probe is actively evaluating pages.
    Active,
    /// Reached a primary authority document (PDF, DOI, ArXiv, .gov table).
    CompletedTerminal,
    /// Satisfied the factual/metric yield quota for this Knowledge Node.
    CompletedQuotaMet,
    /// Page BM25 score dropped below the relevance floor (< 0.35).
    KilledDrift,
    /// MinHash Jaccard similarity showed vocabulary saturation (> 0.75).
    ExhaustedSaturated,
    /// Reached the hard hop budget limit (depth == 2).
    TerminatedMaxDepth,
}

/// Clean, provenance-stamped document output by the Crawl Engine.
#[derive(Debug, Clone)]
pub struct ScrapedDocument {
    pub doc_id: Uuid,
    pub node_id: Uuid,
    pub url: String,
    pub root_domain: String,
    pub branch_query: String,
    pub facet_slot: FacetSlot,
    pub hop_depth: u8,
    pub bm25_relevance: f32,
    pub minhash_sig: Vec<u64>,
    pub dom_sha256: String,
    pub clean_text: String,
}

/// Bounded linear search probe executing on behalf of a single Knowledge Node.
pub struct SearchProbe {
    pub node_id: Uuid,
    pub query: String,
    pub slot: FacetSlot,
    pub current_depth: u8,
    pub max_depth: u8,
    pub seen_minhashes: Vec<u64>,
    pub status: ProbeStatus,
}

impl SearchProbe {
    pub fn new(node_id: Uuid, query: String, slot: FacetSlot, max_depth: u8) -> Self {
        Self {
            node_id,
            query,
            slot,
            current_depth: 0,
            max_depth,
            seen_minhashes: Vec::new(),
            status: ProbeStatus::Active,
        }
    }

    /// Evaluates the 5 deterministic circuit breakers to decide if the Knowledge Node has reached its end.
    pub fn evaluate_circuit_breakers(&mut self, _doc: &ScrapedDocument) -> ProbeStatus {
        // Boilerplate stub:
        // 1. Terminal PDF/DOI check -> ProbeStatus::CompletedTerminal
        // 2. Hard hop depth check -> ProbeStatus::TerminatedMaxDepth
        // 3. BM25 drift floor check -> ProbeStatus::KilledDrift
        // 4. MinHash saturation check -> ProbeStatus::ExhaustedSaturated
        // 5. Metric yield check -> ProbeStatus::CompletedQuotaMet
        self.status
    }
}
