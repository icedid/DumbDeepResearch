use ddr_protocol::KnowledgeNode;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::Direction;
use std::collections::{HashMap, HashSet, VecDeque};
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
        self.index_map
            .get(id)
            .and_then(|&idx| self.dag.node_weight(idx))
    }

    /// Retrieves a mutable reference to a Knowledge Node by UUID.
    pub fn get_node_mut(&mut self, id: &Uuid) -> Option<&mut KnowledgeNode> {
        self.index_map
            .get(id)
            .and_then(|&idx| self.dag.node_weight_mut(idx))
    }

    /// Returns all nodes ordered deterministically by `(depth, topic, id)`.
    pub fn all_nodes(&self) -> Vec<KnowledgeNode> {
        let mut nodes: Vec<KnowledgeNode> = self.dag.node_weights().cloned().collect();
        nodes.sort_by(|a, b| {
            a.depth
                .cmp(&b.depth)
                .then_with(|| a.topic.cmp(&b.topic))
                .then_with(|| a.id.cmp(&b.id))
        });
        nodes
    }

    /// Returns all `(parent_id, child_id)` edges in the DAG.
    pub fn all_edges(&self) -> Vec<(Uuid, Uuid)> {
        self.dag
            .edge_indices()
            .filter_map(|edge_idx| {
                let (src, dst) = self.dag.edge_endpoints(edge_idx)?;
                let parent = self.dag.node_weight(src)?;
                let child = self.dag.node_weight(dst)?;
                Some((parent.id, child.id))
            })
            .collect()
    }

    /// Returns all descendant node IDs reachable from `root_id` via BFS (excluding `root_id`).
    pub fn descendants(&self, root_id: &Uuid) -> Vec<Uuid> {
        let Some(&start_idx) = self.index_map.get(root_id) else {
            return Vec::new();
        };
        let mut queue = VecDeque::new();
        let mut visited = HashSet::new();
        let mut result = Vec::new();

        visited.insert(start_idx);
        queue.push_back(start_idx);

        while let Some(curr) = queue.pop_front() {
            for child_idx in self.dag.neighbors_directed(curr, Direction::Outgoing) {
                if visited.insert(child_idx) {
                    if let Some(node) = self.dag.node_weight(child_idx) {
                        result.push(node.id);
                    }
                    queue.push_back(child_idx);
                }
            }
        }
        result
    }

    /// Traces the ancestry path from root down to `node_id`.
    pub fn ancestry_path(&self, node_id: &Uuid) -> Vec<KnowledgeNode> {
        let mut chain = Vec::new();
        let mut cursor = Some(*node_id);
        let mut guard = HashSet::new();

        while let Some(id) = cursor {
            if !guard.insert(id) {
                break;
            }
            if let Some(node) = self.get_node(&id) {
                chain.push(node.clone());
                cursor = node.parent_id;
            } else {
                break;
            }
        }
        chain.reverse();
        chain
    }

    /// Clears all nodes and edges from the DAG.
    pub fn clear(&mut self) {
        self.dag.clear();
        self.index_map.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ddr_protocol::{FacetSlot, NodeStatus};

    fn stub_node(id: Uuid, parent_id: Option<Uuid>, depth: u8, topic: &str) -> KnowledgeNode {
        KnowledgeNode {
            id,
            parent_id,
            depth,
            topic: topic.into(),
            query: topic.to_lowercase(),
            slot: FacetSlot::Exploratory,
            status: NodeStatus::Complete,
            prompt_override: None,
            probe_reason: None,
            asymmetry_flag: None,
            findings: Vec::new(),
            suggested_branches: Vec::new(),
            sources: Vec::new(),
        }
    }

    #[test]
    fn tracks_descendants_and_ancestry() {
        let mut graph = KnowledgeGraph::new();
        let root = Uuid::new_v4();
        let c1 = Uuid::new_v4();
        let c2 = Uuid::new_v4();
        let gc1 = Uuid::new_v4();

        graph.insert_node(stub_node(root, None, 0, "Root"));
        graph.insert_node(stub_node(c1, Some(root), 1, "Child 1"));
        graph.insert_node(stub_node(c2, Some(root), 1, "Child 2"));
        graph.insert_node(stub_node(gc1, Some(c1), 2, "Grandchild 1"));

        let desc = graph.descendants(&root);
        assert_eq!(desc.len(), 3);
        assert!(desc.contains(&gc1));

        let path = graph.ancestry_path(&gc1);
        assert_eq!(path.len(), 3);
        assert_eq!(path[0].id, root);
        assert_eq!(path[2].id, gc1);
    }
}
