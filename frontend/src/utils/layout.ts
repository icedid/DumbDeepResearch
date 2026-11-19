import { Edge, MarkerType, Node } from '@xyflow/react';
import { KnowledgeNode } from '../types/protocol';

const NODE_WIDTH = 390;
const HORIZONTAL_GAP = 56;
const LEVEL_HEIGHT = 430;

export interface KnowledgeNodeFlowData extends Record<string, unknown> {
  node: KnowledgeNode;
  onExplore: (nodeId: string) => void;
  onPrune: (nodeId: string) => void;
  onProbe: (nodeId: string, customQuery: string, promptOverride?: string) => void;
  onInspect: (nodeId: string) => void;
}

export function computeTreePositions(nodes: KnowledgeNode[]): Record<string, { x: number; y: number }> {
  const byId = new Map<string, KnowledgeNode>();
  const childrenMap = new Map<string, KnowledgeNode[]>();

  for (const n of nodes) {
    byId.set(n.id, n);
  }

  const roots: KnowledgeNode[] = [];
  for (const n of nodes) {
    if (n.parent_id && byId.has(n.parent_id)) {
      const list = childrenMap.get(n.parent_id) ?? [];
      list.push(n);
      childrenMap.set(n.parent_id, list);
    } else {
      roots.push(n);
    }
  }

  for (const list of childrenMap.values()) {
    list.sort((a, b) => a.topic.localeCompare(b.topic) || a.id.localeCompare(b.id));
  }
  roots.sort((a, b) => a.depth - b.depth || a.topic.localeCompare(b.topic));

  const subtreeUnits = new Map<string, number>();

  const measure = (id: string): number => {
    const kids = childrenMap.get(id) ?? [];
    if (kids.length === 0) {
      subtreeUnits.set(id, 1);
      return 1;
    }
    const sum = kids.reduce((acc, k) => acc + measure(k.id), 0);
    subtreeUnits.set(id, sum);
    return sum;
  };

  for (const r of roots) {
    measure(r.id);
  }

  const positions: Record<string, { x: number; y: number }> = {};
  const unitWidth = NODE_WIDTH + HORIZONTAL_GAP;

  const place = (node: KnowledgeNode, leftCursor: number, depthLevel: number) => {
    const units = subtreeUnits.get(node.id) ?? 1;
    const x = leftCursor + ((units - 1) * unitWidth) / 2;
    const y = depthLevel * LEVEL_HEIGHT;
    positions[node.id] = { x, y };

    let childCursor = leftCursor;
    const kids = childrenMap.get(node.id) ?? [];
    for (const child of kids) {
      const childUnits = subtreeUnits.get(child.id) ?? 1;
      place(child, childCursor, depthLevel + 1);
      childCursor += childUnits * unitWidth;
    }
  };

  let rootCursor = 0;
  for (const r of roots) {
    const units = subtreeUnits.get(r.id) ?? 1;
    place(r, rootCursor, r.depth);
    rootCursor += units * unitWidth;
  }

  return positions;
}

export function buildFlowGraph(
  nodesMap: Record<string, KnowledgeNode>,
  showPruned: boolean,
  manualPositions: Record<string, { x: number; y: number }>,
  handlers: {
    onExplore: (nodeId: string) => void;
    onPrune: (nodeId: string) => void;
    onProbe: (nodeId: string, customQuery: string, promptOverride?: string) => void;
    onInspect: (nodeId: string) => void;
  }
): {
  flowNodes: Node<KnowledgeNodeFlowData>[];
  flowEdges: Edge[];
} {
  const visibleNodes = Object.values(nodesMap).filter(
    (n) => showPruned || n.status !== 'PRUNED'
  );
  const autoPositions = computeTreePositions(visibleNodes);
  const visibleIds = new Set(visibleNodes.map((n) => n.id));

  const flowNodes: Node<KnowledgeNodeFlowData>[] = visibleNodes.map((n) => ({
    id: n.id,
    type: 'knowledgeNode',
    position: manualPositions[n.id] ?? autoPositions[n.id] ?? { x: 0, y: 0 },
    data: {
      node: n,
      onExplore: handlers.onExplore,
      onPrune: handlers.onPrune,
      onProbe: handlers.onProbe,
      onInspect: handlers.onInspect,
    },
  }));

  const flowEdges: Edge[] = visibleNodes
    .filter((n) => n.parent_id && visibleIds.has(n.parent_id))
    .map((n) => {
      const isProposed = n.status === 'PROPOSED';
      const isPruned = n.status === 'PRUNED';
      const isActive =
        n.status === 'QUEUED' || n.status === 'SEARCHING' || n.status === 'SCRAPING';

      const stroke = isPruned
        ? '#ef4444'
        : isProposed
          ? '#f59e0b'
          : isActive
            ? '#22d3ee'
            : '#38bdf8';

      return {
        id: `${n.parent_id}->${n.id}`,
        source: n.parent_id!,
        target: n.id,
        type: 'smoothstep',
        animated: isActive,
        style: {
          stroke,
          strokeWidth: isProposed ? 1.75 : 2.25,
          strokeDasharray: isProposed ? '6 4' : undefined,
          opacity: isPruned ? 0.45 : 0.9,
        },
        markerEnd: {
          type: MarkerType.ArrowClosed,
          color: stroke,
        },
      };
    });

  return { flowNodes, flowEdges };
}
