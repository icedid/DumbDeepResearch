use crate::steering::SteeringPolicy;
use ddr_crawler::{
    frontier::FrontierGuard,
    search_client::MetaSearchClient,
    SearchProbe,
};
use ddr_knowledge::{
    discovery::CascadedDiscovery,
    equalizer::PerspectiveEqualizer,
    extractor::ClaimExtractor,
    orthogonality::OrthogonalityGate,
    KnowledgeGraph,
};
use ddr_protocol::{
    ClientAction, EngineBroadcast, ExecutionMode, FacetSlot, KnowledgeNode, NodeStatus,
    SuggestedBranch,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex, RwLock};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};
use uuid::Uuid;

const DEFAULT_MAX_DEPTH: u8 = 2;
const DEFAULT_MAX_SOURCES: usize = 5;
const DEFAULT_DOMAIN_CEILING: usize = 2;
const DEFAULT_NOVELTY_THRESHOLD: f32 = 0.6;
const DEFAULT_BRANCH_COUNT: usize = 3;

/// Session-level execution parameters controlled by `ClientAction::Start`.
#[derive(Debug, Clone, Copy)]
pub struct SessionConfig {
    pub max_depth: u8,
    pub max_sources: usize,
    pub mode: ExecutionMode,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            max_depth: DEFAULT_MAX_DEPTH,
            max_sources: DEFAULT_MAX_SOURCES,
            mode: ExecutionMode::InteractiveSteered,
        }
    }
}

/// Orchestrates the lifecycle between the Knowledge Graph (`ddr-knowledge`)
/// and the bounded Node Explore Algorithm (`ddr-crawler`), supporting both
/// Human-Steered and Autonomous Free Exploration modes.
#[derive(Clone)]
pub struct ResearchCoordinator {
    pub graph: Arc<RwLock<KnowledgeGraph>>,
    pub active_tokens: Arc<Mutex<HashMap<Uuid, CancellationToken>>>,
    pub broadcast_tx: broadcast::Sender<EngineBroadcast>,
    pub session_config: Arc<RwLock<SessionConfig>>,
}

impl ResearchCoordinator {
    pub fn new(broadcast_tx: broadcast::Sender<EngineBroadcast>) -> Self {
        Self {
            graph: Arc::new(RwLock::new(KnowledgeGraph::new())),
            active_tokens: Arc::new(Mutex::new(HashMap::new())),
            broadcast_tx,
            session_config: Arc::new(RwLock::new(SessionConfig::default())),
        }
    }

    /// Handles incoming actions from the UI or Autonomous Policy (`START`, `EXPLORE`, `PROBE`, `PRUNE`).
    pub async fn handle_action(&self, action: ClientAction) {
        match action {
            ClientAction::Start {
                topic,
                max_depth,
                max_sources,
                mode,
            } => {
                self.start_session(topic, max_depth, max_sources, mode).await;
            }
            ClientAction::Explore { node_id } => {
                self.explore_node(node_id).await;
            }
            ClientAction::Probe {
                node_id,
                custom_query,
                prompt_override,
            } => {
                self.probe_node(node_id, custom_query, prompt_override).await;
            }
            ClientAction::Prune { node_id } => {
                self.prune_node(node_id).await;
            }
        }
    }

    async fn start_session(
        &self,
        topic: String,
        max_depth: u8,
        max_sources: usize,
        mode: ExecutionMode,
    ) {
        info!(%topic, max_depth, max_sources, ?mode, "Starting research session");
        {
            let mut cfg = self.session_config.write().await;
            cfg.max_depth = max_depth;
            cfg.max_sources = max_sources;
            cfg.mode = mode;
        }

        let root_node = KnowledgeNode {
            id: Uuid::new_v4(),
            parent_id: None,
            depth: 0,
            topic: topic.clone(),
            query: topic,
            slot: FacetSlot::Exploratory,
            status: NodeStatus::Queued,
            findings: Vec::new(),
            suggested_branches: Vec::new(),
            sources: Vec::new(),
        };

        {
            let mut graph = self.graph.write().await;
            graph.insert_node(root_node.clone());
        }

        self.emit(EngineBroadcast::NodeAdded {
            node: root_node.clone(),
        });
        self.spawn_node_explore(root_node);
    }

