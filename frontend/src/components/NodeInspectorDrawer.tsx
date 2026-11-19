import React from 'react';
import { ExternalLink, ShieldCheck, X } from 'lucide-react';
import { KnowledgeNode } from '../types/protocol';

interface NodeInspectorDrawerProps {
  node: KnowledgeNode | null;
  onClose: () => void;
}

export const NodeInspectorDrawer: React.FC<NodeInspectorDrawerProps> = ({ node, onClose }) => {
  if (!node) return null;

  return (
    <aside className="fixed right-0 top-16 bottom-0 z-40 w-full max-w-md border-l border-slate-800 bg-slate-900/95 p-5 shadow-2xl backdrop-blur-md overflow-y-auto">
      <div className="flex items-start justify-between gap-3 border-b border-slate-800 pb-3">
        <div>
          <div className="flex items-center gap-2 text-[11px] font-mono text-cyan-300">
            <span>DEPTH {node.depth}</span>
            <span>·</span>
            <span>{node.slot}</span>
            <span>·</span>
            <span>{node.status}</span>
          </div>
          <h2 className="mt-1 text-base font-semibold text-slate-100">{node.topic}</h2>
        </div>
        <button
          type="button"
          onClick={onClose}
          className="rounded-lg p-1 text-slate-400 hover:bg-slate-800 hover:text-slate-100"
        >
          <X className="h-4 w-4" />
        </button>
      </div>

      <div className="mt-4 space-y-5 text-xs">
        <div>
          <div className="text-[10px] font-semibold uppercase tracking-wider text-slate-400">
            Search Query
          </div>
          <div className="mt-1 rounded-lg border border-slate-800 bg-slate-950 p-2.5 font-mono text-slate-200">
            {node.query}
          </div>
        </div>

        {node.probe_reason && (
          <div className="flex items-center gap-2 rounded-lg border border-slate-800 bg-slate-950/70 px-3 py-2">
            <ShieldCheck className="h-4 w-4 text-emerald-400" />
            <div>
              <div className="text-[10px] uppercase text-slate-400">Circuit Breaker</div>
              <div className="font-mono text-slate-200">{node.probe_reason}</div>
            </div>
          </div>
        )}

        <div>
          <div className="mb-2 text-[10px] font-semibold uppercase tracking-wider text-slate-400">
            Equalized Atomic Claims ({node.findings.length})
          </div>
          {node.findings.length === 0 ? (
            <p className="text-slate-500">No claims extracted yet.</p>
          ) : (
            <div className="space-y-2">
              {node.findings.map((claim) => (
                <div
                  key={claim.id}
                  className="rounded-lg border border-slate-800 bg-slate-950/80 p-3"
                >
                  <div className="mb-1 flex items-center justify-between font-mono text-[10px]">
                    <span className="font-bold text-cyan-300">{claim.polarity}</span>
                    <span className="text-slate-400">
                      conf {(claim.confidence * 100).toFixed(0)}% · cluster{' '}
                      {claim.cluster_id ?? 0}
                    </span>
                  </div>
                  <p className="text-slate-200 leading-relaxed">{claim.statement}</p>
                  <div className="mt-2 font-mono text-[10px] text-slate-500 break-all">
                    DOM SHA-256: {claim.dom_sha256}
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>

        <div>
          <div className="mb-2 text-[10px] font-semibold uppercase tracking-wider text-slate-400">
            Provenance-Stamped Sources ({node.sources.length})
          </div>
          {node.sources.length === 0 ? (
            <p className="text-slate-500">No scraped sources recorded yet.</p>
          ) : (
            <div className="space-y-2">
              {node.sources.map((src, idx) => (
                <div
                  key={`${src.url}-${idx}`}
                  className="rounded-lg border border-slate-800 bg-slate-950/80 p-3"
                >
                  <div className="flex items-start justify-between gap-2">
                    <a
                      href={src.url}
                      target="_blank"
                      rel="noreferrer"
                      className="font-medium text-cyan-300 hover:underline inline-flex items-center gap-1"
                    >
                      {src.title || src.domain}
                      <ExternalLink className="h-3 w-3 shrink-0" />
                    </a>
                    <span className="rounded bg-slate-800 px-1.5 py-0.5 font-mono text-[10px] text-slate-300">
                      Hop {src.hop_depth}
                    </span>
                  </div>
                  <p className="mt-1 text-[11px] text-slate-400">{src.snippet}</p>
                  <div className="mt-2 font-mono text-[10px] text-slate-500 break-all">
                    SHA-256: {src.dom_sha256}
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
    </aside>
  );
};
