use crate::steering::SteeringPolicy;
use crate::storage::ArtifactStore;
use ddr_crawler::{
    cleaner::DomCleaner,
    frontier::FrontierGuard,
    metrics::{compute_bm25_relevance, compute_minhash, word_count},
    search_client::MetaSearchClient,
    HttpFetcher, PageNode, ProbeStatus, ScrapedDocument, SearchProbe,
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
    SourceMetadata,
};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex, RwLock, Semaphore};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};
use uuid::Uuid;

const DEFAULT_MAX_DEPTH: u8 = 2;
const DEFAULT_MAX_SOURCES: usize = 5;
const DEFAULT_DOMAIN_CEILING: usize = 2;
const DEFAULT_NOVELTY_THRESHOLD: f32 = 0.6;
const DEFAULT_BRANCH_COUNT: usize = 3;
const MAX_CRAWL_HOPS: u8 = 2;
const MAX_CITATIONS_PER_PAGE: usize = 2;
const MIN_FALLBACK_WORDS: usize = 30;

/// Runtime configuration for `ResearchCoordinator`.
#[derive(Debug, Clone)]
pub struct CoordinatorConfig {
    pub search_endpoint: String,
    pub data_dir: Option<PathBuf>,
    pub use_ollama: bool,
    pub max_concurrent_fetches: usize,
    pub min_quality_words: usize,
}

impl Default for CoordinatorConfig {
    fn default() -> Self {
        let search_endpoint = std::env::var("DDR_SEARCH_ENDPOINT")
            .unwrap_or_else(|_| "http://127.0.0.1:8080".to_string());
        let data_dir = match std::env::var("DDR_DATA_DIR") {
            Ok(v) if v.trim().is_empty() || v == "none" => None,
            Ok(v) => Some(PathBuf::from(v)),
            Err(_) => Some(PathBuf::from("data")),
        };
        let use_ollama = matches!(
            std::env::var("DDR_USE_OLLAMA").as_deref(),
            Ok("1") | Ok("true") | Ok("TRUE")
        );
        let min_quality_words = std::env::var("DDR_MIN_QUALITY_WORDS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(250);

        Self {
            search_endpoint,
            data_dir,
            use_ollama,
            max_concurrent_fetches: 6,
            min_quality_words,
        }
    }
}

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
    pub config: Arc<CoordinatorConfig>,
    fetch_semaphore: Arc<Semaphore>,
    fetcher: HttpFetcher,
    storage: ArtifactStore,
}

impl ResearchCoordinator {
    pub fn new(broadcast_tx: broadcast::Sender<EngineBroadcast>) -> Self {
        Self::with_config(broadcast_tx, CoordinatorConfig::default())
    }

    pub fn with_config(
        broadcast_tx: broadcast::Sender<EngineBroadcast>,
        config: CoordinatorConfig,
    ) -> Self {
        let storage = ArtifactStore::new(config.data_dir.clone());
        let fetch_semaphore = Arc::new(Semaphore::new(config.max_concurrent_fetches.max(1)));
        Self {
            graph: Arc::new(RwLock::new(KnowledgeGraph::new())),
            active_tokens: Arc::new(Mutex::new(HashMap::new())),
            broadcast_tx,
            session_config: Arc::new(RwLock::new(SessionConfig::default())),
            config: Arc::new(config),
            fetch_semaphore,
            fetcher: HttpFetcher::new(),
            storage,
        }
    }

    /// Handles incoming actions from the UI or Autonomous Policy (`START`, `EXPLORE`, `PROBE`, `PRUNE`, `RESET`).
    pub async fn handle_action(&self, action: ClientAction) {
        match action {
            ClientAction::Start {
                topic,
                max_depth,
                max_sources,
                mode,
            } => {
                self.start_session(topic, max_depth, max_sources, mode)
                    .await;
            }
            ClientAction::Explore { node_id } => {
                self.explore_node(node_id).await;
            }
            ClientAction::Probe {
                node_id,
                custom_query,
                prompt_override,
            } => {
                self.probe_node(node_id, custom_query, prompt_override)
                    .await;
            }
            ClientAction::Prune { node_id } => {
                self.prune_node(node_id).await;
            }
            ClientAction::Reset => {
                self.reset_session().await;
            }
        }
    }

    async fn reset_session(&self) {
        self.cancel_all_tokens().await;
        {
            let mut graph = self.graph.write().await;
            graph.clear();
        }
        let cfg = *self.session_config.read().await;
        self.emit(EngineBroadcast::GraphSnapshot {
            nodes: Vec::new(),
            max_depth: cfg.max_depth,
            max_sources: cfg.max_sources,
            mode: cfg.mode,
        });
    }