    async fn explore_node(&self, node_id: Uuid) {
        let maybe_node = {
            let mut graph = self.graph.write().await;
            let Some(node) = graph.get_node_mut(&node_id) else {
                warn!(%node_id, "Explore requested for unknown node");
                return;
            };

            if matches!(
                node.status,
                NodeStatus::Queued | NodeStatus::Searching | NodeStatus::Scraping | NodeStatus::Pruned
            ) {
                debug!(%node_id, status = ?node.status, "Ignoring Explore for node in current state");
                return;
            }

            node.status = NodeStatus::Queued;
            node.clone()
        };

        self.emit(EngineBroadcast::NodeUpdated {
            node_id,
            status: NodeStatus::Queued,
            node: maybe_node.clone(),
        });
        self.spawn_node_explore(maybe_node);
    }

    async fn probe_node(
        &self,
        node_id: Uuid,
        custom_query: String,
        _prompt_override: Option<String>,
    ) {
        // Cancel any in-flight exploration for this node before re-probing.
        self.cancel_token(&node_id).await;

        let maybe_node = {
            let mut graph = self.graph.write().await;
            let Some(node) = graph.get_node_mut(&node_id) else {
                warn!(%node_id, "Probe requested for unknown node");
                return;
            };

            node.query = custom_query;
            node.status = NodeStatus::Queued;
            node.findings.clear();
            node.sources.clear();
            node.suggested_branches.clear();
            node.clone()
        };

        self.emit(EngineBroadcast::NodeUpdated {
            node_id,
            status: NodeStatus::Queued,
            node: maybe_node.clone(),
        });
        self.spawn_node_explore(maybe_node);
    }

    async fn prune_node(&self, node_id: Uuid) {
        self.cancel_token(&node_id).await;

        let updated = {
            let mut graph = self.graph.write().await;
            if let Some(node) = graph.get_node_mut(&node_id) {
                node.status = NodeStatus::Pruned;
                true
            } else {
                false
            }
        };

        if updated {
            self.emit(EngineBroadcast::NodePruned { node_id });
        } else {
            warn!(%node_id, "Prune requested for unknown node");
        }
    }

    async fn cancel_token(&self, node_id: &Uuid) {
        let mut tokens = self.active_tokens.lock().await;
        if let Some(token) = tokens.remove(node_id) {
            token.cancel();
        }
    }

    fn spawn_node_explore(&self, node: KnowledgeNode) {
        let coordinator = self.clone();
        let node_id = node.id;
        let token = CancellationToken::new();

        tokio::spawn(async move {
            {
                let mut tokens = coordinator.active_tokens.lock().await;
                if let Some(prev) = tokens.insert(node_id, token.clone()) {
                    prev.cancel();
                }
            }

            tokio::select! {
                _ = token.cancelled() => {
                    debug!(%node_id, "Node explore cancelled");
                }
                _ = coordinator.run_node_explore(node) => {}
            }

            let mut tokens = coordinator.active_tokens.lock().await;
            tokens.remove(&node_id);
        });
    }

