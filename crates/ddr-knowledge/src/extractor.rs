use ddr_crawler::metrics::tokenize_content;
use ddr_crawler::ScrapedDocument;
use ddr_protocol::{AtomicClaim, Polarity};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;
use std::time::Duration;
use tracing::debug;
use uuid::Uuid;

const BULL_CUES: &[&str] = &[
    "advantage",
    "advantages",
    "breakthrough",
    "breakthroughs",
    "improve",
    "improved",
    "improves",
    "improvement",
    "superior",
    "faster",
    "higher",
    "record",
    "growth",
    "promising",
    "efficient",
    "efficiency",
    "milestone",
    "commercial",
    "outperform",
    "outperforms",
    "benefit",
    "benefits",
    "safer",
    "safety",
    "longer",
    "scalable",
    "innovation",
    "achieved",
    "exceeds",
    "enables",
    "eliminates",
    "solves",
    "doubles",
    "triples",
    "strong",
    "leading",
    "resilient",
    "viable",
];

const BEAR_CUES: &[&str] = &[
    "fail",
    "failed",
    "failure",
    "failures",
    "risk",
    "risks",
    "bottleneck",
    "bottlenecks",
    "degradation",
    "degrade",
    "degrades",
    "limitation",
    "limitations",
    "limited",
    "expensive",
    "costly",
    "lawsuit",
    "lawsuits",
    "litigation",
    "recall",
    "hazard",
    "hazards",
    "danger",
    "challenge",
    "challenges",
    "obstacle",
    "criticism",
    "criticized",
    "delay",
    "delays",
    "shortage",
    "shortages",
    "dendrite",
    "dendrites",
    "cracking",
    "instability",
    "unstable",
    "slower",
    "worse",
    "barrier",
    "barriers",
    "vulnerability",
    "concern",
    "concerns",
    "problem",
    "problems",
    "drawback",
    "drawbacks",
    "deficit",
    "decline",
    "however",
    "despite",
    "prevent",
    "prevents",
    "prohibitive",
    "flaw",
    "flaws",
    "penalty",
];

const NOISE_PATTERNS: &[&str] = &[
    "cookie",
    "all rights reserved",
    "privacy policy",
    "terms of service",
    "subscribe",
    "sign in",
    "sign up",
    "javascript",
    "click here",
    "read more",
    "advertisement",
];

#[derive(Clone)]
struct CandidateSentence {
    doc_idx: usize,
    sentence: String,
    tokens: Vec<String>,
    score: f32,
    polarity: Polarity,
    confidence: f32,
    subject: String,
}

#[derive(Serialize)]
struct OllamaRequest<'a> {
    model: &'a str,
    prompt: String,
    stream: bool,
    format: &'a str,
    options: OllamaOptions,
}

#[derive(Serialize)]
struct OllamaOptions {
    temperature: f32,
}

#[derive(Deserialize)]
struct OllamaGenerateResponse {
    response: String,
}

#[derive(Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaModelInfo>,
}

#[derive(Deserialize)]
struct OllamaModelInfo {
    name: String,
}

#[derive(Deserialize)]
struct SlmClaimPayload {
    #[serde(default)]
    claims: Vec<SlmClaimItem>,
}

#[derive(Deserialize)]
struct SlmClaimItem {
    #[serde(default)]
    subject: String,
    statement: String,
    #[serde(default)]
    polarity: String,
    #[serde(default)]
    source_index: usize,
    #[serde(default = "default_slm_conf")]
    confidence: f32,
}

fn default_slm_conf() -> f32 {
    0.85
}

fn split_sentences(text: &str) -> Vec<String> {
    static SENT_RE: OnceLock<Regex> = OnceLock::new();
    let re = SENT_RE.get_or_init(|| Regex::new(r"(?m)(?:\r?\n)+|[.!?]+\s+").expect("valid regex"));

    re.split(text)
        .map(str::trim)
        .filter(|s| {
            let len = s.len();
            let words = s.split_whitespace().count();
            let lower = s.to_ascii_lowercase();
            (40..=340).contains(&len)
                && words >= 6
                && !NOISE_PATTERNS.iter().any(|p| lower.contains(p))
        })
        .map(|s| {
            if s.ends_with('.') || s.ends_with('!') || s.ends_with('?') {
                s.to_string()
            } else {
                format!("{s}.")
            }
        })
        .collect()
}

