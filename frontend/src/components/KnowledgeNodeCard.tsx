import React, { useState } from 'react';
import { Handle, NodeProps, Position } from '@xyflow/react';
import {
  AlertTriangle,
  ChevronDown,
  ChevronUp,
  Compass,
  ExternalLink,
  Eye,
  GitBranch,
  Loader2,
  Plus,
  Search,
  ShieldCheck,
  X,
} from 'lucide-react';
import { KnowledgeNodeFlowData } from '../utils/layout';
import { FacetSlot, NodeStatus, Polarity } from '../types/protocol';

const SLOT_BADGE_COLORS: Record<FacetSlot, string> = {
  Mechanistic: 'bg-violet-500/15 text-violet-300 border-violet-500/30',
  Quantitative: 'bg-cyan-500/15 text-cyan-300 border-cyan-500/30',
  Economic: 'bg-emerald-500/15 text-emerald-300 border-emerald-500/30',
  Comparative: 'bg-amber-500/15 text-amber-300 border-amber-500/30',
  Exploratory: 'bg-slate-700/60 text-slate-300 border-slate-600/60',
};

const POLARITY_STYLES: Record<Polarity, { label: string; badge: string }> = {
  Bull: {
    label: 'BULL',
    badge: 'bg-emerald-500/15 text-emerald-300 border-emerald-500/30',
  },
  Bear: {
    label: 'BEAR',
    badge: 'bg-rose-500/15 text-rose-300 border-rose-500/30',
  },
  NeutralMetric: {
    label: 'METRIC',
    badge: 'bg-sky-500/15 text-sky-300 border-sky-500/30',
  },
};

function statusContainerStyle(status: NodeStatus): string {
  switch (status) {
    case 'PROPOSED':
      return 'border-dashed border-amber-400/65 bg-slate-900/80 shadow-lg shadow-amber-950/20';
    case 'QUEUED':
    case 'SEARCHING':
    case 'SCRAPING':
      return 'border-cyan-400/80 bg-slate-900/95 shadow-xl shadow-cyan-950/40 ring-1 ring-cyan-400/30';
    case 'COMPLETE':
      return 'border-slate-700/90 bg-slate-900/95 shadow-xl shadow-black/50';
    case 'PRUNED':
      return 'border-rose-900/60 bg-slate-950/70 opacity-55';
  }
}

function statusPill(status: NodeStatus) {
  switch (status) {
    case 'PROPOSED':
      return (
        <span className="inline-flex items-center gap-1 rounded-full border border-amber-500/40 bg-amber-500/10 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wider text-amber-300">
          <GitBranch className="h-3 w-3" /> Proposed Choice
        </span>
      );
    case 'QUEUED':
      return (
        <span className="inline-flex items-center gap-1 rounded-full border border-cyan-500/40 bg-cyan-500/10 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wider text-cyan-300">
          <Loader2 className="h-3 w-3 animate-spin" /> Queued
        </span>
      );
    case 'SEARCHING':
      return (
        <span className="inline-flex items-center gap-1 rounded-full border border-cyan-500/40 bg-cyan-500/15 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wider text-cyan-200">
          <Loader2 className="h-3 w-3 animate-spin" /> Searching
        </span>
      );
    case 'SCRAPING':
      return (
        <span className="inline-flex items-center gap-1 rounded-full border border-indigo-400/40 bg-indigo-500/15 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wider text-indigo-200">
          <Loader2 className="h-3 w-3 animate-spin" /> Scraping DOM
        </span>
      );
    case 'COMPLETE':
      return (
        <span className="inline-flex items-center gap-1 rounded-full border border-emerald-500/40 bg-emerald-500/10 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wider text-emerald-300">
          <ShieldCheck className="h-3 w-3" /> Complete
        </span>
      );
    case 'PRUNED':
      return (
        <span className="inline-flex items-center gap-1 rounded-full border border-rose-500/40 bg-rose-500/10 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wider text-rose-300">
          <X className="h-3 w-3" /> Pruned
        </span>
      );
  }
}

