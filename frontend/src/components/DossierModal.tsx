import React, { useEffect, useState } from 'react';
import {
  CheckCircle2,
  Copy,
  Download,
  ExternalLink,
  FileText,
  ShieldAlert,
  ShieldCheck,
  X,
} from 'lucide-react';
import { AuditDossier } from '../types/protocol';

interface DossierModalProps {
  open: boolean;
  onClose: () => void;
}

export const DossierModal: React.FC<DossierModalProps> = ({ open, onClose }) => {
  const [dossier, setDossier] = useState<AuditDossier | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [activeTab, setActiveTab] = useState<'markdown' | 'lineage'>('markdown');
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (!open) return;
    setLoading(true);
    setError(null);
    fetch('/api/dossier')
      .then((res) => {
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        return res.json() as Promise<AuditDossier>;
      })
      .then((data) => {
        setDossier(data);
      })
      .catch((err: unknown) => {
        setError(err instanceof Error ? err.message : 'Failed to load dossier');
      })
      .finally(() => setLoading(false));
  }, [open]);

  if (!open) return null;

  const handleCopy = async () => {
    if (!dossier) return;
    await navigator.clipboard.writeText(dossier.markdown);
    setCopied(true);
    setTimeout(() => setCopied(false), 1800);
  };

  const handleDownload = () => {
    if (!dossier) return;
    const blob = new Blob([dossier.markdown], { type: 'text/markdown;charset=utf-8' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `ddr-dossier-${dossier.root_topic.toLowerCase().replace(/[^a-z0-9]+/g, '-')}.md`;
    a.click();
    URL.revokeObjectURL(url);
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-950/80 p-4 backdrop-blur-sm">
      <div className="flex max-h-[88vh] w-full max-w-5xl flex-col overflow-hidden rounded-2xl border border-slate-700 bg-slate-900 shadow-2xl">
        {/* Modal Header */}
        <div className="flex items-center justify-between border-b border-slate-800 px-5 py-3.5">
          <div className="flex items-center gap-2.5">
            <FileText className="h-5 w-5 text-cyan-400" />
            <div>
              <h2 className="text-sm font-semibold text-slate-100">
                Audit-Ready Research Dossier & Lineage Verification
              </h2>
              <p className="text-[11px] text-slate-400">
                Deterministic citation audit: Claim ID -&gt; Query -&gt; Source URL -&gt; DOM SHA-256
              </p>
            </div>
          </div>

          <div className="flex items-center gap-2">
            {dossier && (
              <>
                <button
                  type="button"
                  onClick={handleCopy}
                  className="inline-flex items-center gap-1.5 rounded-lg border border-slate-700 bg-slate-800 px-3 py-1.5 text-xs font-medium text-slate-200 hover:bg-slate-700"
                >
                  {copied ? (
                    <CheckCircle2 className="h-3.5 w-3.5 text-emerald-400" />
                  ) : (
                    <Copy className="h-3.5 w-3.5" />
                  )}
                  {copied ? 'Copied' : 'Copy MD'}
                </button>
                <button
                  type="button"
                  onClick={handleDownload}
                  className="inline-flex items-center gap-1.5 rounded-lg bg-cyan-600 px-3 py-1.5 text-xs font-semibold text-white hover:bg-cyan-500"
                >
                  <Download className="h-3.5 w-3.5" />
                  Export .md
                </button>
              </>
            )}
            <button
              type="button"
              onClick={onClose}
              className="rounded-lg p-1.5 text-slate-400 hover:bg-slate-800 hover:text-slate-100"
            >
              <X className="h-4 w-4" />
            </button>
          </div>
        </div>

        {/* Body */}
        <div className="flex-1 overflow-y-auto p-5">
          {loading && (
            <div className="py-12 text-center text-sm text-slate-400">
              Compiling deterministic audit dossier...
            </div>
          )}

          {error && (
            <div className="rounded-lg border border-rose-500/40 bg-rose-500/10 p-4 text-sm text-rose-200">
              Failed to compile dossier: {error}
            </div>
          )}

          {dossier && !loading && (
            <div className="space-y-4">
              {/* Verification & Telemetry Banner */}
              <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
                <div className="rounded-xl border border-slate-800 bg-slate-950/70 p-3">
                  <div className="text-[10px] uppercase tracking-wider text-slate-400">
                    Explored Nodes
                  </div>
                  <div className="mt-1 text-lg font-bold text-slate-100">
                    {dossier.node_count}
                  </div>
                </div>
                <div className="rounded-xl border border-slate-800 bg-slate-950/70 p-3">
                  <div className="text-[10px] uppercase tracking-wider text-slate-400">
                    Verified Sources
                  </div>
                  <div className="mt-1 text-lg font-bold text-cyan-300">
                    {dossier.source_count}
                  </div>
                </div>
                <div className="rounded-xl border border-slate-800 bg-slate-950/70 p-3">
                  <div className="text-[10px] uppercase tracking-wider text-slate-400">
                    Perspective Ratio
                  </div>
                  <div className="mt-1 font-mono text-sm font-bold">
                    <span className="text-emerald-400">{dossier.bull_count} Bull</span> ·{' '}
                    <span className="text-rose-400">{dossier.bear_count} Bear</span> ·{' '}
                    <span className="text-sky-400">{dossier.neutral_count} Metric</span>
                  </div>
                </div>
                <div
                  className={`rounded-xl border p-3 ${
                    dossier.citation_audit.passed
                      ? 'border-emerald-500/40 bg-emerald-500/10'
                      : 'border-rose-500/40 bg-rose-500/10'
                  }`}
                >
                  <div className="flex items-center gap-1.5 text-[10px] uppercase tracking-wider text-slate-300">
                    {dossier.citation_audit.passed ? (
                      <ShieldCheck className="h-3.5 w-3.5 text-emerald-400" />
                    ) : (
                      <ShieldAlert className="h-3.5 w-3.5 text-rose-400" />
                    )}
                    Citation Audit
                  </div>
                  <div className="mt-1 text-sm font-bold text-slate-100">
                    {dossier.citation_audit.verified_citations}/
                    {dossier.citation_audit.total_citations} Verified
                  </div>
                </div>
              </div>

              {/* Tabs */}
              <div className="flex gap-2 border-b border-slate-800 pb-2">
                <button
                  type="button"
                  onClick={() => setActiveTab('markdown')}
                  className={`rounded-lg px-3 py-1.5 text-xs font-medium ${
                    activeTab === 'markdown'
                      ? 'bg-slate-800 text-cyan-300'
                      : 'text-slate-400 hover:text-slate-200'
                  }`}
                >
                  Dossier Markdown
                </button>
                <button
                  type="button"
                  onClick={() => setActiveTab('lineage')}
                  className={`rounded-lg px-3 py-1.5 text-xs font-medium ${
                    activeTab === 'lineage'
                      ? 'bg-slate-800 text-cyan-300'
                      : 'text-slate-400 hover:text-slate-200'
                  }`}
                >
                  Cryptographic Lineage ({dossier.lineage.length})
                </button>
              </div>

              {activeTab === 'markdown' ? (
                <pre className="whitespace-pre-wrap rounded-xl border border-slate-800 bg-slate-950 p-4 font-mono text-xs leading-relaxed text-slate-200">
                  {dossier.markdown}
                </pre>
              ) : (
                <div className="overflow-x-auto rounded-xl border border-slate-800">
                  <table className="w-full text-left text-xs">
                    <thead className="bg-slate-950 text-[10px] uppercase text-slate-400">
                      <tr>
                        <th className="px-3 py-2.5">Claim Tag</th>
                        <th className="px-3 py-2.5">Polarity</th>
                        <th className="px-3 py-2.5">Statement</th>
                        <th className="px-3 py-2.5">Source</th>
                        <th className="px-3 py-2.5">DOM SHA-256</th>
                      </tr>
                    </thead>
                    <tbody className="divide-y divide-slate-800 bg-slate-900/60">
                      {dossier.lineage.map((row) => (
                        <tr key={row.claim_id} className="hover:bg-slate-800/40">
                          <td className="whitespace-nowrap px-3 py-2 font-mono text-cyan-300">
                            CLAIM-{row.short_id}
                          </td>
                          <td className="whitespace-nowrap px-3 py-2 font-mono text-[11px]">
                            {row.polarity}
                          </td>
                          <td className="max-w-md px-3 py-2 text-slate-200">
                            {row.statement}
                          </td>
                          <td className="whitespace-nowrap px-3 py-2">
                            <a
                              href={row.source_url}
                              target="_blank"
                              rel="noreferrer"
                              className="inline-flex items-center gap-1 text-cyan-400 hover:underline"
                            >
                              H{row.hop_depth} link <ExternalLink className="h-3 w-3" />
                            </a>
                          </td>
                          <td
                            className="whitespace-nowrap px-3 py-2 font-mono text-[11px] text-slate-400"
                            title={row.dom_sha256}
                          >
                            {row.dom_sha256.slice(0, 16)}...
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  );
};
