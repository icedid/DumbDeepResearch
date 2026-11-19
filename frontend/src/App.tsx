import React, { useCallback, useMemo, useState } from 'react';
import {
  Background,
  BackgroundVariant,
  Controls,
  MiniMap,
  NodeChange,
  ReactFlow,
  ReactFlowProvider,
} from '@xyflow/react';
import {
  Bot,
  Eye,
  EyeOff,
  FileText,
  LayoutGrid,
  Play,
  RotateCcw,
  SlidersHorizontal,
  UserCheck,
  Wifi,
  WifiOff,
} from 'lucide-react';
import { KnowledgeNodeCard } from './components/KnowledgeNodeCard';
import { DossierModal } from './components/DossierModal';
import { NodeInspectorDrawer } from './components/NodeInspectorDrawer';
import { useResearchSocket } from './hooks/useResearchSocket';
import { ExecutionMode } from './types/protocol';
import { buildFlowGraph, KnowledgeNodeFlowData } from './utils/layout';

const NODE_TYPES = {
  knowledgeNode: KnowledgeNodeCard,
};

const CanvasWorkspace: React.FC = () => {
  const {
    nodesMap,
    connectionState,
    lastError,
    startResearch,
    exploreNode,
    probeNode,
    pruneNode,
    resetSession,
  } = useResearchSocket();

  const [topic, setTopic] = useState('Solid-state lithium batteries');
  const [maxDepth, setMaxDepth] = useState(2);
  const [maxSources, setMaxSources] = useState(4);
  const [mode, setMode] = useState<ExecutionMode>('InteractiveSteered');
  const [showPruned, setShowPruned] = useState(false);
  const [dossierOpen, setDossierOpen] = useState(false);
  const [inspectedNodeId, setInspectedNodeId] = useState<string | null>(null);
  const [manualPositions, setManualPositions] = useState<
    Record<string, { x: number; y: number }>
  >({});

  const handlers = useMemo(
    () => ({
      onExplore: exploreNode,
      onPrune: pruneNode,
      onProbe: probeNode,
      onInspect: (id: string) => setInspectedNodeId(id),
    }),
    [exploreNode, pruneNode, probeNode]
  );

  const { flowNodes, flowEdges } = useMemo(
    () => buildFlowGraph(nodesMap, showPruned, manualPositions, handlers),
    [nodesMap, showPruned, manualPositions, handlers]
  );

  const onNodesChange = useCallback((changes: NodeChange[]) => {
    for (const change of changes) {
      if (change.type === 'position' && change.position) {
        const pos = change.position;
        setManualPositions((prev) => ({
          ...prev,
          [change.id]: pos,
        }));
      }
    }
  }, []);

  const handleStart = (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = topic.trim();
    if (!trimmed) return;
    setManualPositions({});
    setInspectedNodeId(null);
    startResearch(trimmed, maxDepth, maxSources, mode);
  };

  const allNodes = Object.values(nodesMap);
  const completeCount = allNodes.filter((n) => n.status === 'COMPLETE').length;
  const proposedCount = allNodes.filter((n) => n.status === 'PROPOSED').length;
  const activeCount = allNodes.filter(
    (n) => n.status === 'QUEUED' || n.status === 'SEARCHING' || n.status === 'SCRAPING'
  ).length;

  const inspectedNode = inspectedNodeId ? nodesMap[inspectedNodeId] ?? null : null;

  return (
    <div className="flex h-screen w-screen flex-col bg-slate-950 text-slate-100">
      {/* Top Steering & Control Bar */}
      <header className="z-30 flex flex-wrap items-center justify-between gap-3 border-b border-slate-800/90 bg-slate-900/95 px-4 py-2.5 backdrop-blur-md">
        <div className="flex items-center gap-3">
          <div className="flex items-center gap-2">
            <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-gradient-to-br from-cyan-500 to-indigo-600 font-mono text-xs font-extrabold text-white shadow">
              DDR
            </div>
            <div>
              <h1 className="text-sm font-bold tracking-tight text-slate-100">
                DumbDeepResearch
              </h1>
              <div className="flex items-center gap-1.5 text-[10px]">
                {connectionState === 'connected' ? (
                  <span className="inline-flex items-center gap-1 text-emerald-400">
                    <Wifi className="h-3 w-3" /> Engine Connected
                  </span>
                ) : (
                  <span className="inline-flex items-center gap-1 text-amber-400">
                    <WifiOff className="h-3 w-3" /> {connectionState}
                  </span>
                )}
              </div>
            </div>
          </div>
        </div>

        {/* Research Launch Form */}
        <form onSubmit={handleStart} className="flex flex-1 max-w-2xl items-center gap-2">
          <input
            type="text"
            value={topic}
            onChange={(e) => setTopic(e.target.value)}
            placeholder="Enter research thesis or topic..."
            className="flex-1 rounded-lg border border-slate-700 bg-slate-950 px-3 py-1.5 text-sm text-slate-100 placeholder:text-slate-500 focus:border-cyan-400 focus:outline-none"
          />
          <button
            type="submit"
            disabled={connectionState !== 'connected'}
            className="inline-flex items-center gap-1.5 rounded-lg bg-cyan-500 px-3.5 py-1.5 text-xs font-bold text-slate-950 shadow hover:bg-cyan-400 disabled:cursor-not-allowed disabled:opacity-50"
          >
            <Play className="h-3.5 w-3.5 fill-current" />
            Start
          </button>
        </form>

        {/* Depth, Sources, Mode, Dossier Controls */}
        <div className="flex flex-wrap items-center gap-3 text-xs">
          <div className="flex items-center gap-2 rounded-lg border border-slate-800 bg-slate-950 px-2.5 py-1">
            <SlidersHorizontal className="h-3.5 w-3.5 text-slate-400" />
            <label className="text-[11px] text-slate-400">
              Depth <span className="font-mono font-bold text-cyan-300">{maxDepth}</span>
            </label>
            <input
              type="range"
              min={1}
              max={4}
              value={maxDepth}
              onChange={(e) => setMaxDepth(Number(e.target.value))}
              className="w-16 accent-cyan-400"
            />
            <span className="text-slate-700">|</span>
            <label className="text-[11px] text-slate-400">
              Src <span className="font-mono font-bold text-cyan-300">{maxSources}</span>
            </label>
            <input
              type="range"
              min={2}
              max={10}
              value={maxSources}
              onChange={(e) => setMaxSources(Number(e.target.value))}
              className="w-16 accent-cyan-400"
            />
          </div>

          <div className="flex rounded-lg border border-slate-800 bg-slate-950 p-0.5">
            <button
              type="button"
              onClick={() => setMode('InteractiveSteered')}
              className={`inline-flex items-center gap-1 rounded-md px-2 py-1 text-[11px] font-medium ${
                mode === 'InteractiveSteered'
                  ? 'bg-slate-800 text-cyan-300'
                  : 'text-slate-400 hover:text-slate-200'
              }`}
              title="Human approves/prunes/probes every branch"
            >
              <UserCheck className="h-3 w-3" />
              Steered
            </button>
            <button
              type="button"
              onClick={() => setMode('AutonomousFree')}
              className={`inline-flex items-center gap-1 rounded-md px-2 py-1 text-[11px] font-medium ${
                mode === 'AutonomousFree'
                  ? 'bg-indigo-600/30 text-indigo-300'
                  : 'text-slate-400 hover:text-slate-200'
              }`}
              title="Auto-explores high-significance branches up to max_depth"
            >
              <Bot className="h-3 w-3" />
              Auto
            </button>
          </div>

          <button
            type="button"
            onClick={() => setShowPruned((v) => !v)}
            className="inline-flex items-center gap-1 rounded-lg border border-slate-800 bg-slate-950 px-2.5 py-1.5 text-[11px] text-slate-300 hover:bg-slate-800"
          >
            {showPruned ? <Eye className="h-3.5 w-3.5" /> : <EyeOff className="h-3.5 w-3.5" />}
            Pruned
          </button>

          <button
            type="button"
            onClick={() => setManualPositions({})}
            className="inline-flex items-center gap-1 rounded-lg border border-slate-800 bg-slate-950 px-2.5 py-1.5 text-[11px] text-slate-300 hover:bg-slate-800"
            title="Reset nodes to deterministic hierarchical layout"
          >
            <LayoutGrid className="h-3.5 w-3.5" />
            Layout
          </button>

          <button
            type="button"
            onClick={() => {
              setManualPositions({});
              setInspectedNodeId(null);
              resetSession();
            }}
            className="inline-flex items-center gap-1 rounded-lg border border-slate-800 bg-slate-950 px-2.5 py-1.5 text-[11px] text-slate-300 hover:bg-slate-800"
            title="Clear canvas and cancel active probes"
          >
            <RotateCcw className="h-3.5 w-3.5" />
          </button>

          <button
            type="button"
            onClick={() => setDossierOpen(true)}
            className="inline-flex items-center gap-1.5 rounded-lg border border-emerald-500/40 bg-emerald-500/15 px-3 py-1.5 text-[11px] font-semibold text-emerald-300 hover:bg-emerald-500/25"
          >
            <FileText className="h-3.5 w-3.5" />
            Audit Dossier
          </button>
        </div>
      </header>

      {/* Telemetry & Error Banner */}
      {(lastError || allNodes.length > 0) && (
        <div className="z-20 flex items-center justify-between border-b border-slate-800/70 bg-slate-950/90 px-4 py-1.5 text-[11px] text-slate-400">
          <div className="flex items-center gap-4 font-mono">
            <span>
              Complete: <strong className="text-emerald-400">{completeCount}</strong>
            </span>
            <span>
              Proposed Gates: <strong className="text-amber-400">{proposedCount}</strong>
            </span>
            <span>
              Active Probes: <strong className="text-cyan-400">{activeCount}</strong>
            </span>
          </div>
          {lastError && <span className="text-rose-400">{lastError}</span>}
        </div>
      )}

      {/* Visual React Flow Canvas */}
      <div className="relative flex-1">
        {allNodes.length === 0 ? (
          <div className="flex h-full flex-col items-center justify-center px-6 text-center">
            <div className="max-w-lg rounded-2xl border border-slate-800 bg-slate-900/70 p-8 shadow-2xl">
              <h2 className="text-lg font-bold text-slate-100">
                Deterministic, Human-Steered Deep Research
              </h2>
              <p className="mt-2 text-xs leading-relaxed text-slate-400">
                Enter a topic above to seed the Knowledge Graph. Each node runs a bounded crawl
                with domain ceilings, SHA-256 DOM stamping, 50/50 Bull vs. Bear perspective
                equalization, and Cascaded PRF orthogonal branch discovery.
              </p>
              <div className="mt-5 grid grid-cols-3 gap-2 text-left text-[11px]">
                <div className="rounded-lg border border-slate-800 bg-slate-950/70 p-2.5">
                  <div className="font-semibold text-amber-300">Explore (+)</div>
                  <div className="mt-0.5 text-slate-400">Approve a dashed choice node</div>
                </div>
                <div className="rounded-lg border border-slate-800 bg-slate-950/70 p-2.5">
                  <div className="font-semibold text-indigo-300">Probe</div>
                  <div className="mt-0.5 text-slate-400">Inject custom query terms</div>
                </div>
                <div className="rounded-lg border border-slate-800 bg-slate-950/70 p-2.5">
                  <div className="font-semibold text-rose-300">Prune (x)</div>
                  <div className="mt-0.5 text-slate-400">Cancel &amp; purge branch</div>
                </div>
              </div>
            </div>
          </div>
        ) : (
          <ReactFlow
            nodes={flowNodes}
            edges={flowEdges}
            nodeTypes={NODE_TYPES}
            onNodesChange={onNodesChange}
            fitView
            fitViewOptions={{ padding: 0.25 }}
            minZoom={0.2}
            maxZoom={1.6}
          >
            <Background
              variant={BackgroundVariant.Dots}
              gap={24}
              size={1.2}
              color="#1e293b"
            />
            <Controls />
            <MiniMap
              nodeColor={(n) => {
                const status = (n.data as unknown as KnowledgeNodeFlowData).node.status;
                if (status === 'COMPLETE') return '#10b981';
                if (status === 'PROPOSED') return '#f59e0b';
                if (status === 'PRUNED') return '#ef4444';
                return '#22d3ee';
              }}
              maskColor="rgba(2, 6, 23, 0.7)"
            />
          </ReactFlow>
        )}
      </div>

      <NodeInspectorDrawer
        node={inspectedNode}
        onClose={() => setInspectedNodeId(null)}
      />
      <DossierModal open={dossierOpen} onClose={() => setDossierOpen(false)} />
    </div>
  );
};

export const App: React.FC = () => (
  <ReactFlowProvider>
    <CanvasWorkspace />
  </ReactFlowProvider>
);

export default App;
