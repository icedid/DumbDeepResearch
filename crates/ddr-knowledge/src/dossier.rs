use crate::graph::KnowledgeGraph;
use ddr_protocol::{
    AuditDossier, CitationAuditResult, DossierLineageEntry, NodeStatus, Polarity,
};
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

/// Generates a structured, audit-ready dossier across the Knowledge Graph and runs
/// the deterministic `[CLAIM-ID]` citation verification pass.
pub fn generate_audit_dossier(graph: &KnowledgeGraph) -> AuditDossier {
    let nodes = graph.all_nodes();
    let root_topic = nodes
        .iter()
        .find(|n| n.depth == 0)
        .map(|n| n.topic.clone())
        .unwrap_or_else(|| "Untitled Research Session".to_string());

    let mut completed_nodes = 0usize;
    let mut unique_sources = HashSet::new();
    let mut bull_count = 0usize;
    let mut bear_count = 0usize;
    let mut neutral_count = 0usize;

    let mut lineage = Vec::new();
    let mut claim_index: HashMap<String, DossierLineageEntry> = HashMap::new();

    for node in &nodes {
        if node.status == NodeStatus::Pruned {
            continue;
        }
        if node.status == NodeStatus::Complete {
            completed_nodes += 1;
        }
        for src in &node.sources {
            unique_sources.insert(src.url.clone());
        }

        let hop_lookup: HashMap<&str, u8> = node
            .sources
            .iter()
            .map(|s| (s.url.as_str(), s.hop_depth))
            .collect();

        for claim in &node.findings {
            match claim.polarity {
                Polarity::Bull => bull_count += 1,
                Polarity::Bear => bear_count += 1,
                Polarity::NeutralMetric => neutral_count += 1,
            }
            let short_id = claim.id.simple().to_string()[..8].to_uppercase();
            let entry = DossierLineageEntry {
                claim_id: claim.id,
                short_id: short_id.clone(),
                node_id: node.id,
                node_topic: node.topic.clone(),
                query: node.query.clone(),
                polarity: claim.polarity,
                statement: claim.statement.clone(),
                source_url: claim.source_url.clone(),
                dom_sha256: claim.dom_sha256.clone(),
                hop_depth: *hop_lookup.get(claim.source_url.as_str()).unwrap_or(&0),
            };
            claim_index.insert(short_id, entry.clone());
            lineage.push(entry);
        }
    }

    let mut md = String::new();
    md.push_str(&format!("# DumbDeepResearch Audit Dossier: {root_topic}\n\n"));
    md.push_str("## 1. Executive Telemetry & Balance Summary\n\n");
    md.push_str(&format!(
        "- **Explored Knowledge Nodes:** {completed_nodes} (Total DAG nodes: {})\n",
        nodes.len()
    ));
    md.push_str(&format!(
        "- **Verified Unique Sources:** {}\n",
        unique_sources.len()
    ));
    md.push_str(&format!(
        "- **Equalized Atomic Claims:** {} (`{bull_count}` Bull / `{bear_count}` Bear / `{neutral_count}` Neutral Metrics)\n\n",
        lineage.len()
    ));

    md.push_str("## 2. Perspective-Balanced Findings by Knowledge Node\n\n");

    for node in nodes.iter().filter(|n| n.status != NodeStatus::Pruned) {
        if node.findings.is_empty() && node.sources.is_empty() {
            continue;
        }
        md.push_str(&format!(
            "### [Depth {} | {:?}] {}\n",
            node.depth, node.slot, node.topic
        ));
        md.push_str(&format!("- **Search Query:** `{}`\n", node.query));
        if let Some(reason) = &node.probe_reason {
            md.push_str(&format!("- **Probe Circuit Breaker:** `{reason}`\n"));
        }
        if let Some(asym) = &node.asymmetry_flag {
            md.push_str(&format!("- **Perspective Audit Notice:** {asym}\n"));
        }
        md.push('\n');

        let emit_section = |out: &mut String, title: &str, pol: Polarity| {
            let matching: Vec<_> = node
                .findings
                .iter()
                .filter(|c| c.polarity == pol)
                .collect();
            if matching.is_empty() {
                return;
            }
            out.push_str(&format!("**{title}**\n"));
            for c in matching {
                let short_id = c.id.simple().to_string()[..8].to_uppercase();
                out.push_str(&format!("- {} `[CLAIM-{short_id}]`\n", c.statement));
            }
            out.push('\n');
        };

        emit_section(&mut md, "Affirmative Thesis (Bull)", Polarity::Bull);
        emit_section(&mut md, "Counter-Thesis & Risks (Bear)", Polarity::Bear);
        emit_section(
            &mut md,
            "Empirical Benchmarks & Metrics",
            Polarity::NeutralMetric,
        );
    }

    md.push_str("## 3. Cryptographic Audit Lineage Table\n\n");
    md.push_str("| Claim Tag | Polarity | Node | Query | Source URL | Hop | DOM SHA-256 |\n");
    md.push_str("| --- | --- | --- | --- | --- | --- | --- |\n");
    for row in &lineage {
        let sha_short = if row.dom_sha256.len() >= 12 {
            &row.dom_sha256[..12]
        } else {
            &row.dom_sha256
        };
        md.push_str(&format!(
            "| `CLAIM-{}` | {:?} | {} | `{}` | {} | H{} | `{}` |\n",
            row.short_id,
            row.polarity,
            row.node_topic.replace('|', "/"),
            row.query.replace('|', "/"),
            row.source_url,
            row.hop_depth,
            sha_short
        ));
    }

    let citation_audit = verify_citations(&md, &claim_index);

    AuditDossier {
        root_topic,
        node_count: completed_nodes,
        source_count: unique_sources.len(),
        claim_count: lineage.len(),
        bull_count,
        bear_count,
        neutral_count,
        markdown: md,
        lineage,
        citation_audit,
    }
}