fn has_empirical_metric(sentence: &str) -> bool {
    static METRIC_RE: OnceLock<Regex> = OnceLock::new();
    let re = METRIC_RE.get_or_init(|| {
        Regex::new(r"(?i)(?:\$\s*\d+(?:[.,]\d+)?|\b\d+(?:[.,]\d+)?\s*(?:%|\b(?:percent|wh/kg|wh/l|mah|gwh|mwh|kwh|kw|mw|gw|ms|hz|ghz|gb|tb|kg|km|cycles|billion|million|thousand|times|x|°c|°f|kelvin|usd|eur|years?|months?)\b))")
            .expect("valid metric regex")
    });
    re.is_match(sentence)
}

fn has_hearst_predicate(sentence: &str) -> bool {
    let lower = sentence.to_ascii_lowercase();
    const PREDICATES: &[&str] = &[
        "demonstrated",
        "measured",
        "reported",
        "achieved",
        "reduced",
        "increased",
        "compared to",
        "caused by",
        "limited by",
        "results in",
        "outperforms",
        "degrades",
        "exhibits",
        "requires",
        "accounts for",
    ];
    PREDICATES.iter().any(|p| lower.contains(p))
}

/// Deterministically classifies a sentence into `Bull`, `Bear`, or `NeutralMetric`
/// and computes extraction confidence and a concise subject label.
pub fn classify_polarity(sentence: &str, query_hint: &str) -> (Polarity, f32, String) {
    let words: Vec<String> = sentence
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| w.to_ascii_lowercase())
        .collect();

    let bull_hits = words
        .iter()
        .filter(|w| BULL_CUES.contains(&w.as_str()))
        .count();
    let bear_hits = words
        .iter()
        .filter(|w| BEAR_CUES.contains(&w.as_str()))
        .count();
    let metric = has_empirical_metric(sentence);

    let polarity = if bear_hits > bull_hits {
        Polarity::Bear
    } else if bull_hits > bear_hits {
        Polarity::Bull
    } else if metric || bull_hits == 0 {
        Polarity::NeutralMetric
    } else {
        Polarity::Bull
    };

    let signal_strength = (bull_hits + bear_hits) as f32 * 0.08 + if metric { 0.14 } else { 0.0 };
    let confidence = (0.68 + signal_strength).clamp(0.60, 0.98);

    let subject_tokens: Vec<&str> = query_hint
        .split_whitespace()
        .filter(|w| w.len() > 2)
        .take(3)
        .collect();
    let subject = if subject_tokens.is_empty() {
        sentence
            .split_whitespace()
            .take(3)
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        subject_tokens.join(" ")
    };

    (polarity, confidence, subject)
}

fn cosine_similarity(a: &[String], b: &[String]) -> f32 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let mut tf_a: HashMap<&str, f32> = HashMap::new();
    let mut tf_b: HashMap<&str, f32> = HashMap::new();
    for t in a {
        *tf_a.entry(t.as_str()).or_insert(0.0) += 1.0;
    }
    for t in b {
        *tf_b.entry(t.as_str()).or_insert(0.0) += 1.0;
    }

    let dot: f32 = tf_a
        .iter()
        .filter_map(|(k, v)| tf_b.get(k).map(|vb| v * vb))
        .sum();
    let norm_a: f32 = tf_a.values().map(|v| v * v).sum::<f32>().sqrt();
    let norm_b: f32 = tf_b.values().map(|v| v * v).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}

/// Extracts atomic factual assertions and empirical benchmarks from scraped documents.
/// Supports both Pure Algorithmic extraction (LexRank + MMR + Hearst/Regex) and Local Micro-SLM JSON extraction.
pub struct ClaimExtractor;

