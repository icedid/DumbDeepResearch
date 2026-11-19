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

export type ExecutionMode = 'InteractiveSteered' | 'AutonomousFree';

export interface AtomicClaim {
  id: string;
  source_url: string;
  dom_sha256: string;
  subject: string;
  statement: string;
  polarity: Polarity;
  confidence: number;
  cluster_id: number | null;
  is_selected: boolean;
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
  prompt_override: string | null;
  probe_reason: string | null;
  asymmetry_flag: string | null;
  findings: AtomicClaim[];
  suggested_branches: SuggestedBranch[];
  sources: SourceMetadata[];
}

export interface CitationAuditResult {
  total_citations: number;
  verified_citations: number;
  unverified_citations: string[];
  passed: boolean;
}

export interface DossierLineageEntry {
  claim_id: string;
  short_id: string;
  node_id: string;
  node_topic: string;
  query: string;
  polarity: Polarity;
  statement: string;
  source_url: string;
  dom_sha256: string;
  hop_depth: number;
}

export interface AuditDossier {
  root_topic: string;
  node_count: number;
  source_count: number;
  claim_count: number;
  bull_count: number;
  bear_count: number;
  neutral_count: number;
  markdown: string;
  lineage: DossierLineageEntry[];
  citation_audit: CitationAuditResult;
}

export type ClientAction =
  | {
      type: 'START';
      topic: string;
      max_depth: number;
      max_sources: number;
      mode: ExecutionMode;
    }
  | { type: 'EXPLORE'; node_id: string }
  | {
      type: 'PROBE';
      node_id: string;
      custom_query: string;
      prompt_override?: string;
    }
  | { type: 'PRUNE'; node_id: string }
  | { type: 'RESET' };

export type EngineBroadcast =
  | { type: 'NODE_ADDED'; node: KnowledgeNode }
  | { type: 'NODE_UPDATED'; node_id: string; status: NodeStatus; node: KnowledgeNode }
  | { type: 'NODE_PRUNED'; node_id: string }
  | {
      type: 'GRAPH_SNAPSHOT';
      nodes: KnowledgeNode[];
      max_depth: number;
      max_sources: number;
      mode: ExecutionMode;
    };
