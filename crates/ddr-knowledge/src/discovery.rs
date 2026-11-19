use ddr_crawler::metrics::tokenize_content;
use ddr_crawler::ScrapedDocument;
use ddr_protocol::{FacetSlot, SuggestedBranch};
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

const CONTRAST_MARKERS: &[&str] = &[
    "however",
    "but",
    "yet",
    "although",
    "despite",
    "whereas",
    "bottleneck",
    "degradation",
    "failure",
    "challenge",
    "limitation",
    "risk",
    "cost",
    "tradeoff",
    "trade-off",
    "drawback",
    "controversy",
    "barrier",
    "shortage",
    "problem",
    "versus",
    "instability",
    "critical",
    "constraint",
    "defect",
    "latency",
];

const GENERIC_WEB_NOISE: &[&str] = &[
    "article", "page", "site", "website", "click", "read", "more", "home", "search", "year",
    "years", "time", "data", "system", "research", "study", "paper", "report", "news", "http",
    "https", "html", "wiki", "wikipedia", "example",
];

/// Discovers new subtopics and tensions via Cascaded Pseudo-Relevance Feedback (PRF)
/// and Anchor-Windowed Discourse Contrast Matching.
pub struct CascadedDiscovery;

impl CascadedDiscovery {
    /// Step 2 (Micro Layer): Extracts statistically spiked terms (TF-IDF residual / KL-Divergence)
    /// from scraped documents, masking root query tokens.
    pub fn extract_prf_term_spikes(docs: &[ScrapedDocument], top_k: usize) -> Vec<String> {
        if docs.is_empty() || top_k == 0 {
            return Vec::new();
        }

        let query_mask: HashSet<String> = tokenize_content(&docs[0].branch_query)
            .into_iter()
            .chain(GENERIC_WEB_NOISE.iter().map(|s| (*s).to_string()))
            .collect();

        let mut fg_counts: HashMap<String, f32> = HashMap::new();
        let mut doc_freq: HashMap<String, usize> = HashMap::new();
        let mut total_fg = 0.0_f32;

        for doc in docs {
            let weight = (0.5 + doc.bm25_relevance).max(0.5);
            let tokens: Vec<String> = tokenize_content(&doc.clean_text)
                .into_iter()
                .filter(|t| t.len() >= 4 && !t.chars().all(|c| c.is_ascii_digit()))
                .collect();

            let mut seen_in_doc = HashSet::new();

            // Unigrams
            for tok in &tokens {
                if query_mask.contains(tok) {
                    continue;
                }
                *fg_counts.entry(tok.clone()).or_insert(0.0) += weight;
                total_fg += weight;
                if seen_in_doc.insert(tok.clone()) {
                    *doc_freq.entry(tok.clone()).or_insert(0) += 1;
                }
            }

            // Technical bigrams
            for pair in tokens.windows(2) {
                let (a, b) = (&pair[0], &pair[1]);
                if query_mask.contains(a) && query_mask.contains(b) {
                    continue;
                }
                if a == b {
                    continue;
                }
                let bigram = format!("{a} {b}");
                *fg_counts.entry(bigram.clone()).or_insert(0.0) += weight * 1.35;
                total_fg += weight * 1.35;
                if seen_in_doc.insert(bigram.clone()) {
                    *doc_freq.entry(bigram).or_insert(0) += 1;
                }
            }
        }

        if total_fg == 0.0 {
            return Vec::new();
        }

        let bg_floor = 1e-4_f32;
        let mut scored: Vec<(String, f32)> = fg_counts
            .into_iter()
            .filter_map(|(term, count)| {
                let p_fg = count / total_fg;
                if p_fg <= bg_floor {
                    return None;
                }
                let df = *doc_freq.get(&term).unwrap_or(&1) as f32;
                let kl = p_fg * (p_fg / bg_floor).ln();
                let df_boost = 1.0 + 0.5 * (1.0 + df).ln();
                let phrase_bonus = if term.contains(' ') { 1.15 } else { 1.0 };
                Some((term, kl * df_boost * phrase_bonus))
            })
            .collect();

        scored.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

        let mut selected: Vec<String> = Vec::new();
        for (term, _) in scored {
            let duplicate = selected
                .iter()
                .any(|existing| existing.contains(&term) || term.contains(existing));
            if !duplicate {
                selected.push(term);
            }
            if selected.len() >= top_k {
                break;
            }
        }

        selected
    }

    /// Step 3 (Polarity Layer): Scans a +/- 2 sentence window around each spiked term
    /// for contrastive discourse markers (`however`, `bottleneck`, `degradation`, `failure`).
    pub fn extract_windowed_contrasts(
        docs: &[ScrapedDocument],
        spiked_terms: &[String],
    ) -> Vec<(String, String)> {
        if docs.is_empty() || spiked_terms.is_empty() {
            return Vec::new();
        }

        let doc_sentences: Vec<Vec<String>> = docs
            .iter()
            .map(|d| split_raw_sentences(&d.clean_text))
            .collect();

        let mut results = Vec::new();

        for term in spiked_terms {
            let term_lower = term.to_ascii_lowercase();
            let mut best_clause: Option<(usize, String)> = None;

            for sentences in &doc_sentences {
                for (i, sent) in sentences.iter().enumerate() {
                    if !sent.to_ascii_lowercase().contains(&term_lower) {
                        continue;
                    }

                    let start = i.saturating_sub(2);
                    let end = (i + 3).min(sentences.len());

                    for window_sent in &sentences[start..end] {
                        let w_lower = window_sent.to_ascii_lowercase();
                        let marker_hits = CONTRAST_MARKERS
                            .iter()
                            .filter(|m| w_lower.contains(**m))
                            .count();

                        let score = marker_hits * 10 + usize::from(window_sent.len() > 50);
                        if best_clause
                            .as_ref()
                            .map(|(best_s, _)| score > *best_s)
                            .unwrap_or(true)
                        {
                            best_clause = Some((score, window_sent.clone()));
                        }
                    }
                }
            }

            let clause = best_clause
                .map(|(_, s)| s)
                .unwrap_or_else(|| format!("Investigate technical tradeoffs around {term}"));
            results.push((term.clone(), clause));
        }

        results
    }