impl ClaimExtractor {
    /// Pure classical extraction (0% hallucination): LexRank consensus + numerical regex gates + MMR.
    pub fn extract_algorithmic(docs: &[ScrapedDocument], max_claims: usize) -> Vec<AtomicClaim> {
        if docs.is_empty() || max_claims == 0 {
            return Vec::new();
        }

        let mut candidates = Vec::new();
        let mut seen_sentences = HashSet::new();

        for (doc_idx, doc) in docs.iter().enumerate() {
            let query_tokens: HashSet<String> =
                tokenize_content(&doc.branch_query).into_iter().collect();

            for sent in split_sentences(&doc.clean_text) {
                let norm_key = sent.to_ascii_lowercase();
                if !seen_sentences.insert(norm_key) {
                    continue;
                }
                let tokens = tokenize_content(&sent);
                if tokens.len() < 4 {
                    continue;
                }

                let overlap_count = tokens
                    .iter()
                    .filter(|t| query_tokens.contains(t.as_str()))
                    .count();
                let query_overlap = if query_tokens.is_empty() {
                    0.0
                } else {
                    overlap_count as f32 / query_tokens.len() as f32
                };

                let metric_bonus = if has_empirical_metric(&sent) { 0.35 } else { 0.0 };
                let hearst_bonus = if has_hearst_predicate(&sent) { 0.15 } else { 0.0 };
                let doc_bm25_bonus = doc.bm25_relevance * 0.25;

                let (polarity, confidence, subject) = classify_polarity(&sent, &doc.branch_query);

                candidates.push(CandidateSentence {
                    doc_idx,
                    sentence: sent,
                    tokens,
                    score: metric_bonus + hearst_bonus + doc_bm25_bonus + query_overlap * 0.25,
                    polarity,
                    confidence,
                    subject,
                });
            }
        }

        if candidates.is_empty() {
            return Vec::new();
        }

        // Cap candidate pool for bounded O(N^2) LexRank runtime
        candidates.sort_by(|a, b| b.score.total_cmp(&a.score));
        candidates.truncate(60);

        let n = candidates.len();
        let mut adj = vec![vec![0.0_f32; n]; n];
        let mut row_sums = vec![0.0_f32; n];

        for i in 0..n {
            for j in (i + 1)..n {
                let sim = cosine_similarity(&candidates[i].tokens, &candidates[j].tokens);
                if sim >= 0.10 {
                    adj[i][j] = sim;
                    adj[j][i] = sim;
                    row_sums[i] += sim;
                    row_sums[j] += sim;
                }
            }
        }

        // Power-method LexRank (12 iterations, damping = 0.85)
        let mut lexrank = vec![1.0_f32 / n as f32; n];
        let damping = 0.85_f32;
        for _ in 0..12 {
            let mut next = vec![(1.0 - damping) / n as f32; n];
            for i in 0..n {
                for j in 0..n {
                    if adj[j][i] > 0.0 && row_sums[j] > 0.0 {
                        next[i] += damping * lexrank[j] * (adj[j][i] / row_sums[j]);
                    }
                }
            }
            lexrank = next;
        }

        let max_lr = lexrank.iter().copied().fold(0.0_f32, f32::max).max(1e-6);
        for (i, cand) in candidates.iter_mut().enumerate() {
            cand.score += 0.45 * (lexrank[i] / max_lr);
        }

        // Polarity-stratified MMR selection so Bull, Bear, and NeutralMetric assertions
        // all enter the Equalizer pool without early drowning.
        let mut selected_indices: Vec<usize> = Vec::new();
        let mut used = vec![false; n];
        let lambda = 0.7_f32;

        let target_buckets = [Polarity::Bull, Polarity::Bear, Polarity::NeutralMetric];
        let mut bucket_cursor = 0usize;

        while selected_indices.len() < max_claims && selected_indices.len() < n {
            let desired_polarity = target_buckets[bucket_cursor % target_buckets.len()];
            bucket_cursor += 1;

            let pick = select_mmr_candidate(
                &candidates,
                &used,
                &selected_indices,
                Some(desired_polarity),
                lambda,
            )
            .or_else(|| {
                select_mmr_candidate(&candidates, &used, &selected_indices, None, lambda)
            });

            if let Some(idx) = pick {
                used[idx] = true;
                selected_indices.push(idx);
            } else {
                break;
            }
        }

        selected_indices
            .into_iter()
            .map(|idx| {
                let cand = &candidates[idx];
                let doc = &docs[cand.doc_idx];
                AtomicClaim {
                    id: Uuid::new_v4(),
                    source_url: doc.url.clone(),
                    dom_sha256: doc.dom_sha256.clone(),
                    subject: cand.subject.clone(),
                    statement: cand.sentence.clone(),
                    polarity: cand.polarity,
                    confidence: cand.confidence,
                    cluster_id: None,
                    is_selected: true,
                }
            })
            .collect()
    }

