/**
 * The active workspace and its instruments.
 *
 * This replaces the single-instrument `SessionProvider` (concepts.md §6.3).
 * The Rust side owns the workspaces, connections and discovery cache; this
 * context mirrors them for the UI and is the single place the shell, the
 * topology view and the panels read workspace state from.
 *
 * The workspace is the unit of work: instruments are first-class members that
 * persist across disconnects, and the shared timeline records what happened.
 */

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";

import * as api from "@/lib/api";
import { toCommandError, type DiscoveryScanArgs } from "@/lib/api";
import type {
  CachedInstrument,
  CommandError,
  DiscoveryReport,
  InstrumentView,
  NetworkConfig,
  TimelineEvent,
  WorkspaceSummary,
  WorkspaceView,
} from "@/lib/types";

interface WorkspaceContextValue {
  /** The active workspace, or `null` while loading. */
  workspace: WorkspaceView | null;
  /** All saved workspaces. */
  workspaces: WorkspaceSummary[];
  /** The instruments in the active workspace. */
  instruments: InstrumentView[];
  /** The selected instrument's id, or `null`. */
  selectedId: string | null;
  /** The selected instrument, or `null`. */
  selected: InstrumentView | null;
  /** Select an instrument by id. */
  select: (id: string | null) => void;
  /** True while a workspace operation is in flight. */
  loading: boolean;
  /** The last error, or `null`. */
  error: CommandError | null;
  /** Clear the last error. */
  clearError: () => void;
  /** Reload the active workspace and the workspace list. */
  refresh: () => Promise<void>;
  /** Create a workspace and make it active. */
  createWorkspace: (name: string) => Promise<void>;
  /** Open a workspace by id. */
  openWorkspace: (id: string) => Promise<void>;
  /** Delete a workspace by id. */
  deleteWorkspace: (id: string) => Promise<void>;
  /** Add an instrument to the active workspace. */
  addInstrument: (args: api.AddInstrumentArgs) => Promise<InstrumentView | null>;
  /** Remove an instrument from the active workspace. */
  removeInstrument: (id: string) => Promise<void>;
  /** Connect an instrument. */
  connectInstrument: (id: string) => Promise<void>;
  /** Disconnect an instrument. */
  disconnectInstrument: (id: string) => Promise<void>;
  /** The cached discovery results. */
  cached: CachedInstrument[];
  /** The detected network configuration. */
  network: NetworkConfig | null;
  /** True while a discovery scan is running. */
  scanning: boolean;
  /** Run a discovery scan. */
  scan: (args?: DiscoveryScanArgs) => Promise<DiscoveryReport | null>;
  /** Clear the discovery cache. */
  invalidateCache: () => Promise<void>;
  /** The shared timeline, oldest first. */
  timeline: TimelineEvent[];
  /** Reload the timeline. */
  refreshTimeline: () => Promise<void>;
}

const WorkspaceContext = createContext<WorkspaceContextValue | null>(null);