/// Deterministically scans synthesized markdown for `[CLAIM-XXXXXXXX]` tags and ensures every tag
/// maps to a real extracted claim with an authenticated URL and valid 64-hex-char SHA-256 digest.
pub fn verify_citations(
    markdown: &str,
    claim_index: &HashMap<String, DossierLineageEntry>,
) -> CitationAuditResult {
    static CITE_RE: OnceLock<Regex> = OnceLock::new();
    let re = CITE_RE.get_or_init(|| Regex::new(r"\[CLAIM-([0-9A-Za-z]{8})\]").expect("valid regex"));

    let mut total_citations = 0usize;
    let mut verified_citations = 0usize;
    let mut unverified_citations = Vec::new();

    for cap in re.captures_iter(markdown) {
        total_citations += 1;
        let tag = cap[1].to_ascii_uppercase();
        if let Some(entry) = claim_index.get(&tag) {
            let valid_sha = entry.dom_sha256.len() == 64
                && entry.dom_sha256.chars().all(|c| c.is_ascii_hexdigit());
            if !entry.source_url.is_empty() && valid_sha {
                verified_citations += 1;
            } else {
                unverified_citations.push(format!("CLAIM-{tag} (missing provenance)"));
            }
        } else {
            unverified_citations.push(format!("CLAIM-{tag} (unknown ID)"));
        }
    }

    let passed = unverified_citations.is_empty();
    CitationAuditResult {
        total_citations,
        verified_citations,
        unverified_citations,
        passed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ddr_protocol::{AtomicClaim, FacetSlot, KnowledgeNode, SourceMetadata};
    use uuid::Uuid;

    #[test]
    fn generates_and_verifies_cryptographic_lineage() {
        let mut graph = KnowledgeGraph::new();
        let sha = "a".repeat(64);
        let claim_id = Uuid::new_v4();

        let node = KnowledgeNode {
            id: Uuid::new_v4(),
            parent_id: None,
            depth: 0,
            topic: "Solid State Batteries".into(),
            query: "solid state batteries".into(),
            slot: FacetSlot::Exploratory,
            status: NodeStatus::Complete,
            prompt_override: None,
            probe_reason: Some("CompletedQuotaMet".into()),
            asymmetry_flag: None,
            findings: vec![AtomicClaim {
                id: claim_id,
                source_url: "https://example.edu/paper".into(),
                dom_sha256: sha.clone(),
                subject: "Battery".into(),
                statement: "Achieves 450 Wh/kg in laboratory cells.".into(),
                polarity: Polarity::NeutralMetric,
                confidence: 0.9,
                cluster_id: Some(0),
                is_selected: true,
            }],
            suggested_branches: vec![],
            sources: vec![SourceMetadata {
                url: "https://example.edu/paper".into(),
                domain: "example.edu".into(),
                title: "Lab Paper".into(),
                snippet: "Achieves 450 Wh/kg".into(),
                dom_sha256: sha,
                hop_depth: 0,
            }],
        };
        graph.insert_node(node);

        let dossier = generate_audit_dossier(&graph);
        assert!(dossier.citation_audit.passed);
        assert_eq!(dossier.citation_audit.total_citations, 1);
        assert_eq!(dossier.citation_audit.verified_citations, 1);
        assert!(dossier.markdown.contains("Cryptographic Audit Lineage Table"));
    }
}
