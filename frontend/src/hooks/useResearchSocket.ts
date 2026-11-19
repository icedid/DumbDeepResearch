import { useCallback, useEffect, useRef, useState } from 'react';
import {
  ClientAction,
  EngineBroadcast,
  ExecutionMode,
  KnowledgeNode,
} from '../types/protocol';

export type ConnectionState = 'connecting' | 'connected' | 'disconnected';

interface UseResearchSocketResult {
  nodesMap: Record<string, KnowledgeNode>;
  connectionState: ConnectionState;
  lastError: string | null;
  sendAction: (action: ClientAction) => void;
  startResearch: (
    topic: string,
    maxDepth: number,
    maxSources: number,
    mode: ExecutionMode
  ) => void;
  exploreNode: (nodeId: string) => void;
  probeNode: (nodeId: string, customQuery: string, promptOverride?: string) => void;
  pruneNode: (nodeId: string) => void;
  resetSession: () => void;
}

function getWsUrl(): string {
  const envUrl = import.meta.env.VITE_WS_URL as string | undefined;
  if (envUrl && envUrl.trim().length > 0) {
    return envUrl;
  }
  const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
  return `${protocol}//${window.location.host}/ws`;
}

export function useResearchSocket(): UseResearchSocketResult {
  const [nodesMap, setNodesMap] = useState<Record<string, KnowledgeNode>>({});
  const [connectionState, setConnectionState] = useState<ConnectionState>('connecting');
  const [lastError, setLastError] = useState<string | null>(null);

  const wsRef = useRef<WebSocket | null>(null);
  const reconnectTimerRef = useRef<number | null>(null);
  const unmountedRef = useRef(false);

  const handleBroadcast = useCallback((msg: EngineBroadcast) => {
    switch (msg.type) {
      case 'GRAPH_SNAPSHOT': {
        const next: Record<string, KnowledgeNode> = {};
        for (const n of msg.nodes) {
          next[n.id] = n;
        }
        setNodesMap(next);
        break;
      }
      case 'NODE_ADDED': {
        setNodesMap((prev) => ({
          ...prev,
          [msg.node.id]: msg.node,
        }));
        break;
      }
      case 'NODE_UPDATED': {
        setNodesMap((prev) => ({
          ...prev,
          [msg.node_id]: msg.node,
        }));
        break;
      }
      case 'NODE_PRUNED': {
        setNodesMap((prev) => {
          const existing = prev[msg.node_id];
          if (!existing) return prev;
          return {
            ...prev,
            [msg.node_id]: {
              ...existing,
              status: 'PRUNED',
            },
          };
        });
        break;
      }
    }
  }, []);

  useEffect(() => {
    unmountedRef.current = false;

    const connect = () => {
      if (unmountedRef.current) return;
      setConnectionState('connecting');

      const ws = new WebSocket(getWsUrl());
      wsRef.current = ws;

      ws.onopen = () => {
        if (unmountedRef.current) {
          ws.close(1000, 'Client unmounted');
          return;
        }
        setConnectionState('connected');
        setLastError(null);
      };

      ws.onmessage = (event) => {
        try {
          const parsed = JSON.parse(event.data as string) as EngineBroadcast;
          handleBroadcast(parsed);
        } catch (err) {
          console.error('Failed to parse WebSocket broadcast:', err);
        }
      };

      ws.onerror = () => {
        if (unmountedRef.current) return;
        setLastError('WebSocket connection error — retrying...');
      };

      ws.onclose = () => {
        if (unmountedRef.current) return;
        setConnectionState('disconnected');
        reconnectTimerRef.current = window.setTimeout(connect, 1800);
      };
    };

    // Defer initial connection slightly so React 18 StrictMode's dev unmount
    // cancels before opening a transient socket that resets the Vite WS proxy.
    const initialTimer = window.setTimeout(connect, 30);

    const handleBeforeUnload = () => {
      const ws = wsRef.current;
      if (ws && ws.readyState === WebSocket.OPEN) {
        ws.close(1000, 'Page unload');
      }
    };
    window.addEventListener('beforeunload', handleBeforeUnload);

    return () => {
      unmountedRef.current = true;
      window.clearTimeout(initialTimer);
      window.removeEventListener('beforeunload', handleBeforeUnload);
      if (reconnectTimerRef.current !== null) {
        window.clearTimeout(reconnectTimerRef.current);
      }
      const ws = wsRef.current;
      if (ws && ws.readyState === WebSocket.OPEN) {
        ws.close(1000, 'Component unmounted');
      }
    };
  }, [handleBroadcast]);

  const sendAction = useCallback((action: ClientAction) => {
    const ws = wsRef.current;
    if (!ws || ws.readyState !== WebSocket.OPEN) {
      setLastError('Not connected to DDR backend server (/ws).');
      return;
    }
    ws.send(JSON.stringify(action));
  }, []);

  const startResearch = useCallback(
    (topic: string, maxDepth: number, maxSources: number, mode: ExecutionMode) => {
      sendAction({
        type: 'START',
        topic,
        max_depth: maxDepth,
        max_sources: maxSources,
        mode,
      });
    },
    [sendAction]
  );

  const exploreNode = useCallback(
    (nodeId: string) => {
      sendAction({ type: 'EXPLORE', node_id: nodeId });
    },
    [sendAction]
  );

  const probeNode = useCallback(
    (nodeId: string, customQuery: string, promptOverride?: string) => {
      sendAction({
        type: 'PROBE',
        node_id: nodeId,
        custom_query: customQuery,
        prompt_override: promptOverride,
      });
    },
    [sendAction]
  );

  const pruneNode = useCallback(
    (nodeId: string) => {
      sendAction({ type: 'PRUNE', node_id: nodeId });
    },
    [sendAction]
  );

  const resetSession = useCallback(() => {
    setNodesMap({});
    sendAction({ type: 'RESET' });
  }, [sendAction]);

  return {
    nodesMap,
    connectionState,
    lastError,
    sendAction,
    startResearch,
    exploreNode,
    probeNode,
    pruneNode,
    resetSession,
  };
}
