use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

const STOPWORDS: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "been", "being", "by", "can", "could", "did",
    "do", "does", "for", "from", "had", "has", "have", "he", "her", "here", "hers", "him", "his",
    "how", "i", "if", "in", "into", "is", "it", "its", "may", "might", "more", "most", "much",
    "must", "my", "no", "not", "of", "on", "or", "other", "our", "out", "over", "she", "should",
    "so", "some", "such", "than", "that", "the", "their", "them", "then", "there", "these",
    "they", "this", "those", "to", "too", "under", "up", "use", "used", "using", "very", "was",
    "we", "were", "what", "when", "where", "which", "while", "who", "will", "with", "would",
    "you", "your",
];

fn normalize_token(raw: &str) -> String {
    let lower = raw.to_ascii_lowercase();
    if lower.len() > 5 && lower.ends_with("ies") {
        format!("{}y", &lower[..lower.len() - 3])
    } else if lower.len() > 4 && lower.ends_with('s') && !lower.ends_with("ss") {
        lower[..lower.len() - 1].to_string()
    } else {
        lower
    }
}

/// Tokenizes text into normalized non-stopword alphanumeric terms.
pub fn tokenize_content(text: &str) -> Vec<String> {
    let stop: HashSet<&str> = STOPWORDS.iter().copied().collect();
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() >= 2)
        .map(normalize_token)
        .filter(|w| !stop.contains(w.as_str()))
        .collect()
}

/// Counts whitespace-delimited words for quality gating.
pub fn word_count(text: &str) -> usize {
    text.split_whitespace().count()
}

/// Computes normalized Okapi BM25 relevance in `[0.0, 1.0]` combining term-frequency
/// saturation (`k1 = 1.5, b = 0.75, avgdl = 450`) and query term coverage.
pub fn compute_bm25_relevance(query: &str, text: &str) -> f32 {
    let query_terms: Vec<String> = {
        let mut seen = HashSet::new();
        tokenize_content(query)
            .into_iter()
            .filter(|t| seen.insert(t.clone()))
            .collect()
    };
    if query_terms.is_empty() {
        return 0.0;
    }

    let doc_tokens = tokenize_content(text);
    if doc_tokens.is_empty() {
        return 0.0;
    }

    let mut tf_map: HashMap<String, usize> = HashMap::new();
    for tok in &doc_tokens {
        *tf_map.entry(tok.clone()).or_insert(0) += 1;
    }

    let dl = doc_tokens.len() as f32;
    let avgdl = 450.0_f32;
    let k1 = 1.5_f32;
    let b = 0.75_f32;
    let norm_denom = k1 + 1.0;

    let mut sum_tf_norm = 0.0_f32;
    let mut matched = 0usize;

    for term in &query_terms {
        let tf = *tf_map.get(term).unwrap_or(&0) as f32;
        if tf > 0.0 {
            matched += 1;
            let bm25_tf = (tf * (k1 + 1.0)) / (tf + k1 * (1.0 - b + b * (dl / avgdl)));
            sum_tf_norm += bm25_tf / norm_denom;
        }
    }

    let avg_tf_norm = sum_tf_norm / query_terms.len() as f32;
    let coverage = matched as f32 / query_terms.len() as f32;
    (0.6 * avg_tf_norm + 0.4 * coverage).clamp(0.0, 1.0)
}