    /// Optional local 1B-3B/14B Micro-SLM extraction (via Ollama) using a strict JSON schema,
    /// automatically falling back to `extract_algorithmic` if unavailable or invalid.
    pub async fn extract_via_slm(docs: &[ScrapedDocument], max_claims: usize) -> Vec<AtomicClaim> {
        if docs.is_empty() || max_claims == 0 {
            return Vec::new();
        }

        let base_url = std::env::var("DDR_OLLAMA_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:11434".to_string())
            .trim_end_matches('/')
            .to_string();

        let client = match reqwest::Client::builder()
            .timeout(Duration::from_secs(12))
            .build()
        {
            Ok(c) => c,
            Err(_) => return Self::extract_algorithmic(docs, max_claims),
        };

        let model = match resolve_ollama_model(&client, &base_url).await {
            Some(m) => m,
            None => return Self::extract_algorithmic(docs, max_claims),
        };

        let mut context_blocks = Vec::new();
        for (idx, doc) in docs.iter().take(4).enumerate() {
            let excerpt: String = doc.clean_text.chars().take(1200).collect();
            context_blocks.push(format!("[SOURCE {idx}] ({})\n{}", doc.url, excerpt));
        }

        let prompt = format!(
            "Extract up to {max_claims} atomic, verifiable factual claims from the sources below.\n\
             Return ONLY valid JSON matching: {{\"claims\": [{{\"subject\": string, \"statement\": string, \"polarity\": \"Bull\"|\"Bear\"|\"NeutralMetric\", \"source_index\": integer, \"confidence\": number}}]}}.\n\
             Balance affirmative (Bull), critical/risk (Bear), and numeric benchmark (NeutralMetric) claims.\n\n{}",
            context_blocks.join("\n\n")
        );

        let req_body = OllamaRequest {
            model: &model,
            prompt,
            stream: false,
            format: "json",
            options: OllamaOptions { temperature: 0.0 },
        };

        let Ok(resp) = client
            .post(format!("{base_url}/api/generate"))
            .json(&req_body)
            .send()
            .await
        else {
            debug!("Ollama request failed; falling back to algorithmic claim extractor");
            return Self::extract_algorithmic(docs, max_claims);
        };

        if !resp.status().is_success() {
            return Self::extract_algorithmic(docs, max_claims);
        }

        let Ok(gen_resp) = resp.json::<OllamaGenerateResponse>().await else {
            return Self::extract_algorithmic(docs, max_claims);
        };

        let Ok(payload) = serde_json::from_str::<SlmClaimPayload>(&gen_resp.response) else {
            debug!("Ollama returned non-schema JSON; falling back to algorithmic extractor");
            return Self::extract_algorithmic(docs, max_claims);
        };

        let parsed: Vec<AtomicClaim> = payload
            .claims
            .into_iter()
            .filter(|c| !c.statement.trim().is_empty())
            .take(max_claims)
            .filter_map(|c| {
                let doc = docs.get(c.source_index).or_else(|| docs.first())?;
                let polarity = match c.polarity.trim().to_ascii_lowercase().as_str() {
                    "bull" => Polarity::Bull,
                    "bear" => Polarity::Bear,
                    _ => Polarity::NeutralMetric,
                };
                Some(AtomicClaim {
                    id: Uuid::new_v4(),
                    source_url: doc.url.clone(),
                    dom_sha256: doc.dom_sha256.clone(),
                    subject: if c.subject.trim().is_empty() {
                        doc.branch_query.clone()
                    } else {
                        c.subject
                    },
                    statement: c.statement.trim().to_string(),
                    polarity,
                    confidence: c.confidence.clamp(0.1, 1.0),
                    cluster_id: None,
                    is_selected: true,
                })
            })
            .collect();

        if parsed.is_empty() {
            Self::extract_algorithmic(docs, max_claims)
        } else {
            parsed
        }
    }
}

