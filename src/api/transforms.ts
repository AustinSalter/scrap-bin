import type {
  ClusterView,
  Disposition,
  Fragment,
  FragmentPage,
  HighlightRange,
  ThreadView,
  StreamItem,
  StatusData,
  SearchResult,
  SourceType,
  ServiceHealth,
} from '../types';
import type { SidecarStatus, PipelineStats } from './commands';

// ── Cluster ─────────────────────────────────────────────────

export function transformCluster(raw: Record<string, unknown>): ClusterView {
  return {
    label: typeof raw.label === 'number' ? raw.label : -1,
    displayLabel: typeof raw.display_label === 'string' ? raw.display_label : 'Unlabeled',
    size: typeof raw.size === 'number' ? raw.size : 0,
    pinned: typeof raw.pinned === 'boolean' ? raw.pinned : false,
    fragmentIds: Array.isArray(raw.fragment_ids) ? raw.fragment_ids : [],
  };
}

// ── Fragment ────────────────────────────────────────────────

function normalizeSourceType(raw: string): SourceType {
  if (raw === 'podcasts') return 'podcast';
  if (raw === 'vault' || raw === 'twitter' || raw === 'readwise' || raw === 'podcast' || raw === 'rss' || raw === 'apple_notes') {
    return raw as SourceType;
  }
  return 'vault';
}

function deriveSourceLabel(sourceType: SourceType, metadata: Record<string, unknown>): string {
  const isUserNote = metadata.is_user_note === true;
  if (isUserNote) return 'Your note';

  switch (sourceType) {
    case 'vault': return 'Obsidian';
    case 'twitter': {
      const path = metadata.source_path as string | undefined;
      return path && path.startsWith('@') ? path : 'Twitter';
    }
    case 'readwise': return 'Readwise';
    case 'podcast': {
      const path = metadata.source_path as string | undefined;
      return path ? path.replace(/\.[^.]+$/, '') : 'Podcast';
    }
    case 'rss': return 'RSS';
    case 'apple_notes': return 'Apple Notes';
  }
}

export function transformFragment(raw: Record<string, unknown>): Fragment {
  const metadata = (typeof raw.metadata === 'object' && raw.metadata !== null ? raw.metadata : {}) as Record<string, unknown>;
  const rawSt = typeof raw.source_type === 'string' ? raw.source_type : (typeof metadata.source_type === 'string' ? metadata.source_type : 'vault');
  const rawSourceType = rawSt;
  const sourceType = normalizeSourceType(rawSourceType);

  const tagsRaw = (metadata.tags as string) ?? '';
  const tags = tagsRaw ? tagsRaw.split(',').filter(Boolean) : [];

  const headingRaw = (metadata.heading_path as string) ?? '';
  const headingPath = headingRaw ? headingRaw.split(' > ').filter(Boolean) : [];

  const modifiedAt = (metadata.modified_at as string) ?? '';
  const timestamp = modifiedAt ? new Date(modifiedAt).getTime() : 0;

  const clusterId = (metadata.cluster_id as number) ?? -1;
  const isYourNote = (metadata.is_user_note as boolean) ?? false;

  const rawDisposition = typeof raw.disposition === 'string'
    ? raw.disposition
    : typeof metadata.disposition === 'string'
      ? metadata.disposition
      : 'inbox';
  const disposition = (['signal', 'inbox', 'ignored'].includes(rawDisposition)
    ? rawDisposition
    : 'inbox') as Disposition;

  // Highlights are stored as a JSON string in Chroma metadata or as a parsed
  // array when returned directly from a Rust command.
  let highlights: HighlightRange[] = [];
  const rawHighlights = raw.highlights ?? metadata.highlights;
  if (typeof rawHighlights === 'string' && rawHighlights) {
    try { highlights = JSON.parse(rawHighlights); } catch { /* no-op */ }
  } else if (Array.isArray(rawHighlights)) {
    highlights = rawHighlights as HighlightRange[];
  }

  return {
    id: typeof raw.id === 'string' ? raw.id : '',
    content: typeof raw.content === 'string' ? raw.content : '',
    sourceType,
    sourceLabel: deriveSourceLabel(sourceType, metadata),
    tags,
    timestamp,
    isYourNote,
    clusterId,
    headingPath,
    disposition,
    highlights,
    metadata,
  };
}