export const KnowledgeNodeCard: React.FC<NodeProps> = ({ data }) => {
  const { node, onExplore, onPrune, onProbe, onInspect } =
    data as unknown as KnowledgeNodeFlowData;

  const [probeOpen, setProbeOpen] = useState(false);
  const [sourcesOpen, setSourcesOpen] = useState(false);
  const [customQuery, setCustomQuery] = useState(node.query);
  const [promptOverride, setPromptOverride] = useState(node.prompt_override ?? '');

  const bullCount = node.findings.filter((c) => c.polarity === 'Bull').length;
  const bearCount = node.findings.filter((c) => c.polarity === 'Bear').length;
  const neutralCount = node.findings.filter((c) => c.polarity === 'NeutralMetric').length;
  const totalClaims = Math.max(1, node.findings.length);

  const isRunning =
    node.status === 'QUEUED' || node.status === 'SEARCHING' || node.status === 'SCRAPING';
  const isProposed = node.status === 'PROPOSED';
  const isPruned = node.status === 'PRUNED';

  const handleSubmitProbe = (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = customQuery.trim();
    if (!trimmed) return;
    onProbe(node.id, trimmed, promptOverride.trim() || undefined);
    setProbeOpen(false);
  };

  return (
    <div
      className={`w-[390px] rounded-xl border-2 text-slate-100 backdrop-blur-md transition-all ${statusContainerStyle(
        node.status
      )}`}
    >
      <Handle
        type="target"
        position={Position.Top}
        className="!h-2.5 !w-2.5 !border-slate-900 !bg-cyan-400"
      />

      {/* Top Metadata Strip */}
      <div className="flex items-center justify-between gap-2 border-b border-slate-800/80 px-3.5 py-2.5">
        <div className="flex flex-wrap items-center gap-1.5">
          <span className="rounded bg-slate-800 px-1.5 py-0.5 font-mono text-[10px] font-bold text-slate-300">
            D{node.depth}
          </span>
          <span
            className={`rounded border px-1.5 py-0.5 text-[10px] font-medium ${
              SLOT_BADGE_COLORS[node.slot]
            }`}
          >
            {node.slot}
          </span>
          {statusPill(node.status)}
        </div>

        <button
          type="button"
          onClick={() => onInspect(node.id)}
          title="Inspect node lineage & full SHA-256 provenance"
          className="nodrag rounded p-1 text-slate-400 hover:bg-slate-800 hover:text-cyan-300"
        >
          <Eye className="h-3.5 w-3.5" />
        </button>
      </div>

      {/* Topic & Query Header */}
      <div className="px-3.5 pt-3 pb-2">
        <h3
          className={`text-sm font-semibold leading-snug ${
            isPruned ? 'line-through text-slate-500' : 'text-slate-100'
          }`}
        >
          {node.topic}
        </h3>
        <div className="mt-1 flex items-start gap-1.5 text-[11px] text-slate-400">
          <Search className="mt-0.5 h-3 w-3 shrink-0 text-slate-500" />
          <span className="font-mono leading-tight break-words">{node.query}</span>
        </div>
        {node.prompt_override && (
          <div className="mt-1 rounded bg-indigo-950/60 border border-indigo-500/30 px-2 py-0.5 text-[10px] text-indigo-300">
            Override: {node.prompt_override}
          </div>
        )}
      </div>

      {/* Perspective Balance Bar & Findings */}
      {node.findings.length > 0 && (
        <div className="px-3.5 pb-2">
          <div className="mb-2">
            <div className="mb-1 flex items-center justify-between text-[10px] text-slate-400">
              <span>50/50 Perspective Balance</span>
              <span className="font-mono">
                <span className="text-emerald-400">{bullCount}B</span> /{' '}
                <span className="text-rose-400">{bearCount}R</span> /{' '}
                <span className="text-sky-400">{neutralCount}M</span>
              </span>
            </div>
            <div className="flex h-1.5 w-full overflow-hidden rounded-full bg-slate-800">
              <div
                style={{ width: `${(bullCount / totalClaims) * 100}%` }}
                className="bg-emerald-500"
              />
              <div
                style={{ width: `${(bearCount / totalClaims) * 100}%` }}
                className="bg-rose-500"
              />
              <div
                style={{ width: `${(neutralCount / totalClaims) * 100}%` }}
                className="bg-sky-500"
              />
            </div>
          </div>

          {node.asymmetry_flag && (
            <div className="mb-2 flex items-start gap-1.5 rounded-md border border-amber-500/30 bg-amber-500/10 px-2 py-1.5 text-[10px] leading-tight text-amber-200">
              <AlertTriangle className="mt-0.5 h-3 w-3 shrink-0 text-amber-400" />
              <span>{node.asymmetry_flag}</span>
            </div>
          )}

          <ul className="space-y-1.5">
            {node.findings.slice(0, 4).map((claim) => {
              const pStyle = POLARITY_STYLES[claim.polarity];
              return (
                <li
                  key={claim.id}
                  className="rounded-lg border border-slate-800/90 bg-slate-950/60 px-2.5 py-2 text-[11px] leading-relaxed text-slate-200"
                >
                  <div className="mb-1 flex items-center justify-between gap-2">
                    <span
                      className={`rounded border px-1.5 py-0.5 font-mono text-[9px] font-bold ${pStyle.badge}`}
                    >
                      {pStyle.label}
                    </span>
                    <div className="flex items-center gap-1.5 font-mono text-[9px] text-slate-500">
                      <span title={`DOM SHA-256: ${claim.dom_sha256}`}>
                        sha:{claim.dom_sha256.slice(0, 8)}
                      </span>
                      <a
                        href={claim.source_url}
                        target="_blank"
                        rel="noreferrer"
                        className="nodrag inline-flex items-center gap-0.5 text-cyan-400 hover:text-cyan-300"
                      >
                        src <ExternalLink className="h-2.5 w-2.5" />
                      </a>
                    </div>
                  </div>
                  <p className="text-slate-200">{claim.statement}</p>
                </li>
              );
            })}
          </ul>
        </div>
      )}

      {/* Circuit Breaker & Sources Accordion */}
      {(node.sources.length > 0 || node.probe_reason) && (
        <div className="border-t border-slate-800/80 px-3.5 py-2">
          <div className="flex items-center justify-between text-[11px]">
            <button
              type="button"
              onClick={() => setSourcesOpen((v) => !v)}
              className="nodrag inline-flex items-center gap-1 font-medium text-slate-300 hover:text-cyan-300"
            >
              {sourcesOpen ? (
                <ChevronUp className="h-3.5 w-3.5" />
              ) : (
                <ChevronDown className="h-3.5 w-3.5" />
              )}
              Sources ({node.sources.length})
            </button>

            {node.probe_reason && (
              <span className="rounded bg-slate-800/90 px-1.5 py-0.5 font-mono text-[9px] text-slate-400">
                {node.probe_reason}
              </span>
            )}
          </div>

          {sourcesOpen && (
            <div className="nodrag nowheel mt-2 max-h-36 space-y-1.5 overflow-y-auto pr-1">
              {node.sources.map((src, i) => (
                <div
                  key={`${src.url}-${i}`}
                  className="rounded border border-slate-800 bg-slate-950/70 px-2 py-1.5 text-[10px]"
                >
                  <div className="flex items-center justify-between gap-2">
                    <a
                      href={src.url}
                      target="_blank"
                      rel="noreferrer"
                      className="truncate font-medium text-cyan-300 hover:underline"
                    >
                      {src.title || src.domain}
                    </a>
                    <span className="shrink-0 rounded bg-slate-800 px-1 font-mono text-[9px] text-slate-400">
                      H{src.hop_depth}
                    </span>
                  </div>
                  <div className="mt-0.5 flex items-center justify-between font-mono text-[9px] text-slate-500">
                    <span className="truncate">{src.domain}</span>
                    <span>sha:{src.dom_sha256.slice(0, 10)}</span>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      )}

      {/* Inline Probe Form */}
      {probeOpen && !isPruned && (
        <form
          onSubmit={handleSubmitProbe}
          className="nodrag nopan border-t border-slate-800 bg-slate-950/90 px-3.5 py-2.5 space-y-2"
        >
          <div>
            <label className="block text-[10px] font-medium text-slate-400 mb-1">
              Custom Search Query
            </label>
            <input
              type="text"
              value={customQuery}
              onChange={(e) => setCustomQuery(e.target.value)}
              className="w-full rounded border border-slate-700 bg-slate-900 px-2 py-1 text-xs text-slate-100 focus:border-cyan-400 focus:outline-none"
              placeholder="Override query terms..."
            />
          </div>
          <div>
            <label className="block text-[10px] font-medium text-slate-400 mb-1">
              Focus Override (optional)
            </label>
            <input
              type="text"
              value={promptOverride}
              onChange={(e) => setPromptOverride(e.target.value)}
              className="w-full rounded border border-slate-700 bg-slate-900 px-2 py-1 text-xs text-slate-100 focus:border-cyan-400 focus:outline-none"
              placeholder="e.g. cost breakdown, regulatory filings..."
            />
          </div>
          <div className="flex justify-end gap-1.5">
            <button
              type="button"
              onClick={() => setProbeOpen(false)}
              className="rounded border border-slate-700 px-2 py-1 text-[11px] text-slate-300 hover:bg-slate-800"
            >
              Cancel
            </button>
            <button
              type="submit"
              className="rounded bg-indigo-600 px-2.5 py-1 text-[11px] font-semibold text-white hover:bg-indigo-500"
            >
              Run Probe
            </button>
          </div>
        </form>
      )}

      {/* Human Steering Controls Footer */}
      {!isPruned && (
        <div className="nodrag flex items-center justify-between gap-2 border-t border-slate-800/80 bg-slate-950/50 px-3.5 py-2 rounded-b-xl">
          <div className="flex items-center gap-1.5">
            <button
              type="button"
              disabled={isRunning}
              onClick={() => onExplore(node.id)}
              className={`inline-flex items-center gap-1 rounded-md px-2.5 py-1 text-[11px] font-semibold transition ${
                isRunning
                  ? 'cursor-not-allowed bg-slate-800 text-slate-500'
                  : isProposed
                    ? 'bg-amber-500 text-slate-950 hover:bg-amber-400'
                    : 'bg-cyan-500/15 text-cyan-300 border border-cyan-500/30 hover:bg-cyan-500/25'
              }`}
            >
              <Plus className="h-3.5 w-3.5" />
              {isProposed ? 'Explore (+)' : 'Re-Explore'}
            </button>

            <button
              type="button"
              onClick={() => {
                setCustomQuery(node.query);
                setProbeOpen((v) => !v);
              }}
              className="inline-flex items-center gap-1 rounded-md border border-indigo-500/30 bg-indigo-500/10 px-2.5 py-1 text-[11px] font-medium text-indigo-300 hover:bg-indigo-500/20"
            >
              <Compass className="h-3.5 w-3.5" />
              Probe
            </button>
          </div>

          <button
            type="button"
            onClick={() => onPrune(node.id)}
            className="inline-flex items-center gap-1 rounded-md border border-rose-500/30 bg-rose-500/10 px-2 py-1 text-[11px] font-medium text-rose-300 hover:bg-rose-500/20"
            title="Prune branch and cancel active scraping"
          >
            <X className="h-3.5 w-3.5" />
            Prune
          </button>
        </div>
      )}

      <Handle
        type="source"
        position={Position.Bottom}
        className="!h-2.5 !w-2.5 !border-slate-900 !bg-cyan-400"
      />
    </div>
  );
};