    /// Synthesizes typed `SuggestedBranch` candidates from Cascaded PRF + Windowed Contrasts
    /// and Wave-1 autocomplete facets.
    pub fn synthesize_candidate_branches(
        root_query: &str,
        docs: &[ScrapedDocument],
        macro_facets: &[String],
        top_k: usize,
    ) -> Vec<SuggestedBranch> {
        let spikes = Self::extract_prf_term_spikes(docs, top_k.max(5));
        let contrasts = Self::extract_windowed_contrasts(docs, &spikes);

        let mut candidates = Vec::new();
        let total = contrasts.len().max(1);

        for (idx, (term, friction_clause)) in contrasts.into_iter().enumerate() {
            let slot = classify_facet_slot(&term, &friction_clause);
            let friction_bonus = if CONTRAST_MARKERS
                .iter()
                .any(|m| friction_clause.to_ascii_lowercase().contains(m))
            {
                0.15
            } else {
                0.0
            };
            let rank_score = 0.80 - (idx as f32 / total as f32) * 0.25 + friction_bonus;

            let topic = title_case(&term);
            let query = format!("{root_query} {term}");
            candidates.push(SuggestedBranch {
                topic,
                query,
                slot,
                significance_score: rank_score.clamp(0.45, 0.98),
            });
        }

        for (idx, facet) in macro_facets.iter().take(4).enumerate() {
            let trimmed = facet.trim();
            if trimmed.is_empty() {
                continue;
            }
            let slot = classify_facet_slot(trimmed, trimmed);
            candidates.push(SuggestedBranch {
                topic: title_case(trimmed),
                query: trimmed.to_string(),
                slot,
                significance_score: (0.68 - idx as f32 * 0.05).clamp(0.45, 0.90),
            });
        }

        candidates
    }
}

fn split_raw_sentences(text: &str) -> Vec<String> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"(?m)(?:\r?\n)+|[.!?]+\s+").expect("valid regex"));
    re.split(text)
        .map(str::trim)
        .filter(|s| s.len() >= 20)
        .map(ToString::to_string)
        .collect()
}

pub fn classify_facet_slot(term: &str, context: &str) -> FacetSlot {
    let combined = format!("{term} {context}").to_ascii_lowercase();
    if [
        "cost",
        "price",
        "economic",
        "market",
        "revenue",
        "margin",
        "capex",
        "supply",
        "manufacturing",
        "scale",
    ]
    .iter()
    .any(|k| combined.contains(k))
    {
        FacetSlot::Economic
    } else if [
        "density",
        "benchmark",
        "metric",
        "cycle",
        "voltage",
        "efficiency",
        "temperature",
        "capacity",
        "rate",
        "percent",
    ]
    .iter()
    .any(|k| combined.contains(k))
    {
        FacetSlot::Quantitative
    } else if [
        "versus",
        "vs",
        "compare",
        "comparison",
        "alternative",
        "conventional",
        "lithium",
        "tradeoff",
    ]
    .iter()
    .any(|k| combined.contains(k))
    {
        FacetSlot::Comparative
    } else if [
        "dendrite",
        "electrolyte",
        "interface",
        "mechanism",
        "architecture",
        "reaction",
        "cathode",
        "anode",
        "degradation",
        "cracking",
    ]
    .iter()
    .any(|k| combined.contains(k))
    {
        FacetSlot::Mechanistic
    } else {
        FacetSlot::Exploratory
    }
}

fn title_case(input: &str) -> String {
    input
        .split_whitespace()
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use ddr_crawler::metrics::compute_minhash;
    use uuid::Uuid;

    #[test]
    fn discovers_prf_spikes_and_contrast_windows() {
        let doc = ScrapedDocument {
            doc_id: Uuid::new_v4(),
            node_id: Uuid::new_v4(),
            url: "https://example.org/tech".into(),
            root_domain: "example.org".into(),
            branch_query: "solid state batteries".into(),
            facet_slot: FacetSlot::Exploratory,
            hop_depth: 0,
            bm25_relevance: 0.85,
            minhash_sig: compute_minhash("solid state batteries", 64),
            dom_sha256: "e".repeat(64),
            clean_text: "Sulfide electrolytes enable high ionic conductivity in prototype solid state batteries. \
                         However, sulfide electrolytes undergo severe moisture sensitivity and toxic gas evolution bottlenecks. \
                         Dendrite propagation penetrates grain boundaries at elevated current density. \
                         Manufacturing cost remains prohibitive compared to liquid lithium cells."
                .into(),
        };

        let spikes = CascadedDiscovery::extract_prf_term_spikes(std::slice::from_ref(&doc), 5);
        assert!(!spikes.is_empty());
        assert!(!spikes.iter().any(|s| s == "solid" || s == "state" || s == "batteries"));

        let contrasts = CascadedDiscovery::extract_windowed_contrasts(&[doc], &spikes);
        assert_eq!(contrasts.len(), spikes.len());
        assert!(contrasts
            .iter()
            .any(|(_, clause)| clause.contains("However") || clause.contains("cost") || clause.contains("Dendrite")));
    }
}
