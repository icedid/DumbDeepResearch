/**
 * TypeScript Mirror of `ddr-protocol` Rust structs and WebSocket messages.
 */

export type NodeStatus =
  | 'PROPOSED'
  | 'QUEUED'
  | 'SEARCHING'
  | 'SCRAPING'
  | 'COMPLETE'
  | 'PRUNED';

export type FacetSlot =
  | 'Mechanistic'
  | 'Quantitative'
  | 'Economic'
  | 'Comparative'
  | 'Exploratory';

export type Polarity = 'Bull' | 'Bear' | 'NeutralMetric';

export interface AtomicClaim {
  id: string;
  source_url: string;
  dom_sha256: string;
  statement: string;
  polarity: Polarity;
}

export interface SourceMetadata {
  url: string;
  domain: string;
  title: string;
  snippet: string;
  dom_sha256: string;
  hop_depth: number;
}

export interface SuggestedBranch {
  topic: string;
  query: string;
  slot: FacetSlot;
  significance_score: number;
}

export interface KnowledgeNode {
  id: string;
  parent_id: string | null;
  depth: number;
  topic: string;
  query: string;
  slot: FacetSlot;
  status: NodeStatus;
  findings: AtomicClaim[];
  suggested_branches: SuggestedBranch[];
  sources: SourceMetadata[];
}

export type ClientAction =
  | { type: 'START'; topic: string; max_depth: number; max_sources: number; mode: 'InteractiveSteered' | 'AutonomousFree' }
  | { type: 'EXPLORE'; node_id: string }
  | { type: 'PROBE'; node_id: string; custom_query: string; prompt_override?: string }
  | { type: 'PRUNE'; node_id: string };

export type EngineBroadcast =
  | { type: 'NODE_ADDED'; node: KnowledgeNode }
  | { type: 'NODE_UPDATED'; node_id: string; status: NodeStatus; node: KnowledgeNode }
  | { type: 'NODE_PRUNED'; node_id: string };
