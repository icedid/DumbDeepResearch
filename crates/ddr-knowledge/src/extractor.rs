use ddr_crawler::ScrapedDocument;
use ddr_protocol::AtomicClaim;

/// Extracts atomic factual assertions and empirical benchmarks from scraped documents.
/// Supports both Pure Algorithmic extraction (LexRank + MMR + Hearst/Regex) and Local Micro-SLM JSON extraction.
pub struct ClaimExtractor;

impl ClaimExtractor {
    /// Pure classical extraction (0% hallucination): LexRank consensus + numerical regex gates + MMR.
    pub fn extract_algorithmic(_docs: &[ScrapedDocument], _max_claims: usize) -> Vec<AtomicClaim> {
        // Boilerplate stub
        Vec::new()
    }

    /// Optional local 1B-3B Micro-SLM extraction (via Ollama / ONNX) using a strict JSON schema.
    pub async fn extract_via_slm(_docs: &[ScrapedDocument], _max_claims: usize) -> Vec<AtomicClaim> {
        // Boilerplate stub
        Vec::new()
    }
}
