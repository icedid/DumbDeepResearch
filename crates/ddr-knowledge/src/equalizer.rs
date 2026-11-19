use ddr_crawler::metrics::tokenize_content;
use ddr_protocol::{AtomicClaim, Polarity};
use std::collections::HashMap;

const DEFAULT_EQUALIZED_CLAIMS: usize = 6;
const DEDUP_COSINE_CEILING: f32 = 0.65;

/// Diagnostic summary returned alongside equalized claims.
#[derive(Debug, Clone)]
pub struct EqualizerReport {
    pub claims: Vec<AtomicClaim>,
    pub bull_count: usize,
    pub bear_count: usize,
    pub neutral_count: usize,
    pub asymmetry_flag: Option<String>,
}

/// Enforces the 50/50 perspective ratio between Bull (affirmative) and Bear (critical) assertions
/// alongside neutral empirical metrics.
pub struct PerspectiveEqualizer;

impl PerspectiveEqualizer {
    /// Balances a pool of extracted claims into a 1:1 affirmative-to-critical ratio using Bipolar MMR.
    pub fn balance_claims(claims: Vec<AtomicClaim>) -> Vec<AtomicClaim> {
        Self::balance_claims_with_report(claims, DEFAULT_EQUALIZED_CLAIMS).claims
    }

    /// Assigns semantic cluster IDs, deduplicates redundant promotional assertions via Bipolar MMR,
    /// enforces 1:1 Bull vs. Bear parity, and flags structural information asymmetry.
    pub fn balance_claims_with_report(
        mut claims: Vec<AtomicClaim>,
        max_output: usize,
    ) -> EqualizerReport {
        if claims.is_empty() || max_output == 0 {
            return EqualizerReport {
                claims: Vec::new(),
                bull_count: 0,
                bear_count: 0,
                neutral_count: 0,
                asymmetry_flag: None,
            };
        }

        // Step 1: Dimensional clustering across claim statements
        assign_dimensional_clusters(&mut claims);

        // Step 2: Partition by polarity and deduplicate within each pole via MMR
        let mut bull_raw = Vec::new();
        let mut bear_raw = Vec::new();
        let mut neutral_raw = Vec::new();

        for claim in claims {
            match claim.polarity {
                Polarity::Bull => bull_raw.push(claim),
                Polarity::Bear => bear_raw.push(claim),
                Polarity::NeutralMetric => neutral_raw.push(claim),
            }
        }

        let bull_pool = deduplicate_pole(bull_raw);
        let bear_pool = deduplicate_pole(bear_raw);
        let neutral_pool = deduplicate_pole(neutral_raw);

        let raw_bull_len = bull_pool.len();
        let raw_bear_len = bear_pool.len();

        let mut selected_bull = Vec::new();
        let mut selected_bear = Vec::new();
        let mut asymmetry_flag = None;

        // Step 3: Enforce 1:1 Bull / Bear quota or flag structural information asymmetry
        if raw_bull_len > 0 && raw_bear_len > 0 {
            let pair_quota = raw_bull_len
                .min(raw_bear_len)
                .min((max_output / 2).max(1));

            selected_bull.extend(bull_pool.into_iter().take(pair_quota));
            selected_bear.extend(bear_pool.into_iter().take(pair_quota));

            let pruned_excess = (raw_bull_len - pair_quota) + (raw_bear_len - pair_quota);
            if raw_bull_len > raw_bear_len * 2 || raw_bear_len > raw_bull_len * 2 {
                asymmetry_flag = Some(format!(
                    "Equalized perspective skew: pruned {pruned_excess} excess claim(s) to enforce 1:1 Bull/Bear parity ({pair_quota}:{pair_quota})"
                ));
            }
        } else if raw_bull_len > 0 || raw_bear_len > 0 {
            asymmetry_flag = Some(format!(
                "Structural information asymmetry: {raw_bull_len} Bull vs {raw_bear_len} Bear claims found across scraped sources"
            ));
            // Cap one-sided assertions at 2 to prevent unchallenged echo-chamber dominance.
            selected_bull.extend(bull_pool.into_iter().take(2));
            selected_bear.extend(bear_pool.into_iter().take(2));
        }

        // Step 4: Fill remaining capacity with NeutralMetric empirical claims
        let used_slots = selected_bull.len() + selected_bear.len();
        let neutral_budget = max_output.saturating_sub(used_slots);
        let selected_neutral: Vec<AtomicClaim> =
            neutral_pool.into_iter().take(neutral_budget).collect();

        // Step 5: Interleave deterministically (Bull, Bear, Neutral)
        let mut final_claims = Vec::with_capacity(
            selected_bull.len() + selected_bear.len() + selected_neutral.len(),
        );
        let max_len = selected_bull
            .len()
            .max(selected_bear.len())
            .max(selected_neutral.len());

        for i in 0..max_len {
            if let Some(mut c) = selected_bull.get(i).cloned() {
                c.is_selected = true;
                final_claims.push(c);
            }
            if let Some(mut c) = selected_bear.get(i).cloned() {
                c.is_selected = true;
                final_claims.push(c);
            }
            if let Some(mut c) = selected_neutral.get(i).cloned() {
                c.is_selected = true;
                final_claims.push(c);
            }
        }

        let bull_count = final_claims
            .iter()
            .filter(|c| c.polarity == Polarity::Bull)
            .count();
        let bear_count = final_claims
            .iter()
            .filter(|c| c.polarity == Polarity::Bear)
            .count();
        let neutral_count = final_claims
            .iter()
            .filter(|c| c.polarity == Polarity::NeutralMetric)
            .count();

        EqualizerReport {
            claims: final_claims,
            bull_count,
            bear_count,
            neutral_count,
            asymmetry_flag,
        }
    }
}

