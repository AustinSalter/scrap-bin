import { create } from 'zustand';
import type {
  ClusterView,
  Disposition,
  DispositionCounts,
  Fragment,
  HighlightRange,
  SourceType,
  StreamItem,
  ThreadView,
  StatusData,
  SearchResult,
  TriageTab,
} from '../types';
import {
  sidecarStartAll,
  sidecarStatus,
  clusteringGetAll,
  clusteringGetPositions,
  clusteringGetFragments,
  clusteringRun,
  clusteringMoveFragment,
  clusteringMerge,
  clusteringRename,
  threadsGetAll,
  threadsDetect,
  threadsName,
  searchAll,
  pipelineGetStats,
  pipelineGetRecent,
  pipelineCreateNote,
  configGet,
  watcherStart,
  listFragments,
  setDisposition,
  setHighlights,
  getDispositionCounts,
} from '../api/commands';
import {
  deriveStatusData,
  fragmentToStreamItem,
} from '../api/transforms';

export type UIState = 'overview' | 'browsing' | 'threaded';
export type RailIcon = 'landscape' | 'stream' | 'search' | 'settings';
export type ActiveView = 'landscape' | 'settings' | 'stream-triage' | 'inbox-triage';
export type InboxSourceFilter = 'all' | SourceType;

interface LoadingState {
  clusters: boolean;
  fragments: boolean;
  status: boolean;
  search: boolean;
  recluster: boolean;
  saveNote: boolean;
}

interface DragContext {
  type: 'fragment' | 'cluster';
  id: string;
  fromCluster?: number;
}

interface AppState {
  activeView: ActiveView;
  uiState: UIState;
  streamOpen: boolean;
  marginOpen: boolean;
  selectedClusterId: number | null;
  selectedThreadId: string | null;
  activeRailIcon: RailIcon;
  hasNewItems: boolean;

  // Data
  clusters: ClusterView[];
  threads: ThreadView[];
  streamItems: StreamItem[];
  selectedClusterFragments: Fragment[];
  statusData: StatusData;
  clusterPositions: Map<number, { x: number; y: number }>;
  searchQuery: string;
  searchResults: SearchResult[];
  loading: LoadingState;
  error: string | null;
  vaultPath: string | null;

  // Internal state
  _fragmentFetchGen: number;
  _inboxFetchGen: number;

  // Triage state
  triageTab: TriageTab;
  triageFragments: Fragment[];
  triageSelectedId: string | null;
  triageTotal: number;
  triageCounts: DispositionCounts;
  triagePage: number;
  triageLoading: boolean;

  // Inbox triage state
  inboxCards: Fragment[];
  inboxSourceFilter: InboxSourceFilter;
  inboxTriagedCount: number;
  inboxTotalCount: number;
  inboxLoading: boolean;
  inboxAnimating: 'swipe-right' | 'swipe-left' | 'swipe-up' | null;

  // Interaction state
  dragContext: DragContext | null;
  editingClusterId: number | null;
  editingThreadId: string | null;
  highlightedClusterIds: number[];

  // Sync actions
  setActiveView: (view: ActiveView) => void;
  goOverview: () => void;
  goBrowsing: (clusterId: number) => void;
  goThreaded: (threadId?: string) => void;
  toggleStream: () => void;
  selectCluster: (id: number) => void;
  clearSelection: () => void;
  setHasNewItems: (v: boolean) => void;
  clearError: () => void;
  clearSearch: () => void;
  addStreamItems: (fragments: Fragment[]) => void;
  setDragContext: (ctx: DragContext | null) => void;
  clearDragContext: () => void;
  setEditingCluster: (id: number | null) => void;
  setEditingThread: (id: string | null) => void;

  // Inbox triage actions
  goInboxTriage: () => void;
  setInboxSourceFilter: (filter: InboxSourceFilter) => void;
  fetchInboxCards: (sourceFilter?: InboxSourceFilter) => Promise<void>;
  inboxDismiss: () => Promise<void>;
  inboxSkip: () => void;
  inboxPromote: () => Promise<void>;