    async fn start_session(
        &self,
        topic: String,
        max_depth: u8,
        max_sources: usize,
        mode: ExecutionMode,
    ) {
        let trimmed_topic = topic.trim().to_string();
        if trimmed_topic.is_empty() {
            warn!("Ignoring Start action with empty topic");
            return;
        }

        info!(topic = %trimmed_topic, max_depth, max_sources, ?mode, "Starting research session");

        // Cancel any previous session tasks and reset graph if non-empty.
        self.cancel_all_tokens().await;
        let had_existing_nodes = {
            let mut graph = self.graph.write().await;
            let non_empty = !graph.index_map.is_empty();
            if non_empty {
                graph.clear();
            }
            non_empty
        };

        {
            let mut cfg = self.session_config.write().await;
            cfg.max_depth = max_depth.clamp(1, 5);
            cfg.max_sources = max_sources.clamp(1, 15);
            cfg.mode = mode;
        }

        if had_existing_nodes {
            let cfg = *self.session_config.read().await;
            self.emit(EngineBroadcast::GraphSnapshot {
                nodes: Vec::new(),
                max_depth: cfg.max_depth,
                max_sources: cfg.max_sources,
                mode: cfg.mode,
            });
        }

        let root_node = KnowledgeNode {
            id: Uuid::new_v4(),
            parent_id: None,
            depth: 0,
            topic: trimmed_topic.clone(),
            query: trimmed_topic,
            slot: FacetSlot::Exploratory,
            status: NodeStatus::Queued,
            prompt_override: None,
            probe_reason: None,
            asymmetry_flag: None,
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
            node.probe_reason = None;
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
        prompt_override: Option<String>,
    ) {
        let trimmed_query = custom_query.trim().to_string();
        if trimmed_query.is_empty() {
            warn!(%node_id, "Ignoring Probe with empty custom_query");
            return;
        }

        // Cancel any in-flight exploration for this node before re-probing.
        self.cancel_token(&node_id).await;

        let (maybe_node, pruned_children) = {
            let mut graph = self.graph.write().await;
            let descendants = graph.descendants(&node_id);
            let mut pruned_ids = Vec::new();
            for desc_id in descendants {
                if let Some(child) = graph.get_node_mut(&desc_id) {
                    if child.status == NodeStatus::Proposed {
                        child.status = NodeStatus::Pruned;
                        pruned_ids.push(desc_id);
                    }
                }
            }

            let Some(node) = graph.get_node_mut(&node_id) else {
                warn!(%node_id, "Probe requested for unknown node");
                return;
            };

            node.query = trimmed_query;
            node.prompt_override = prompt_override.filter(|s| !s.trim().is_empty());
            node.status = NodeStatus::Queued;
            node.probe_reason = None;
            node.asymmetry_flag = None;
            node.findings.clear();
            node.sources.clear();
            node.suggested_branches.clear();
            (node.clone(), pruned_ids)
        };

        for child_id in pruned_children {
            self.emit(EngineBroadcast::NodePruned { node_id: child_id });
        }

        self.emit(EngineBroadcast::NodeUpdated {
            node_id,
            status: NodeStatus::Queued,
            node: maybe_node.clone(),
        });
        self.spawn_node_explore(maybe_node);
    }

    async fn prune_node(&self, node_id: Uuid) {
        let target_ids = {
            let graph = self.graph.read().await;
            if graph.get_node(&node_id).is_none() {
                warn!(%node_id, "Prune requested for unknown node");
                return;
            }
            let mut ids = vec![node_id];
            ids.extend(graph.descendants(&node_id));
            ids
        };

        for id in &target_ids {
            self.cancel_token(id).await;
        }

        let mut pruned_now = Vec::new();
        {
            let mut graph = self.graph.write().await;
            for id in target_ids {
                if let Some(node) = graph.get_node_mut(&id) {
                    if node.status != NodeStatus::Pruned {
                        node.status = NodeStatus::Pruned;
                        pruned_now.push(id);
                    }
                }
            }
        }

        for id in pruned_now {
            self.emit(EngineBroadcast::NodePruned { node_id: id });
        }
    }

    async fn cancel_token(&self, node_id: &Uuid) {
        let mut tokens = self.active_tokens.lock().await;
        if let Some(token) = tokens.remove(node_id) {
            token.cancel();
        }
    }

    async fn cancel_all_tokens(&self) {
        let mut tokens = self.active_tokens.lock().await;
        for (_, token) in tokens.drain() {
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
                _ = coordinator.run_node_explore(node, token.clone()) => {}
            }

            let mut tokens = coordinator.active_tokens.lock().await;
            tokens.remove(&node_id);
        });
    }

    /// Bounded per-node exploration pipeline:
    /// `Searching` (Query Matrix + MetaSearch + Frontier) ->
    /// `Scraping` (Bounded Fetch + `lol_html` + SHA-256 + Depth-1 Citation Hop + 5 Circuit Breakers) ->
    /// `Complete` (Claim Extraction + 50/50 Equalizer + Cascaded PRF + 4-Stage Orthogonality Gate).
    async fn run_node_explore(&self, node: KnowledgeNode, cancel_token: CancellationToken) {
        let node_id = node.id;
        let cfg = *self.session_config.read().await;

        // Phase 1: Searching
        if !self.transition_status(node_id, NodeStatus::Searching).await {
            return;
        }

        let search_client = MetaSearchClient::new(&self.config.search_endpoint);
        let mut frontier = FrontierGuard::new(DEFAULT_DOMAIN_CEILING);
        let mut probe = SearchProbe::new(node_id, node.query.clone(), node.slot, MAX_CRAWL_HOPS);

        let query_matrix = MetaSearchClient::build_query_matrix(
            &node.query,
            node.slot,
            node.prompt_override.as_deref(),
        );

        let macro_facets = if node.depth == 0 {
            search_client.fetch_facets(&node.query).await
        } else {
            Vec::new()
        };

        let mut page_queue: VecDeque<PageNode> = VecDeque::new();
        let mut seen_serp_urls = HashSet::new();
        let per_query_limit = cfg.max_sources.max(3);

        for q in &query_matrix {
            if cancel_token.is_cancelled() {
                return;
            }
            let entries = search_client.search(q, per_query_limit).await;
            for entry in entries {
                if seen_serp_urls.insert(entry.url.clone()) && frontier.allow_url(&entry.url) {
                    let domain = FrontierGuard::extract_root_domain(&entry.url);
                    page_queue.push_back(PageNode::new(
                        entry.url,
                        domain,
                        entry.title,
                        entry.snippet,
                        0,
                        None,
                    ));
                }
            }
        }

        // Phase 2: Scraping & Cleaning (bounded per-node PageNode pass)
        if !self.transition_status(node_id, NodeStatus::Scraping).await {
            return;
        }

        let mut accepted_docs: Vec<ScrapedDocument> = Vec::new();
        let mut subthreshold_docs: Vec<(ScrapedDocument, SourceMetadata)> = Vec::new();
        let mut collected_sources: Vec<SourceMetadata> = Vec::new();
        let mut terminal_reason: Option<String> = None;

        while let Some(mut page) = page_queue.pop_front() {
            if cancel_token.is_cancelled() || accepted_docs.len() >= cfg.max_sources {
                break;
            }

            let raw_html = match self
                .fetcher
                .fetch_page(&page.url, &self.fetch_semaphore, &cancel_token)
                .await
            {
                Ok(html) => html,
                Err(err) => {
                    debug!(url = %page.url, %err, "Failed to fetch candidate URL");
                    continue;
                }
            };

            let (clean_text, dom_sha256) = DomCleaner::sanitize_html(&raw_html);
            let wc = word_count(&clean_text);
            if wc < MIN_FALLBACK_WORDS {
                debug!(url = %page.url, wc, "Dropped thin content below minimum floor");
                continue;
            }

            let bm25_relevance = compute_bm25_relevance(&node.query, &clean_text);
            let minhash_sig = compute_minhash(&clean_text, 64);

            page.raw_dom_hash = dom_sha256.clone();
            page.clean_text = Some(clean_text.clone());

            let title = DomCleaner::extract_title(&raw_html)
                .filter(|t| !t.is_empty())
                .unwrap_or_else(|| {
                    if page.title_hint.is_empty() {
                        page.domain.clone()
                    } else {
                        page.title_hint.clone()
                    }
                });

            let snippet: String = if !page.snippet_hint.is_empty() {
                page.snippet_hint.chars().take(240).collect()
            } else {
                clean_text.chars().take(240).collect()
            };

            let doc = ScrapedDocument {
                doc_id: page.id,
                node_id,
                url: page.url.to_string(),
                root_domain: page.domain.clone(),
                branch_query: node.query.clone(),
                facet_slot: node.slot,
                hop_depth: page.depth as u8,
                bm25_relevance,
                minhash_sig,
                dom_sha256: dom_sha256.clone(),
                clean_text: clean_text.clone(),
            };

            let breaker = probe.evaluate_circuit_breakers(&doc);
            if matches!(
                breaker,
                ProbeStatus::KilledDrift | ProbeStatus::ExhaustedSaturated
            ) {
                debug!(url = %page.url, ?breaker, bm25_relevance, "Circuit breaker rejected document");
                continue;
            }

            let source_meta = SourceMetadata {
                url: doc.url.clone(),
                domain: doc.root_domain.clone(),
                title,
                snippet,
                dom_sha256,
                hop_depth: doc.hop_depth,
            };

            if wc >= self.config.min_quality_words {
                accepted_docs.push(doc);
                collected_sources.push(source_meta);
                self.update_node_sources(node_id, collected_sources.clone())
                    .await;
            } else {
                subthreshold_docs.push((doc, source_meta));
            }

            // Branching Gate: Depth 0 pages can harvest up to 2 primary evidence citations (Depth 1).
            if page.depth == 0
                && !breaker.is_terminal_stop()
                && accepted_docs.len() + page_queue.len() < cfg.max_sources * 2
            {
                let citations =
                    DomCleaner::harvest_citation_links_with_base(&raw_html, Some(&page.url));
                let mut added_citations = 0usize;
                for cit_url in citations {
                    if added_citations >= MAX_CITATIONS_PER_PAGE {
                        break;
                    }
                    if frontier.allow_url(&cit_url) {
                        let domain = FrontierGuard::extract_root_domain(&cit_url);
                        page_queue.push_back(PageNode::new(
                            cit_url,
                            domain,
                            String::new(),
                            String::new(),
                            1,
                            Some(page.id),
                        ));
                        added_citations += 1;
                    }
                }
            }

            if breaker.is_terminal_stop() {
                terminal_reason = Some(breaker.as_label().to_string());
                break;
            }
        }

        // Promote sub-threshold fallback docs if zero pages cleared the full 250-word bar
        // (e.g., concise abstracts or paywalled landing summaries).
        if accepted_docs.is_empty() && !subthreshold_docs.is_empty() {
            for (doc, meta) in subthreshold_docs.into_iter().take(cfg.max_sources) {
                accepted_docs.push(doc);
                collected_sources.push(meta);
            }
        }

        let probe_reason = terminal_reason.or_else(|| Some(probe.status.as_label().to_string()));

        // Save raw text dump to disk (`data/raw/{node_id}.txt`)
        self.storage.write_raw_dump(&node, &accepted_docs).await;

        // Phase 3: Claim Extraction, 50/50 Perspective Equalizer, & Orthogonal Branch Discovery
        let raw_claims = if self.config.use_ollama {
            ClaimExtractor::extract_via_slm(&accepted_docs, 10).await
        } else {
            ClaimExtractor::extract_algorithmic(&accepted_docs, 10)
        };
        let eq_report = PerspectiveEqualizer::balance_claims_with_report(raw_claims, 6);

        let candidate_branches = CascadedDiscovery::synthesize_candidate_branches(
            &node.query,
            &accepted_docs,
            &macro_facets,
            8,
        );
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
            if stored.status == NodeStatus::Pruned {
                return;
            }
            stored.status = NodeStatus::Complete;
            stored.probe_reason = probe_reason;
            stored.asymmetry_flag = eq_report.asymmetry_flag;
            stored.findings = eq_report.claims;
            stored.suggested_branches = branches.clone();
            stored.sources = collected_sources;
            stored.clone()
        };

        self.storage.write_node_summary(&completed_node).await;

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
                        prompt_override: None,
                        probe_reason: None,
                        asymmetry_flag: None,
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

    async fn transition_status(&self, node_id: Uuid, status: NodeStatus) -> bool {
        let maybe_node = {
            let mut graph = self.graph.write().await;
            let Some(node) = graph.get_node_mut(&node_id) else {
                return false;
            };
            if node.status == NodeStatus::Pruned {
                return false;
            }
            node.status = status;
            node.clone()
        };

        self.emit(EngineBroadcast::NodeUpdated {
            node_id,
            status,
            node: maybe_node,
        });
        true
    }

    async fn update_node_sources(&self, node_id: Uuid, sources: Vec<SourceMetadata>) {
        let maybe_node = {
            let mut graph = self.graph.write().await;
            let Some(node) = graph.get_node_mut(&node_id) else {
                return;
            };
            if node.status == NodeStatus::Pruned {
                return;
            }
            node.sources = sources;
            node.clone()
        };

        self.emit(EngineBroadcast::NodeUpdated {
            node_id,
            status: maybe_node.status,
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
        let cfg = CoordinatorConfig {
            search_endpoint: String::new(),
            data_dir: None,
            use_ollama: false,
            max_concurrent_fetches: 2,
            min_quality_words: 50,
        };
        let coordinator = ResearchCoordinator::with_config(tx, cfg);

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
