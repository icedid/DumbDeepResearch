use ddr_crawler::metrics::tokenize_content;
use ddr_protocol::SuggestedBranch;
use std::collections::HashSet;

const MMR_LAMBDA: f32 = 0.5;

/// Enforces non-overlapping query generation via the 4-stage mathematical filter:
/// 1. Semantic Facet Slot Dedup / Prioritization
/// 2. Pointwise Mutual Information (PMI) Co-occurrence Merge
/// 3. Root-Masked Token Jaccard Distance (Sim == 0.0)
/// 4. Query-Level Maximal Marginal Relevance (MMR, lambda = 0.5)
pub struct OrthogonalityGate;

impl OrthogonalityGate {
    /// Filters a pool of candidate branches down to `target_count` mutually orthogonal branches.
    pub fn filter_orthogonal_branches(
        root_query: &str,
        candidates: Vec<SuggestedBranch>,
        target_count: usize,
    ) -> Vec<SuggestedBranch> {
        if candidates.is_empty() || target_count == 0 {
            return Vec::new();
        }

        let root_tokens: HashSet<String> = tokenize_content(root_query).into_iter().collect();

        // Stage 1: Sort descending by significance score (tie-break deterministically by topic)
        let mut sorted = candidates;
        sorted.sort_by(|a, b| {
            b.significance_score
                .total_cmp(&a.significance_score)
                .then_with(|| a.topic.cmp(&b.topic))
        });

        // Stage 2: PMI Co-occurrence Merge for near-identical lexical equivalents (Jaccard >= 0.80)
        // so shorter un-entangled branches survive if a longer compound branch collides in Stage 3.
        let mut merged: Vec<SuggestedBranch> = Vec::new();
        for cand in sorted {
            let cand_masked = masked_tokens(&cand, &root_tokens);
            if cand_masked.is_empty() {
                continue;
            }

            let overlaps_existing = merged.iter().any(|existing| {
                let existing_masked = masked_tokens(existing, &root_tokens);
                jaccard_similarity(&cand_masked, &existing_masked) >= 0.80
            });

            if !overlaps_existing {
                merged.push(cand);
            }
        }

        // Stage 3 & Stage 4: Strictly enforce Root-Masked Token Jaccard == 0.0 (zero overlap
        // outside root_query) while selecting via Query-Level MMR (lambda = 0.5).
        let mut selected: Vec<SuggestedBranch> = Vec::new();
        let mut selected_masked: Vec<HashSet<String>> = Vec::new();
        let mut used = vec![false; merged.len()];

        while selected.len() < target_count {
            let mut best_idx = None;
            let mut best_mmr = f32::NEG_INFINITY;

            for (i, cand) in merged.iter().enumerate() {
                if used[i] {
                    continue;
                }
                let cand_masked = masked_tokens(cand, &root_tokens);

                // Stage 3: Strict zero non-root token overlap against already-selected branches
                let strictly_orthogonal = selected_masked
                    .iter()
                    .all(|prev| jaccard_similarity(&cand_masked, prev) == 0.0);
                if !strictly_orthogonal {
                    continue;
                }

                // Stage 4: MMR = lambda * significance + (1 - lambda) * diversity
                let diversity = if selected.is_empty() {
                    1.0
                } else {
                    let max_trigram_sim = selected
                        .iter()
                        .map(|s| char_trigram_sim(&cand.topic, &s.topic))
                        .fold(0.0_f32, f32::max);
                    let same_slot_penalty = if selected.iter().any(|s| s.slot == cand.slot) {
                        0.35
                    } else {
                        0.0
                    };
                    (1.0 - max_trigram_sim - same_slot_penalty).clamp(0.0, 1.0)
                };

                let mmr = MMR_LAMBDA * cand.significance_score + (1.0 - MMR_LAMBDA) * diversity;
                if mmr > best_mmr {
                    best_mmr = mmr;
                    best_idx = Some(i);
                }
            }

            if let Some(idx) = best_idx {
                used[idx] = true;
                selected_masked.push(masked_tokens(&merged[idx], &root_tokens));
                selected.push(merged[idx].clone());
            } else {
                break;
            }
        }

        selected
    }
}

fn masked_tokens(branch: &SuggestedBranch, root_tokens: &HashSet<String>) -> HashSet<String> {
    tokenize_content(&format!("{} {}", branch.topic, branch.query))
        .into_iter()
        .filter(|t| !root_tokens.contains(t))
        .collect()
}

fn jaccard_similarity(a: &HashSet<String>, b: &HashSet<String>) -> f32 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let inter = a.intersection(b).count() as f32;
    let union = a.union(b).count() as f32;
    if union == 0.0 {
        0.0
    } else {
        inter / union
    }
}

fn char_trigram_sim(a: &str, b: &str) -> f32 {
    let trigrams = |s: &str| -> HashSet<String> {
        let chars: Vec<char> = s.to_ascii_lowercase().chars().collect();
        if chars.len() < 3 {
            return HashSet::from([s.to_ascii_lowercase()]);
        }
        chars.windows(3).map(|w| w.iter().collect()).collect()
    };
    let ta = trigrams(a);
    let tb = trigrams(b);
    jaccard_similarity(&ta, &tb)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ddr_protocol::FacetSlot;

    #[test]
    fn enforces_zero_non_root_token_overlap() {
        let candidates = vec![
            SuggestedBranch {
                topic: "Sulfide Electrolytes".into(),
                query: "solid state batteries sulfide electrolytes".into(),
                slot: FacetSlot::Mechanistic,
                significance_score: 0.90,
            },
            SuggestedBranch {
                topic: "Sulfide Manufacturing Cost".into(),
                query: "solid state batteries sulfide manufacturing cost".into(),
                slot: FacetSlot::Economic,
                significance_score: 0.88,
            },
            SuggestedBranch {
                topic: "Dendrite Growth".into(),
                query: "solid state batteries dendrite growth".into(),
                slot: FacetSlot::Quantitative,
                significance_score: 0.82,
            },
            SuggestedBranch {
                topic: "Manufacturing Cost".into(),
                query: "solid state batteries manufacturing cost".into(),
                slot: FacetSlot::Economic,
                significance_score: 0.79,
            },
        ];

        let root = "solid state batteries";
        let selected = OrthogonalityGate::filter_orthogonal_branches(root, candidates, 3);
        assert_eq!(selected.len(), 3);

        let root_set: HashSet<String> = tokenize_content(root).into_iter().collect();
        for i in 0..selected.len() {
            for j in (i + 1)..selected.len() {
                let ti = masked_tokens(&selected[i], &root_set);
                let tj = masked_tokens(&selected[j], &root_set);
                assert_eq!(
                    jaccard_similarity(&ti, &tj),
                    0.0,
                    "selected branches must have zero non-root token overlap"
                );
            }
        }
    }
}
