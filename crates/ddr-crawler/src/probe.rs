use crate::metrics::{count_empirical_metrics, minhash_jaccard};
use ddr_protocol::FacetSlot;
use uuid::Uuid;

const BM25_DRIFT_FLOOR: f32 = 0.35;
const MINHASH_SATURATION_CEILING: f32 = 0.75;
const DEFAULT_METRIC_QUOTA: usize = 6;
const MIN_QUOTA_DOCS: usize = 2;

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
    /// Reached the hard hop budget limit (`hop_depth >= max_depth`).
    TerminatedMaxDepth,
}

impl ProbeStatus {
    pub fn as_label(self) -> &'static str {
        match self {
            ProbeStatus::Active => "Active",
            ProbeStatus::CompletedTerminal => "CompletedTerminal",
            ProbeStatus::CompletedQuotaMet => "CompletedQuotaMet",
            ProbeStatus::KilledDrift => "KilledDrift",
            ProbeStatus::ExhaustedSaturated => "ExhaustedSaturated",
            ProbeStatus::TerminatedMaxDepth => "TerminatedMaxDepth",
        }
    }

    pub fn is_terminal_stop(self) -> bool {
        matches!(
            self,
            ProbeStatus::CompletedTerminal
                | ProbeStatus::CompletedQuotaMet
                | ProbeStatus::TerminatedMaxDepth
        )
    }
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
    pub seen_signatures: Vec<Vec<u64>>,
    pub accepted_docs: usize,
    pub accumulated_metrics: usize,
    pub metric_quota: usize,
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
            seen_signatures: Vec::new(),
            accepted_docs: 0,
            accumulated_metrics: 0,
            metric_quota: DEFAULT_METRIC_QUOTA,
            status: ProbeStatus::Active,
        }
    }

    /// Evaluates the 5 deterministic circuit breakers to decide if the Knowledge Node has reached its end.
    pub fn evaluate_circuit_breakers(&mut self, doc: &ScrapedDocument) -> ProbeStatus {
        self.current_depth = self.current_depth.max(doc.hop_depth);

        // 1. Terminal PDF / DOI / ArXiv / .gov primary authority document check
        if Self::is_terminal_authority_doc(doc) && doc.bm25_relevance >= BM25_DRIFT_FLOOR {
            self.record_signature(&doc.minhash_sig);
            self.accepted_docs += 1;
            self.accumulated_metrics += count_empirical_metrics(&doc.clean_text).max(1);
            self.status = ProbeStatus::CompletedTerminal;
            return self.status;
        }

        // 2. Hard hop depth check
        if doc.hop_depth >= self.max_depth && self.max_depth > 0 {
            self.record_signature(&doc.minhash_sig);
            self.accepted_docs += 1;
            self.accumulated_metrics += count_empirical_metrics(&doc.clean_text).max(1);
            self.status = ProbeStatus::TerminatedMaxDepth;
            return self.status;
        }

        // 3. BM25 drift floor check (< 0.35)
        if doc.bm25_relevance < BM25_DRIFT_FLOOR {
            self.status = ProbeStatus::KilledDrift;
            return self.status;
        }

        // 4. MinHash vocabulary saturation check (> 0.75 Jaccard)
        let max_sim = self
            .seen_signatures
            .iter()
            .map(|prev| minhash_jaccard(prev, &doc.minhash_sig))
            .fold(0.0_f32, f32::max);
        if max_sim > MINHASH_SATURATION_CEILING {
            self.status = ProbeStatus::ExhaustedSaturated;
            return self.status;
        }

        // 5. Metric yield quota check
        self.record_signature(&doc.minhash_sig);
        self.accepted_docs += 1;
        self.accumulated_metrics += count_empirical_metrics(&doc.clean_text).max(1);

        if self.accumulated_metrics >= self.metric_quota && self.accepted_docs >= MIN_QUOTA_DOCS {
            self.status = ProbeStatus::CompletedQuotaMet;
        } else {
            self.status = ProbeStatus::Active;
        }
        self.status
    }

    fn record_signature(&mut self, sig: &[u64]) {
        if !sig.is_empty() {
            self.seen_signatures.push(sig.to_vec());
            self.seen_minhashes.push(sig[0]);
        }
    }

    fn is_terminal_authority_doc(doc: &ScrapedDocument) -> bool {
        let lower = doc.url.to_ascii_lowercase();
        lower.ends_with(".pdf")
            || lower.contains("doi.org/")
            || lower.contains("arxiv.org/abs/")
            || lower.contains("arxiv.org/pdf/")
            || lower.contains("sec.gov/archives/")
            || lower.contains("ncbi.nlm.nih.gov/")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::compute_minhash;

    fn make_doc(url: &str, hop_depth: u8, bm25: f32, text: &str) -> ScrapedDocument {
        ScrapedDocument {
            doc_id: Uuid::new_v4(),
            node_id: Uuid::new_v4(),
            url: url.into(),
            root_domain: "example.org".into(),
            branch_query: "solid state battery".into(),
            facet_slot: FacetSlot::Quantitative,
            hop_depth,
            bm25_relevance: bm25,
            minhash_sig: compute_minhash(text, 64),
            dom_sha256: "b".repeat(64),
            clean_text: text.into(),
        }
    }

    #[test]
    fn triggers_all_five_circuit_breakers() {
        let node_id = Uuid::new_v4();

        // 1. Terminal PDF/DOI
        let mut p1 = SearchProbe::new(node_id, "solid state".into(), FacetSlot::Quantitative, 2);
        let pdf_doc = make_doc("https://arxiv.org/pdf/2401.12345.pdf", 1, 0.72, "400 Wh/kg cell");
        assert_eq!(p1.evaluate_circuit_breakers(&pdf_doc), ProbeStatus::CompletedTerminal);

        // 2. MaxDepth
        let mut p2 = SearchProbe::new(node_id, "solid state".into(), FacetSlot::Quantitative, 2);
        let deep_doc = make_doc("https://example.org/deep", 2, 0.65, "Depth 2 leaf");
        assert_eq!(p2.evaluate_circuit_breakers(&deep_doc), ProbeStatus::TerminatedMaxDepth);

        // 3. Drift
        let mut p3 = SearchProbe::new(node_id, "solid state".into(), FacetSlot::Quantitative, 2);
        let drift_doc = make_doc("https://example.org/offtopic", 0, 0.20, "Baking bread recipe");
        assert_eq!(p3.evaluate_circuit_breakers(&drift_doc), ProbeStatus::KilledDrift);

        // 4. Saturation
        let mut p4 = SearchProbe::new(node_id, "solid state".into(), FacetSlot::Quantitative, 2);
        let base_text = "Solid state electrolytes achieve 450 Wh/kg energy density across 1000 cycles at room temperature in prototype cells.";
        let doc_a = make_doc("https://a.org/1", 0, 0.80, base_text);
        let doc_b = make_doc("https://b.org/2", 0, 0.78, base_text);
        assert_eq!(p4.evaluate_circuit_breakers(&doc_a), ProbeStatus::Active);
        assert_eq!(p4.evaluate_circuit_breakers(&doc_b), ProbeStatus::ExhaustedSaturated);

        // 5. Quota met
        let mut p5 = SearchProbe::new(node_id, "solid state".into(), FacetSlot::Quantitative, 2);
        let m1 = make_doc(
            "https://c.org/1",
            0,
            0.80,
            "Prototype achieved 450 Wh/kg, 90% capacity, $110 per kWh, 1500 cycles.",
        );
        let m2 = make_doc(
            "https://d.org/2",
            0,
            0.78,
            "Cathode thickness reached 120 ms pulse, 250 mAh capacity, 25 °C stability, 5 GW factory.",
        );
        assert_eq!(p5.evaluate_circuit_breakers(&m1), ProbeStatus::Active);
        assert_eq!(p5.evaluate_circuit_breakers(&m2), ProbeStatus::CompletedQuotaMet);
    }
}
