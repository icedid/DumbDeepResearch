use ddr_crawler::ScrapedDocument;
use ddr_protocol::KnowledgeNode;
use std::path::PathBuf;
use tracing::warn;

/// Disk persistence layer for raw sanitized DOM dumps (`data/raw/{node_id}.txt`)
/// and per-node structured dossiers (`data/nodes/{node_id}.json`).
#[derive(Debug, Clone)]
pub struct ArtifactStore {
    base_dir: Option<PathBuf>,
}

impl ArtifactStore {
    pub fn new(base_dir: Option<PathBuf>) -> Self {
        Self { base_dir }
    }

    pub async fn write_raw_dump(&self, node: &KnowledgeNode, docs: &[ScrapedDocument]) {
        let Some(base) = &self.base_dir else {
            return;
        };
        let raw_dir = base.join("raw");
        if let Err(err) = tokio::fs::create_dir_all(&raw_dir).await {
            warn!(%err, dir = %raw_dir.display(), "Failed to create raw dump directory");
            return;
        }

        let mut content = String::new();
        content.push_str(&format!(
            "# Node: {} ({})\n# Query: {}\n# Slot: {:?}\n# Depth: {}\n\n",
            node.topic, node.id, node.query, node.slot, node.depth
        ));

        for (idx, doc) in docs.iter().enumerate() {
            content.push_str(&format!(
                "=== SOURCE {} ===\nURL: {}\nDomain: {}\nHop: {}\nBM25: {:.3}\nDOM_SHA256: {}\n\n{}\n\n",
                idx + 1,
                doc.url,
                doc.root_domain,
                doc.hop_depth,
                doc.bm25_relevance,
                doc.dom_sha256,
                doc.clean_text
            ));
        }

        let path = raw_dir.join(format!("{}.txt", node.id));
        if let Err(err) = tokio::fs::write(&path, content).await {
            warn!(%err, path = %path.display(), "Failed to write raw dump file");
        }
    }

    pub async fn write_node_summary(&self, node: &KnowledgeNode) {
        let Some(base) = &self.base_dir else {
            return;
        };
        let nodes_dir = base.join("nodes");
        if let Err(err) = tokio::fs::create_dir_all(&nodes_dir).await {
            warn!(%err, dir = %nodes_dir.display(), "Failed to create node summary directory");
            return;
        }

        let path = nodes_dir.join(format!("{}.json", node.id));
        match serde_json::to_string_pretty(node) {
            Ok(json) => {
                if let Err(err) = tokio::fs::write(&path, json).await {
                    warn!(%err, path = %path.display(), "Failed to write node summary JSON");
                }
            }
            Err(err) => {
                warn!(%err, node_id = %node.id, "Failed to serialize node summary JSON");
            }
        }
    }
}
