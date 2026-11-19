use crate::models::{KnowledgeNode, NodeStatus};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Execution mode for the research session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionMode {
    /// Human approves/prunes/probes each branch on the visual canvas.
    InteractiveSteered,
    /// Engine automatically explores branches with high novelty until saturation or max_depth.
    AutonomousFree,
}

/// WebSocket actions sent from the React Canvas UI to the Rust Engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ClientAction {
    Start {
        topic: String,
        max_depth: u8,
        max_sources: usize,
        mode: ExecutionMode,
    },
    Explore {
        node_id: Uuid,
    },
    Probe {
        node_id: Uuid,
        custom_query: String,
        prompt_override: Option<String>,
    },
    Prune {
        node_id: Uuid,
    },
    Reset,
}

/// WebSocket events broadcast from the Rust Engine to the React Canvas UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EngineBroadcast {
    NodeAdded {
        node: KnowledgeNode,
    },
    NodeUpdated {
        node_id: Uuid,
        status: NodeStatus,
        node: KnowledgeNode,
    },
    NodePruned {
        node_id: Uuid,
    },
    GraphSnapshot {
        nodes: Vec<KnowledgeNode>,
        max_depth: u8,
        max_sources: usize,
        mode: ExecutionMode,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{AtomicClaim, FacetSlot, Polarity};

    #[test]
    fn roundtrips_client_actions_and_broadcasts() {
        let start_json = r#"{"type":"START","topic":"Solid State Batteries","max_depth":2,"max_sources":5,"mode":"InteractiveSteered"}"#;
        let parsed: ClientAction = serde_json::from_str(start_json).expect("valid start JSON");
        assert!(matches!(parsed, ClientAction::Start { max_depth: 2, .. }));

        let claim = AtomicClaim {
            id: Uuid::new_v4(),
            source_url: "https://example.edu/paper".into(),
            dom_sha256: "a".repeat(64),
            subject: "Solid-State Battery".into(),
            statement: "Retains 80% capacity after 1,000 cycles at 25C.".into(),
            polarity: Polarity::NeutralMetric,
            confidence: 0.92,
            cluster_id: Some(0),
            is_selected: true,
        };
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
            findings: vec![claim],
            suggested_branches: vec![],
            sources: vec![],
        };
        let ev = EngineBroadcast::NodeAdded { node };
        let serialized = serde_json::to_string(&ev).expect("serialize");
        let roundtrip: EngineBroadcast = serde_json::from_str(&serialized).expect("deserialize");
        assert!(matches!(roundtrip, EngineBroadcast::NodeAdded { .. }));
    }
}