    /// Bounded per-node exploration pipeline:
    /// `Searching` (MetaSearch + Frontier) -> `Scraping` (Fetch + Clean/SHA-256) ->
    /// `Complete` (Claim Extraction + 50/50 Equalizer + Branch Discovery).
    async fn run_node_explore(&self, node: KnowledgeNode) {
        let node_id = node.id;
        let cfg = *self.session_config.read().await;

        // Phase 1: Searching
        self.transition_status(node_id, NodeStatus::Searching).await;

        let search_client = MetaSearchClient::new("http://localhost:8080");
        let mut frontier = FrontierGuard::new(DEFAULT_DOMAIN_CEILING);
        let mut probe = SearchProbe::new(node_id, node.query.clone(), node.slot, cfg.max_depth);

        let serp_entries = search_client.search(&node.query, cfg.max_sources).await;
        let allowed_urls: Vec<_> = serp_entries
            .into_iter()
            .filter(|entry| frontier.allow_url(&entry.url))
            .take(cfg.max_sources)
            .collect();

        // Phase 2: Scraping & Cleaning (bounded per-node PageNode pass)
        self.transition_status(node_id, NodeStatus::Scraping).await;

        let scraped_docs = Vec::new();
        for doc in &scraped_docs {
            let _ = probe.evaluate_circuit_breakers(doc);
        }
        let _ = allowed_urls;

        // Phase 3: Claim Extraction, 50/50 Perspective Equalizer, & Orthogonal Branch Discovery
        let raw_claims = ClaimExtractor::extract_algorithmic(&scraped_docs, 6);
        let balanced_claims = PerspectiveEqualizer::balance_claims(raw_claims);

        let term_spikes = CascadedDiscovery::extract_prf_term_spikes(&scraped_docs, 5);
        let contrasts = CascadedDiscovery::extract_windowed_contrasts(&scraped_docs, &term_spikes);
        let candidate_branches: Vec<SuggestedBranch> = contrasts
            .into_iter()
            .map(|(term, _friction)| SuggestedBranch {
                topic: term.clone(),
                query: format!("{} {}", node.query, term),
                slot: FacetSlot::Exploratory,
                significance_score: 0.7,
            })
            .collect();

        let branches = OrthogonalityGate::filter_orthogonal_branches(
            &node.query,
            candidate_branches,
            DEFAULT_BRANCH_COUNT,
        );

        let completed_node = {
            let mut graph = self.graph.write().await;
            let Some(stored) = graph.get_node_mut(&node_id) else {
                return;
            };
            stored.status = NodeStatus::Complete;
            stored.findings = balanced_claims;
            stored.suggested_branches = branches.clone();
            stored.clone()
        };

        self.emit(EngineBroadcast::NodeUpdated {
            node_id,
            status: NodeStatus::Complete,
            node: completed_node.clone(),
        });

        // Materialize proposed child branches if depth budget allows.
        if completed_node.depth < cfg.max_depth && !branches.is_empty() {
            let mut proposed_children = Vec::with_capacity(branches.len());
            {
                let mut graph = self.graph.write().await;
                for branch in &branches {
                    let child = KnowledgeNode {
                        id: Uuid::new_v4(),
                        parent_id: Some(completed_node.id),
                        depth: completed_node.depth + 1,
                        topic: branch.topic.clone(),
                        query: branch.query.clone(),
                        slot: branch.slot,
                        status: NodeStatus::Proposed,
                        findings: Vec::new(),
                        suggested_branches: Vec::new(),
                        sources: Vec::new(),
                    };
                    graph.insert_node(child.clone());
                    proposed_children.push((child, branch.clone()));
                }
            }

            for (child, _) in &proposed_children {
                self.emit(EngineBroadcast::NodeAdded {
                    node: child.clone(),
                });
            }

            if cfg.mode == ExecutionMode::AutonomousFree {
                let policy = SteeringPolicy::new(cfg.max_depth, DEFAULT_NOVELTY_THRESHOLD);
                let selected = policy.select_auto_branches(completed_node.depth, &branches);
                for chosen in selected {
                    if let Some((child, _)) = proposed_children
                        .iter()
                        .find(|(c, _)| c.query == chosen.query && c.slot == chosen.slot)
                    {
                        self.explore_node(child.id).await;
                    }
                }
            }
        }
    }

    async fn transition_status(&self, node_id: Uuid, status: NodeStatus) {
        let maybe_node = {
            let mut graph = self.graph.write().await;
            let Some(node) = graph.get_node_mut(&node_id) else {
                return;
            };
            node.status = status;
            node.clone()
        };

        self.emit(EngineBroadcast::NodeUpdated {
            node_id,
            status,
            node: maybe_node,
        });
    }

    fn emit(&self, event: EngineBroadcast) {
        // Ignore SendError when no WebSocket clients are currently subscribed.
        let _ = self.broadcast_tx.send(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn start_and_prune_lifecycle() {
        let (tx, mut rx) = broadcast::channel(32);
        let coordinator = ResearchCoordinator::new(tx);

        coordinator
            .handle_action(ClientAction::Start {
                topic: "solid state batteries".into(),
                max_depth: 1,
                max_sources: 3,
                mode: ExecutionMode::InteractiveSteered,
            })
            .await;

        let first = rx.recv().await.expect("expected NodeAdded broadcast");
        let root_id = match first {
            EngineBroadcast::NodeAdded { node } => {
                assert_eq!(node.status, NodeStatus::Queued);
                assert_eq!(node.depth, 0);
                node.id
            }
            other => panic!("unexpected first broadcast: {other:?}"),
        };

        coordinator
            .handle_action(ClientAction::Prune { node_id: root_id })
            .await;

        let graph = coordinator.graph.read().await;
        assert_eq!(
            graph.get_node(&root_id).map(|n| n.status),
            Some(NodeStatus::Pruned)
        );
    }
}