export function WorkspaceProvider({ children }: { children: ReactNode }) {
  const [workspace, setWorkspace] = useState<WorkspaceView | null>(null);
  const [workspaces, setWorkspaces] = useState<WorkspaceSummary[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<CommandError | null>(null);
  const [cached, setCached] = useState<CachedInstrument[]>([]);
  const [network, setNetwork] = useState<NetworkConfig | null>(null);
  const [scanning, setScanning] = useState(false);
  const [timeline, setTimeline] = useState<TimelineEvent[]>([]);

  // Keep the latest selection without re-creating callbacks.
  const selectedRef = useRef<string | null>(null);
  selectedRef.current = selectedId;

  const refreshTimeline = useCallback(async () => {
    try {
      const events = await api.timelineGet(0, Date.now() + 60_000);
      setTimeline(events);
    } catch {
      // A missing timeline is not fatal; the panel shows an empty state.
    }
  }, []);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      const [view, list] = await Promise.all([
        api.workspaceGet(),
        api.workspaceList(),
      ]);
      setWorkspace(view);
      setWorkspaces(list);
      // Keep the selection valid across a refresh.
      const current = selectedRef.current;
      if (current && !view.instruments.some((i) => i.id === current)) {
        setSelectedId(view.instruments[0]?.id ?? null);
      } else if (!current && view.instruments.length > 0) {
        setSelectedId(view.instruments[0].id);
      }
      await refreshTimeline();
    } catch (err) {
      setError(toCommandError(err));
    } finally {
      setLoading(false);
    }
  }, [refreshTimeline]);

  const refreshCache = useCallback(async () => {
    try {
      const [entries, config] = await Promise.all([
        api.discoveryCache(),
        api.networkConfig(),
      ]);
      setCached(entries);
      setNetwork(config);
    } catch {
      // Discovery is optional; ignore transient failures.
    }
  }, []);

  // Load the active workspace and the discovery cache on mount.
  useEffect(() => {
    void refresh();
    void refreshCache();
  }, [refresh, refreshCache]);

  // Refresh the workspace when an instrument's state changes.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let disposed = false;
    void (async () => {
      const stop = await api.onInstrumentState(() => {
        void refresh();
      });
      if (disposed) stop();
      else unlisten = stop;
    })();
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [refresh]);

  const createWorkspace = useCallback(
    async (name: string) => {
      setLoading(true);
      try {
        const view = await api.workspaceCreate(name);
        setWorkspace(view);
        setSelectedId(null);
        setWorkspaces(await api.workspaceList());
      } catch (err) {
        setError(toCommandError(err));
      } finally {
        setLoading(false);
      }
    },
    [],
  );

  const openWorkspace = useCallback(
    async (id: string) => {
      setLoading(true);
      try {
        const view = await api.workspaceOpen(id);
        setWorkspace(view);
        setSelectedId(view.instruments[0]?.id ?? null);
        setWorkspaces(await api.workspaceList());
        await refreshTimeline();
      } catch (err) {
        setError(toCommandError(err));
      } finally {
        setLoading(false);
      }
    },
    [refreshTimeline],
  );

  const deleteWorkspace = useCallback(
    async (id: string) => {
      try {
        await api.workspaceDelete(id);
        await refresh();
      } catch (err) {
        setError(toCommandError(err));
      }
    },
    [refresh],
  );

  const addInstrument = useCallback(
    async (args: api.AddInstrumentArgs) => {
      try {
        const instrument = await api.workspaceAddInstrument(args);
        await refresh();
        setSelectedId(instrument.id);
        return instrument;
      } catch (err) {
        setError(toCommandError(err));
        return null;
      }
    },
    [refresh],
  );

  const removeInstrument = useCallback(
    async (id: string) => {
      try {
        await api.workspaceRemoveInstrument(id);
        await refresh();
      } catch (err) {
        setError(toCommandError(err));
      }
    },
    [refresh],
  );

  const connectInstrument = useCallback(
    async (id: string) => {
      try {
        await api.instrumentConnect(id);
        await refresh();
      } catch (err) {
        setError(toCommandError(err));
        await refresh();
      }
    },
    [refresh],
  );

  const disconnectInstrument = useCallback(
    async (id: string) => {
      try {
        await api.instrumentDisconnect(id);
        await refresh();
      } catch (err) {
        setError(toCommandError(err));
      }
    },
    [refresh],
  );

  const scan = useCallback(
    async (args?: DiscoveryScanArgs) => {
      setScanning(true);
      setError(null);
      try {
        const report = await api.discoveryScan(args);
        await refreshCache();
        await refreshTimeline();
        return report;
      } catch (err) {
        setError(toCommandError(err));
        return null;
      } finally {
        setScanning(false);
      }
    },
    [refreshCache, refreshTimeline],
  );

  const invalidateCache = useCallback(async () => {
    try {
      await api.discoveryInvalidate();
      await refreshCache();
    } catch (err) {
      setError(toCommandError(err));
    }
  }, [refreshCache]);

  const clearError = useCallback(() => setError(null), []);

  const instruments = workspace?.instruments ?? [];
  const selected = useMemo(
    () => instruments.find((instrument) => instrument.id === selectedId) ?? null,
    [instruments, selectedId],
  );

  const value = useMemo<WorkspaceContextValue>(
    () => ({
      workspace,
      workspaces,
      instruments,
      selectedId,
      selected,
      select: setSelectedId,
      loading,
      error,
      clearError,
      refresh,
      createWorkspace,
      openWorkspace,
      deleteWorkspace,
      addInstrument,
      removeInstrument,
      connectInstrument,
      disconnectInstrument,
      cached,
      network,
      scanning,
      scan,
      invalidateCache,
      timeline,
      refreshTimeline,
    }),
    [
      workspace,
      workspaces,
      instruments,
      selectedId,
      selected,
      loading,
      error,
      clearError,
      refresh,
      createWorkspace,
      openWorkspace,
      deleteWorkspace,
      addInstrument,
      removeInstrument,
      connectInstrument,
      disconnectInstrument,
      cached,
      network,
      scanning,
      scan,
      invalidateCache,
      timeline,
      refreshTimeline,
    ],
  );

  return (
    <WorkspaceContext.Provider value={value}>
      {children}
    </WorkspaceContext.Provider>
  );
}

/** Read the active workspace. Throws outside a `WorkspaceProvider`. */
export function useWorkspace(): WorkspaceContextValue {
  const value = useContext(WorkspaceContext);
  if (!value) {
    throw new Error("useWorkspace must be used within a WorkspaceProvider");
  }
  return value;
}