  // Triage actions
  goStreamTriage: () => void;
  setTriageTab: (tab: TriageTab) => void;
  setTriageSelectedId: (id: string | null) => void;
  fetchTriageFragments: (tab?: TriageTab, page?: number) => Promise<void>;
  fetchTriageCounts: () => Promise<void>;
  triageDisposition: (id: string, disposition: Disposition) => Promise<void>;
  saveHighlights: (id: string, highlights: HighlightRange[]) => Promise<void>;

  // Async actions
  fetchInitialData: () => Promise<void>;
  fetchClusters: () => Promise<void>;
  fetchClusterFragments: (clusterId: number) => Promise<void>;
  fetchStatus: () => Promise<void>;
  recluster: () => Promise<void>;
  runSearch: (query: string) => Promise<void>;
  saveNote: (content: string) => Promise<void>;
  moveFragment: (fragmentId: string, fromCluster: number, toCluster: number) => Promise<void>;
  mergeClusters: (ids: number[]) => Promise<void>;
  renameCluster: (clusterId: number, label: string) => Promise<void>;
  renameThread: (threadId: string, label: string) => Promise<void>;
}

const defaultStatus: StatusData = {
  chromaHealth: 'err',
  embeddingHealth: 'err',
  fragmentCount: 0,
  clusterCount: 0,
  threadCount: 0,
};