fn assign_dimensional_clusters(claims: &mut [AtomicClaim]) {
    let k = (claims.len() / 3).clamp(1, 3);
    if k == 1 {
        for c in claims.iter_mut() {
            c.cluster_id = Some(0);
        }
        return;
    }

    let token_vecs: Vec<Vec<String>> = claims
        .iter()
        .map(|c| tokenize_content(&c.statement))
        .collect();

    // Deterministic farthest-first traversal centroids
    let mut centroid_indices = vec![0usize];
    while centroid_indices.len() < k {
        let mut farthest_idx = None;
        let mut max_min_dist = -1.0_f32;

        for (i, vec_i) in token_vecs.iter().enumerate() {
            if centroid_indices.contains(&i) {
                continue;
            }
            let min_dist = centroid_indices
                .iter()
                .map(|&c_idx| 1.0 - token_cosine(vec_i, &token_vecs[c_idx]))
                .fold(f32::INFINITY, f32::min);
            if min_dist > max_min_dist {
                max_min_dist = min_dist;
                farthest_idx = Some(i);
            }
        }
        if let Some(idx) = farthest_idx {
            centroid_indices.push(idx);
        } else {
            break;
        }
    }

    for (i, claim) in claims.iter_mut().enumerate() {
        let mut best_cluster = 0usize;
        let mut best_sim = f32::NEG_INFINITY;
        for (c_num, &c_idx) in centroid_indices.iter().enumerate() {
            let sim = token_cosine(&token_vecs[i], &token_vecs[c_idx]);
            if sim > best_sim {
                best_sim = sim;
                best_cluster = c_num;
            }
        }
        claim.cluster_id = Some(best_cluster);
    }
}

fn deduplicate_pole(mut pool: Vec<AtomicClaim>) -> Vec<AtomicClaim> {
    pool.sort_by(|a, b| {
        b.confidence
            .total_cmp(&a.confidence)
            .then_with(|| a.statement.cmp(&b.statement))
    });

    let mut kept: Vec<(AtomicClaim, Vec<String>)> = Vec::new();
    for claim in pool {
        let tokens = tokenize_content(&claim.statement);
        let too_similar = kept
            .iter()
            .any(|(_, prev_tokens)| token_cosine(&tokens, prev_tokens) > DEDUP_COSINE_CEILING);
        if !too_similar {
            kept.push((claim, tokens));
        }
    }

    kept.into_iter().map(|(c, _)| c).collect()
}

fn token_cosine(a: &[String], b: &[String]) -> f32 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let mut map_a: HashMap<&str, f32> = HashMap::new();
    let mut map_b: HashMap<&str, f32> = HashMap::new();
    for t in a {
        *map_a.entry(t.as_str()).or_insert(0.0) += 1.0;
    }
    for t in b {
        *map_b.entry(t.as_str()).or_insert(0.0) += 1.0;
    }
    let dot: f32 = map_a
        .iter()
        .filter_map(|(k, v)| map_b.get(k).map(|vb| v * vb))
        .sum();
    let na: f32 = map_a.values().map(|v| v * v).sum::<f32>().sqrt();
    let nb: f32 = map_b.values().map(|v| v * v).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 {
        0.0
    } else {
        dot / (na * nb)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn claim(stmt: &str, polarity: Polarity, conf: f32) -> AtomicClaim {
        AtomicClaim {
            id: Uuid::new_v4(),
            source_url: "https://example.org".into(),
            dom_sha256: "d".repeat(64),
            subject: "Battery".into(),
            statement: stmt.into(),
            polarity,
            confidence: conf,
            cluster_id: None,
            is_selected: false,
        }
    }

    #[test]
    fn enforces_one_to_one_bull_bear_parity() {
        let input = vec![
            claim("Solid state electrolytes improve thermal stability across cycles.", Polarity::Bull, 0.92),
            claim("Sulfide anodes accelerate fast charging performance significantly.", Polarity::Bull, 0.90),
            claim("Ceramic separators increase volumetric energy density in prototypes.", Polarity::Bull, 0.88),
            claim("High stack pressure requirements raise pack weight and cost.", Polarity::Bear, 0.91),
            claim("Measured energy density reached 450 Wh/kg at 25C.", Polarity::NeutralMetric, 0.85),
        ];

        let report = PerspectiveEqualizer::balance_claims_with_report(input, 6);
        assert_eq!(report.bull_count, 1);
        assert_eq!(report.bear_count, 1);
        assert_eq!(report.neutral_count, 1);
        assert!(report.asymmetry_flag.is_some());
        assert!(report.claims.iter().all(|c| c.cluster_id.is_some() && c.is_selected));
    }

    #[test]
    fn flags_structural_asymmetry_when_bear_missing() {
        let input = vec![
            claim("Solid state electrolytes improve thermal stability.", Polarity::Bull, 0.92),
            claim("Fast charging performance scales well at lab scale.", Polarity::Bull, 0.89),
            claim("Manufacturing yield benefits from dry coating.", Polarity::Bull, 0.86),
        ];

        let report = PerspectiveEqualizer::balance_claims_with_report(input, 6);
        assert!(report.bull_count <= 2);
        assert_eq!(report.bear_count, 0);
        assert!(report
            .asymmetry_flag
            .as_deref()
            .unwrap_or("")
            .contains("Structural information asymmetry"));
    }
}
