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
}