fn select_mmr_candidate(
    candidates: &[CandidateSentence],
    used: &[bool],
    selected: &[usize],
    polarity_filter: Option<Polarity>,
    lambda: f32,
) -> Option<usize> {
    let mut best_idx = None;
    let mut best_mmr = f32::NEG_INFINITY;

    for (i, cand) in candidates.iter().enumerate() {
        if used[i] {
            continue;
        }
        if let Some(target_pol) = polarity_filter {
            if cand.polarity != target_pol {
                continue;
            }
        }

        let max_sim = selected
            .iter()
            .map(|&s_idx| cosine_similarity(&cand.tokens, &candidates[s_idx].tokens))
            .fold(0.0_f32, f32::max);

        let mmr = lambda * cand.score - (1.0 - lambda) * max_sim;
        if mmr > best_mmr {
            best_mmr = mmr;
            best_idx = Some(i);
        }
    }

    best_idx
}

async fn resolve_ollama_model(client: &reqwest::Client, base_url: &str) -> Option<String> {
    if let Ok(explicit) = std::env::var("DDR_OLLAMA_MODEL") {
        if !explicit.trim().is_empty() {
            return Some(explicit);
        }
    }

    let resp = client
        .get(format!("{base_url}/api/tags"))
        .timeout(Duration::from_millis(1200))
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }

    let tags = resp.json::<OllamaTagsResponse>().await.ok()?;
    if let Some(preferred) = tags
        .models
        .iter()
        .find(|m| m.name.contains("qwen2.5") || m.name.contains("qwen3"))
    {
        return Some(preferred.name.clone());
    }
    tags.models.first().map(|m| m.name.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ddr_crawler::metrics::compute_minhash;
    use ddr_protocol::FacetSlot;

    #[test]
    fn extracts_balanced_bull_bear_and_metric_claims() {
        let doc = ScrapedDocument {
            doc_id: Uuid::new_v4(),
            node_id: Uuid::new_v4(),
            url: "https://example.edu/solid-state".into(),
            root_domain: "example.edu".into(),
            branch_query: "solid state batteries".into(),
            facet_slot: FacetSlot::Quantitative,
            hop_depth: 0,
            bm25_relevance: 0.85,
            minhash_sig: compute_minhash("solid state batteries", 64),
            dom_sha256: "c".repeat(64),
            clean_text: "Solid state batteries achieved 450 Wh/kg energy density in laboratory benchmarks across 900 cycles. \
                         Sulfide electrolytes deliver superior safety and faster charging compared to conventional liquid cells. \
                         However, lithium dendrite growth and interfacial cracking remain critical manufacturing bottlenecks and cost risks. \
                         Cell manufacturing requires 120 MPa stack pressure at $145 per kWh under current pilot lines."
                .into(),
        };

        let claims = ClaimExtractor::extract_algorithmic(&[doc], 6);
        assert!(claims.len() >= 3);
        assert!(claims.iter().any(|c| c.polarity == Polarity::Bull));
        assert!(claims.iter().any(|c| c.polarity == Polarity::Bear));
        assert!(claims.iter().all(|c| c.dom_sha256 == "c".repeat(64)));
    }
}
