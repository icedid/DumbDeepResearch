use ddr_protocol::KnowledgeNode;
use petgraph::graph::{DiGraph, NodeIndex};
use std::collections::HashMap;
use uuid::Uuid;

/// Directed Acyclic Graph (DAG) tracking all Knowledge Nodes, lineage, and completion states.
#[derive(Default)]
pub struct KnowledgeGraph {
    pub dag: DiGraph<KnowledgeNode, ()>,
    pub index_map: HashMap<Uuid, NodeIndex>,
}

impl KnowledgeGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts a root or child Knowledge Node into the DAG.
    pub fn insert_node(&mut self, node: KnowledgeNode) -> NodeIndex {
        let id = node.id;
        let parent_id = node.parent_id;
        let idx = self.dag.add_node(node);
        self.index_map.insert(id, idx);

        if let Some(pid) = parent_id {
            if let Some(&parent_idx) = self.index_map.get(&pid) {
                self.dag.add_edge(parent_idx, idx, ());
            }
        }
        idx
    }

    /// Retrieves a reference to a Knowledge Node by UUID.
    pub fn get_node(&self, id: &Uuid) -> Option<&KnowledgeNode> {
        self.index_map.get(id).and_then(|&idx| self.dag.node_weight(idx))
    }

    /// Retrieves a mutable reference to a Knowledge Node by UUID.
    pub fn get_node_mut(&mut self, id: &Uuid) -> Option<&mut KnowledgeNode> {
        self.index_map.get(id).and_then(|&idx| self.dag.node_weight_mut(idx))
    }
}
