import React from 'react';
import { KnowledgeNode } from '../types/protocol';

interface KnowledgeNodeCardProps {
  node: KnowledgeNode;
  onExplore: (nodeId: string) => void;
  onPrune: (nodeId: string) => void;
  onProbe: (nodeId: string, customQuery: string) => void;
}

/**
 * Boilerplate React Flow Custom Node Card (`Explore (+)`, `Prune (x)`, `Probe`).
 */
export const KnowledgeNodeCard: React.FC<KnowledgeNodeCardProps> = ({
  node,
  onExplore,
  onPrune,
}) => {
  return (
    <div className="ddr-node-card">
      <h4>{node.topic}</h4>
      <p>{node.query}</p>
      <span>Status: {node.status}</span>
      <div>
        <button onClick={() => onExplore(node.id)}>Explore (+)</button>
        <button onClick={() => onPrune(node.id)}>Prune (x)</button>
      </div>
    </div>
  );
};