// ── Fragment Page ──────────────────────────────────────────

export function transformFragmentPage(raw: Record<string, unknown>): FragmentPage {
  const fragments = Array.isArray(raw.fragments)
    ? (raw.fragments as Record<string, unknown>[]).map(transformFragment)
    : [];
  return {
    fragments,
    total: typeof raw.total === 'number' ? raw.total : 0,
    page: typeof raw.page === 'number' ? raw.page : 0,
    page_size: typeof raw.page_size === 'number' ? raw.page_size : 50,
  };
}

// ── Thread ──────────────────────────────────────────────────

function parseClusterIdFromString(s: string): number {
  const match = s.match(/cluster_(\d+)/);
  return match ? parseInt(match[1], 10) : -1;
}

export function transformThread(raw: Record<string, unknown>): ThreadView {
  const sourceCluster = typeof raw.source_cluster === 'string' ? raw.source_cluster : '';
  const targetCluster = typeof raw.target_cluster === 'string' ? raw.target_cluster : '';

  return {
    id: typeof raw.id === 'string' ? raw.id : '',
    label: typeof raw.label === 'string' ? raw.label : '',
    sourceClusterId: parseClusterIdFromString(sourceCluster),
    targetClusterId: parseClusterIdFromString(targetCluster),
    similarity: typeof raw.similarity === 'number' ? raw.similarity : 0,
  };
}

// ── Positions ───────────────────────────────────────────────

export function transformPositions(
  raw: Array<{ label: number; x: number; y: number }>
): Map<number, { x: number; y: number }> {
  const map = new Map<number, { x: number; y: number }>();
  for (const pos of raw) {
    map.set(pos.label, { x: pos.x, y: pos.y });
  }
  return map;
}

// ── Search Result ───────────────────────────────────────────

export function transformSearchResult(raw: Record<string, unknown>): SearchResult {
  const rawSourceType = typeof raw.source_type === 'string' ? raw.source_type : 'vault';
  return {
    id: typeof raw.id === 'string' ? raw.id : '',
    content: typeof raw.content === 'string' ? raw.content : '',
    sourceType: normalizeSourceType(rawSourceType),
    sourcePath: typeof raw.source_path === 'string' ? raw.source_path : '',
    distance: typeof raw.distance === 'number' ? raw.distance : Infinity,
    metadata: (typeof raw.metadata === 'object' && raw.metadata !== null ? raw.metadata : {}) as Record<string, unknown>,
  };
}

// ── Status ──────────────────────────────────────────────────

export function deriveStatusData(
  sidecarStatus: SidecarStatus | null,
  pipelineStats: PipelineStats | null,
  clusterCount: number,
  threadCount: number,
): StatusData {
  let chromaHealth: ServiceHealth = 'err';
  let embeddingHealth: ServiceHealth = 'err';

  if (sidecarStatus) {
    chromaHealth = sidecarStatus.chroma_running ? 'ok' : 'err';
    if (sidecarStatus.python_model_ready) {
      embeddingHealth = 'ok';
    } else if (sidecarStatus.python_running) {
      embeddingHealth = 'warn';
    } else {
      embeddingHealth = 'err';
    }
  }

  return {
    chromaHealth,
    embeddingHealth,
    fragmentCount: pipelineStats?.total_chunks ?? 0,
    clusterCount,
    threadCount,
  };
}

// ── Stream Item Conversion ──────────────────────────────────

const DAY_MS = 86400000;

export function fragmentToStreamItem(
  fragment: Fragment,
  clusters: ClusterView[],
): StreamItem {
  const cluster = clusters.find((c) => c.label === fragment.clusterId);
  const title = fragment.content.slice(0, 80).replace(/\n/g, ' ').trim();

  return {
    id: fragment.id,
    title: title || 'Untitled',
    sourceType: fragment.sourceType,
    sourceLabel: fragment.sourceLabel,
    clusterLabel: cluster?.displayLabel ?? 'Unclustered',
    clusterId: fragment.clusterId,
    isNew: Date.now() - fragment.timestamp < DAY_MS,
    timestamp: fragment.timestamp,
  };
}