fn fnv1a_64(data: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for &b in data {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

/// Computes a deterministic MinHash signature over 3-word shingles.
pub fn compute_minhash(text: &str, num_hashes: usize) -> Vec<u64> {
    if num_hashes == 0 {
        return Vec::new();
    }
    let tokens = tokenize_content(text);
    if tokens.is_empty() {
        return vec![0; num_hashes];
    }

    let mut shingle_hashes: Vec<u64> = if tokens.len() >= 3 {
        tokens
            .windows(3)
            .map(|w| {
                let joined = format!("{} {} {}", w[0], w[1], w[2]);
                fnv1a_64(joined.as_bytes())
            })
            .collect()
    } else {
        tokens.iter().map(|t| fnv1a_64(t.as_bytes())).collect()
    };
    shingle_hashes.sort_unstable();
    shingle_hashes.dedup();

    (0..num_hashes)
        .map(|seed| {
            let salt = splitmix64((seed as u64).wrapping_add(1));
            shingle_hashes
                .iter()
                .map(|&h| splitmix64(h ^ salt))
                .min()
                .unwrap_or(0)
        })
        .collect()
}

/// Estimates Jaccard similarity in `[0.0, 1.0]` from two equal-length MinHash signatures.
pub fn minhash_jaccard(sig_a: &[u64], sig_b: &[u64]) -> f32 {
    if sig_a.is_empty() || sig_b.is_empty() || sig_a.len() != sig_b.len() {
        return 0.0;
    }
    let matches = sig_a
        .iter()
        .zip(sig_b.iter())
        .filter(|(a, b)| a == b)
        .count();
    matches as f32 / sig_a.len() as f32
}

/// Counts distinct quantitative / empirical metric expressions in text.
pub fn count_empirical_metrics(text: &str) -> usize {
    static METRIC_RE: OnceLock<Regex> = OnceLock::new();
    let re = METRIC_RE.get_or_init(|| {
        Regex::new(r"(?i)(?:\$\s*\d+(?:[.,]\d+)?|\b\d+(?:[.,]\d+)?\s*(?:%|\b(?:percent|wh/kg|wh/l|mah|gwh|mwh|kwh|kw|mw|gw|ms|hz|ghz|gb|tb|kg|km|cycles|billion|million|thousand|times|x|°c|°f|kelvin|usd|eur|years?|months?)\b))")
            .expect("valid metric regex")
    });

    let unique: HashSet<String> = re
        .find_iter(text)
        .map(|m| m.as_str().to_ascii_lowercase())
        .collect();
    unique.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bm25_separates_relevant_from_off_topic() {
        let query = "solid state battery thermal runaway";
        let relevant = "Solid state battery architectures reduce thermal runaway risks in electric vehicles. Testing shows solid state battery electrolytes tolerate higher temperatures without liquid leakage.";
        let unrelated = "Artisan sourdough fermentation relies on wild lactobacillus cultures, ambient hydration percentages, and Dutch oven steam retention during baking.";

        let rel_score = compute_bm25_relevance(query, relevant);
        let irrel_score = compute_bm25_relevance(query, unrelated);

        assert!(rel_score >= 0.35, "expected relevant score >= 0.35, got {rel_score}");
        assert!(irrel_score < 0.35, "expected unrelated score < 0.35, got {irrel_score}");
    }

    #[test]
    fn minhash_captures_near_duplicates() {
        let a = "Solid state lithium metal batteries achieve 450 Wh/kg energy density under laboratory testing with sulfide electrolytes and ceramic separators.";
        let b = "Solid state lithium metal batteries achieve 450 Wh/kg energy density under laboratory testing with sulfide electrolytes and ceramic separators across trials.";
        let c = "Offshore wind turbine foundations require deep monopile driving in North Sea clay strata to resist hydrodynamic cyclic loads.";

        let sig_a = compute_minhash(a, 64);
        let sig_b = compute_minhash(b, 64);
        let sig_c = compute_minhash(c, 64);

        let sim_ab = minhash_jaccard(&sig_a, &sig_b);
        let sim_ac = minhash_jaccard(&sig_a, &sig_c);

        assert!(sim_ab > 0.75, "expected near-duplicate Jaccard > 0.75, got {sim_ab}");
        assert!(sim_ac < 0.25, "expected distinct text Jaccard < 0.25, got {sim_ac}");
    }

    #[test]
    fn counts_empirical_metrics() {
        let text = "Capacity reached 420 Wh/kg with 85% retention over 1,200 cycles at $95 per kWh.";
        assert!(count_empirical_metrics(text) >= 4);
    }
}