export const useAppStore = create<AppState>((set, get) => ({
  activeView: 'landscape',
  uiState: 'overview',
  streamOpen: false,
  marginOpen: false,
  selectedClusterId: null,
  selectedThreadId: null,
  activeRailIcon: 'landscape',
  hasNewItems: false,

  // Data (initialized empty)
  clusters: [],
  threads: [],
  streamItems: [],
  selectedClusterFragments: [],
  statusData: defaultStatus,
  clusterPositions: new Map(),
  searchQuery: '',
  searchResults: [],
  loading: {
    clusters: false,
    fragments: false,
    status: false,
    search: false,
    recluster: false,
    saveNote: false,
  },
  error: null,
  vaultPath: null,

  // Internal state
  _fragmentFetchGen: 0,
  _inboxFetchGen: 0,

  // Inbox triage state
  inboxCards: [],
  inboxSourceFilter: 'all',
  inboxTriagedCount: 0,
  inboxTotalCount: 0,
  inboxLoading: false,
  inboxAnimating: null,

  // Triage state
  triageTab: 'all',
  triageFragments: [],
  triageSelectedId: null,
  triageTotal: 0,
  triageCounts: { signal: 0, inbox: 0, ignored: 0 },
  triagePage: 0,
  triageLoading: false,

  // Interaction state
  dragContext: null,
  editingClusterId: null,
  editingThreadId: null,
  highlightedClusterIds: [],

  // ── Sync actions ──────────────────────────────────────────

  setActiveView: (view) =>
    set({
      activeView: view,
      activeRailIcon: view === 'settings' ? 'settings'
        : view === 'stream-triage' || view === 'inbox-triage' ? 'stream'
        : 'landscape',
    }),

  goOverview: () =>
    set({
      activeView: 'landscape',
      uiState: 'overview',
      streamOpen: false,
      marginOpen: false,
      selectedClusterId: null,
      selectedThreadId: null,
      activeRailIcon: 'landscape',
      selectedClusterFragments: [],
      highlightedClusterIds: [],
    }),

  goBrowsing: (clusterId) => {
    set({
      activeView: 'landscape',
      uiState: 'browsing',
      streamOpen: true,
      marginOpen: true,
      selectedClusterId: clusterId,
      selectedThreadId: null,
      activeRailIcon: 'stream',
    });
    get().fetchClusterFragments(clusterId);
  },

  goThreaded: (threadId) => {
    const { threads } = get();
    let highlighted: number[] = [];
    if (threadId) {
      const thread = threads.find((t) => t.id === threadId);
      if (thread) {
        highlighted = [thread.sourceClusterId, thread.targetClusterId];
      }
    }
    set({
      activeView: 'landscape',
      uiState: 'threaded',
      streamOpen: false,
      marginOpen: false,
      selectedClusterId: null,
      selectedThreadId: threadId ?? null,
      activeRailIcon: 'search',
      selectedClusterFragments: [],
      highlightedClusterIds: highlighted,
    });
  },

  toggleStream: () => {
    const { activeView } = get();
    if (activeView === 'stream-triage' || activeView === 'inbox-triage') {
      get().goOverview();
    } else {
      get().goStreamTriage();
    }
  },

  selectCluster: (id) => {
    set({ selectedClusterId: id, marginOpen: true });
    get().fetchClusterFragments(id);
  },

  clearSelection: () =>
    set({
      selectedClusterId: null,
      marginOpen: false,
      selectedClusterFragments: [],
    }),

  setHasNewItems: (v) => set({ hasNewItems: v }),
  clearError: () => set({ error: null }),

  clearSearch: () =>
    set((s) => ({
      searchQuery: '',
      searchResults: [],
      highlightedClusterIds: [],
      uiState: s.uiState === 'threaded' ? 'overview' : s.uiState,
      activeRailIcon: s.uiState === 'threaded' ? 'landscape' : s.activeRailIcon,
    })),

  setDragContext: (ctx) => set({ dragContext: ctx }),
  clearDragContext: () => set({ dragContext: null }),
  setEditingCluster: (id) => set({ editingClusterId: id }),
  setEditingThread: (id) => set({ editingThreadId: id }),

  // ── Inbox triage actions ──────────────────────────────

  goInboxTriage: () => {
    set({
      activeView: 'inbox-triage',
      activeRailIcon: 'stream',
      inboxTriagedCount: 0,
      inboxTotalCount: 0,
      inboxSourceFilter: 'all',
      inboxAnimating: null,
    });
    get().fetchInboxCards('all');
  },

  setInboxSourceFilter: (filter) => {
    set({
      inboxSourceFilter: filter,
      inboxTriagedCount: 0,
      inboxTotalCount: 0,
      inboxAnimating: null,
    });
    get().fetchInboxCards(filter);
  },

  fetchInboxCards: async (sourceFilter) => {
    const filter = sourceFilter ?? get().inboxSourceFilter;
    const gen = get()._inboxFetchGen + 1;
    set({ inboxLoading: true, _inboxFetchGen: gen });
    try {
      const result = await listFragments({
        disposition: 'inbox',
        source_type: filter === 'all' ? undefined : filter,
        page_size: 200,
      });
      if (get()._inboxFetchGen !== gen) return; // stale
      set({
        inboxCards: result.fragments,
        inboxTotalCount: result.total,
        inboxTriagedCount: 0,
        inboxLoading: false,
      });
    } catch (e) {
      if (get()._inboxFetchGen !== gen) return; // stale
      const msg = e instanceof Error ? e.message : String(e);
      set({ inboxLoading: false, error: `Failed to load inbox: ${msg}` });
    }
  },

  inboxDismiss: async () => {
    const { inboxCards, inboxAnimating } = get();
    if (inboxAnimating || inboxCards.length === 0) return;
    const card = inboxCards[0];
    set({ inboxAnimating: 'swipe-left' });
    setTimeout(async () => {
      try {
        await setDisposition(card.id, 'ignored');
        set((s) => ({
          inboxCards: s.inboxCards.filter((c) => c.id !== card.id),
          inboxTriagedCount: s.inboxTriagedCount + 1,
          inboxAnimating: null,
        }));
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        set({ error: `Dismiss failed: ${msg}`, inboxAnimating: null });
      }
    }, 350);
  },

  inboxSkip: () => {
    const { inboxCards, inboxAnimating } = get();
    if (inboxAnimating || inboxCards.length === 0) return;
    set({ inboxAnimating: 'swipe-up' });
    setTimeout(() => {
      set((s) => {
        const [first, ...rest] = s.inboxCards;
        return {
          inboxCards: [...rest, first],
          inboxAnimating: null,
        };
      });
    }, 350);
  },

  inboxPromote: async () => {
    const { inboxCards, inboxAnimating } = get();
    if (inboxAnimating || inboxCards.length === 0) return;
    const card = inboxCards[0];
    set({ inboxAnimating: 'swipe-right' });
    setTimeout(async () => {
      try {
        await setDisposition(card.id, 'signal');
        set((s) => ({
          inboxCards: s.inboxCards.filter((c) => c.id !== card.id),
          inboxTriagedCount: s.inboxTriagedCount + 1,
          inboxAnimating: null,
        }));
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        set({ error: `Promote failed: ${msg}`, inboxAnimating: null });
      }
    }, 350);
  },

  // ── Triage actions ─────────────────────────────────────

  goStreamTriage: () => {
    set({
      activeView: 'stream-triage',
      activeRailIcon: 'stream',
      triageTab: 'all',
      triagePage: 0,
      triageSelectedId: null,
    });
    get().fetchTriageFragments('all', 0);
    get().fetchTriageCounts();
  },

  setTriageTab: (tab) => {
    set({ triageTab: tab, triagePage: 0 });
    get().fetchTriageFragments(tab, 0);
  },

  setTriageSelectedId: (id) => set({ triageSelectedId: id }),

  fetchTriageFragments: async (tab, page) => {
    const currentTab = tab ?? get().triageTab;
    const currentPage = page ?? get().triagePage;
    set({ triageLoading: true });
    try {
      const disposition = currentTab === 'all' ? undefined : currentTab;
      const result = await listFragments({
        disposition,
        page: currentPage,
        page_size: 50,
      });
      set((s) => {
        // Keep current selection if it exists in the new results, otherwise select first.
        const currentStillExists = s.triageSelectedId
          && result.fragments.some((f) => f.id === s.triageSelectedId);
        return {
          triageFragments: result.fragments,
          triageTotal: result.total,
          triagePage: result.page,
          triageLoading: false,
          triageSelectedId: currentStillExists
            ? s.triageSelectedId
            : (result.fragments.length > 0 ? result.fragments[0].id : null),
        };
      });
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      set({ triageLoading: false, error: `Failed to load fragments: ${msg}` });
    }
  },

  fetchTriageCounts: async () => {
    try {
      const counts = await getDispositionCounts();
      set({ triageCounts: counts });
    } catch {
      // Non-fatal — counts will show stale data.
    }
  },

  triageDisposition: async (id, disposition) => {
    try {
      const updatedFragment = await setDisposition(id, disposition);
      const { triageFragments, triageTab, triageSelectedId } = get();

      // On a filtered tab, remove the item from the list.
      const isFiltered =
        (triageTab !== 'all' && updatedFragment.disposition !== triageTab) ||
        updatedFragment.disposition === 'ignored';
      let newFragments: Fragment[];
      if (isFiltered) {
        newFragments = triageFragments.filter((f) => f.id !== id);
      } else {
        newFragments = triageFragments.map((f) =>
          f.id === id ? updatedFragment : f
        );
      }

      // Auto-advance to next item.
      let nextId = triageSelectedId;
      if (triageSelectedId === id) {
        const oldIndex = triageFragments.findIndex((f) => f.id === id);
        if (isFiltered) {
          // Item removed: select the item that's now at the same index.
          nextId = newFragments[oldIndex]?.id ?? newFragments[oldIndex - 1]?.id ?? null;
        } else {
          // Item updated: advance to the next item.
          nextId = triageFragments[oldIndex + 1]?.id ?? triageSelectedId;
        }
      }

      set({ triageFragments: newFragments, triageSelectedId: nextId });
      get().fetchTriageCounts();
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      set({ error: `Triage failed: ${msg}` });
    }
  },

  saveHighlights: async (id, highlights) => {
    try {
      const updatedFragment = await setHighlights(id, highlights);
      const { triageFragments, inboxCards } = get();

      const newTriageFragments = triageFragments.map((f) =>
        f.id === id ? updatedFragment : f
      );
      const newInboxCards = inboxCards.map((f) =>
        f.id === id ? updatedFragment : f
      );

      set({ triageFragments: newTriageFragments, inboxCards: newInboxCards });
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      set({ error: `Save highlights failed: ${msg}` });
    }
  },

  addStreamItems: (fragments) => {
    const { clusters, streamItems } = get();
    const newItems = fragments.map((f) => fragmentToStreamItem(f, clusters));
    const merged = [...newItems, ...streamItems].slice(0, 100);
    set({ streamItems: merged });
  },

  // ── Async actions ─────────────────────────────────────────

  fetchInitialData: async () => {
    set((s) => ({ loading: { ...s.loading, clusters: true, status: true } }));

    try {
      // Start sidecars.
      await sidecarStartAll();

      // Fetch config and data in parallel.
      const [config, clusters, positions, threads, recentFragments, stats, sidecarSt] =
        await Promise.all([
          configGet(),
          clusteringGetAll().catch(() => [] as ClusterView[]),
          clusteringGetPositions().catch(() => new Map<number, { x: number; y: number }>()),
          threadsGetAll().catch(() => [] as ThreadView[]),
          pipelineGetRecent(30).catch(() => [] as Fragment[]),
          pipelineGetStats().catch(() => null),
          sidecarStatus().catch(() => null),
        ]);

      // Build stream items from recent fragments.
      const streamItems = recentFragments.map((f) =>
        fragmentToStreamItem(f, clusters)
      );

      const statusData = deriveStatusData(
        sidecarSt as Parameters<typeof deriveStatusData>[0],
        stats as Parameters<typeof deriveStatusData>[1],
        clusters.length,
        threads.length,
      );

      set({
        clusters,
        clusterPositions: positions,
        threads,
        streamItems,
        statusData,
        vaultPath: config.vault_path,
        loading: {
          clusters: false, fragments: false, status: false,
          search: false, recluster: false, saveNote: false,
        },
      });

      // Auto-cluster if fragments exist but no clusters yet.
      if (statusData.fragmentCount > 0 && clusters.length === 0) {
        tracing: console.info('[appStore] Fragments exist but 0 clusters — auto-clustering...');
        get().recluster().catch(() => {
          // Non-fatal — user can manually recluster later.
        });
      }

      // Start vault watcher if configured.
      if (config.vault_path) {
        try {
          await watcherStart(config.vault_path);
        } catch {
          // Watcher may already be active — that's fine.
        }
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      set({
        error: `Initialization failed: ${msg}`,
        loading: {
          clusters: false, fragments: false, status: false,
          search: false, recluster: false, saveNote: false,
        },
      });
    }
  },

  fetchClusters: async () => {
    set((s) => ({ loading: { ...s.loading, clusters: true } }));
    try {
      const [clusters, positions, threads] = await Promise.all([
        clusteringGetAll(),
        clusteringGetPositions(),
        threadsGetAll(),
      ]);
      set((s) => ({
        clusters,
        clusterPositions: positions,
        threads,
        loading: { ...s.loading, clusters: false },
      }));
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      set((s) => ({
        error: `Failed to fetch clusters: ${msg}`,
        loading: { ...s.loading, clusters: false },
      }));
    }
  },

  fetchClusterFragments: async (clusterId) => {
    const gen = get()._fragmentFetchGen + 1;
    set((s) => ({ _fragmentFetchGen: gen, loading: { ...s.loading, fragments: true } }));
    try {
      const fragments = await clusteringGetFragments(clusterId);
      if (get()._fragmentFetchGen !== gen) return; // stale
      set((s) => ({
        selectedClusterFragments: fragments,
        loading: { ...s.loading, fragments: false },
      }));
    } catch (e) {
      if (get()._fragmentFetchGen !== gen) return; // stale
      const msg = e instanceof Error ? e.message : String(e);
      set((s) => ({
        error: `Failed to fetch fragments: ${msg}`,
        loading: { ...s.loading, fragments: false },
      }));
    }
  },

  fetchStatus: async () => {
    try {
      const [sidecarSt, stats] = await Promise.all([
        sidecarStatus().catch(() => null),
        pipelineGetStats().catch(() => null),
      ]);
      const { clusters, threads } = get();
      const statusData = deriveStatusData(
        sidecarSt as Parameters<typeof deriveStatusData>[0],
        stats as Parameters<typeof deriveStatusData>[1],
        clusters.length,
        threads.length,
      );
      set({ statusData });
    } catch {
      // Status polling failures are non-fatal.
    }
  },

  recluster: async () => {
    set((s) => ({ loading: { ...s.loading, recluster: true } }));
    try {
      const clusters = await clusteringRun();
      const [threads, positions] = await Promise.all([
        threadsDetect(),
        clusteringGetPositions(),
      ]);
      set((s) => ({
        clusters,
        threads,
        clusterPositions: positions,
        loading: { ...s.loading, recluster: false },
      }));
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      set((s) => ({
        error: `Re-clustering failed: ${msg}`,
        loading: { ...s.loading, recluster: false },
      }));
    }
  },

  runSearch: async (query) => {
    if (!query.trim()) return;
    set((s) => ({
      searchQuery: query,
      loading: { ...s.loading, search: true },
    }));
    try {
      const results = await searchAll({ query });
      // Compute highlighted cluster IDs from search result metadata.
      const clusterIdSet = new Set<number>();
      for (const r of results) {
        const cid = r.metadata?.cluster_id as number | undefined;
        if (cid !== undefined && cid >= 0) clusterIdSet.add(cid);
      }
      set((s) => ({
        searchResults: results,
        highlightedClusterIds: [...clusterIdSet],
        uiState: 'threaded',
        activeRailIcon: 'search',
        loading: { ...s.loading, search: false },
      }));
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      set((s) => ({
        error: `Search failed: ${msg}`,
        loading: { ...s.loading, search: false },
      }));
    }
  },

  saveNote: async (content) => {
    set((s) => ({ loading: { ...s.loading, saveNote: true } }));
    try {
      const { selectedClusterId } = get();
      await pipelineCreateNote({
        content,
        cluster_id: selectedClusterId ?? undefined,
      });

      // Re-fetch fragments for the current cluster.
      if (selectedClusterId !== null) {
        const fragments = await clusteringGetFragments(selectedClusterId);
        set((s) => ({
          selectedClusterFragments: fragments,
          loading: { ...s.loading, saveNote: false },
        }));
      } else {
        set((s) => ({ loading: { ...s.loading, saveNote: false } }));
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      set((s) => ({
        error: `Failed to save note: ${msg}`,
        loading: { ...s.loading, saveNote: false },
      }));
    }
  },

  moveFragment: async (fragmentId, fromCluster, toCluster) => {
    try {
      await clusteringMoveFragment(fragmentId, fromCluster, toCluster);
      await get().fetchClusterFragments(toCluster);
      await get().fetchClusters();
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      set({ error: `Move failed: ${msg}` });
    }
  },

  mergeClusters: async (ids) => {
    try {
      await clusteringMerge(ids);
      await get().fetchClusters();
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      set({ error: `Merge failed: ${msg}` });
    }
  },

  renameCluster: async (clusterId, label) => {
    try {
      await clusteringRename(clusterId, label);
      // Optimistic local update.
      set((s) => ({
        clusters: s.clusters.map((c) =>
          c.label === clusterId ? { ...c, displayLabel: label } : c
        ),
        editingClusterId: null,
      }));
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      set({ error: `Rename failed: ${msg}`, editingClusterId: null });
    }
  },

  renameThread: async (threadId, label) => {
    try {
      await threadsName(threadId, label);
      // Optimistic local update.
      set((s) => ({
        threads: s.threads.map((t) =>
          t.id === threadId ? { ...t, label } : t
        ),
        editingThreadId: null,
      }));
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      set({ error: `Rename failed: ${msg}`, editingThreadId: null });
    }
  },
}));
