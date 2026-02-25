import type {
  ClusterView,
  Fragment,
  ThreadView,
  StreamItem,
  StatusData,
  SearchResult,
  SourceType,
  ServiceHealth,
} from '../types';

// ── Cluster ─────────────────────────────────────────────────

export function transformCluster(raw: Record<string, unknown>): ClusterView {
  return {
    label: (raw.label as number) ?? -1,
    displayLabel: (raw.display_label as string) ?? 'Unlabeled',
    size: (raw.size as number) ?? 0,
    pinned: (raw.pinned as boolean) ?? false,
    fragmentIds: (raw.fragment_ids as string[]) ?? [],
  };
}

// ── Fragment ────────────────────────────────────────────────

function normalizeSourceType(raw: string): SourceType {
  if (raw === 'podcasts') return 'podcast';
  if (raw === 'vault' || raw === 'twitter' || raw === 'readwise' || raw === 'podcast') {
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
  }
}

export function transformFragment(raw: Record<string, unknown>): Fragment {
  const metadata = (raw.metadata as Record<string, unknown>) ?? {};
  const rawSourceType = (raw.source_type as string) ?? (metadata.source_type as string) ?? 'vault';
  const sourceType = normalizeSourceType(rawSourceType);

  const tagsRaw = (metadata.tags as string) ?? '';
  const tags = tagsRaw ? tagsRaw.split(',').filter(Boolean) : [];

  const headingRaw = (metadata.heading_path as string) ?? '';
  const headingPath = headingRaw ? headingRaw.split(' > ').filter(Boolean) : [];

  const modifiedAt = (metadata.modified_at as string) ?? '';
  const timestamp = modifiedAt ? new Date(modifiedAt).getTime() : 0;

  const clusterId = (metadata.cluster_id as number) ?? -1;
  const isYourNote = (metadata.is_user_note as boolean) ?? false;

  return {
    id: (raw.id as string) ?? '',
    content: (raw.content as string) ?? '',
    sourceType,
    sourceLabel: deriveSourceLabel(sourceType, metadata),
    tags,
    timestamp,
    isYourNote,
    clusterId,
    headingPath,
  };
}

// ── Thread ──────────────────────────────────────────────────

function parseClusterIdFromString(s: string): number {
  const match = s.match(/cluster_(\d+)/);
  return match ? parseInt(match[1], 10) : -1;
}

export function transformThread(raw: Record<string, unknown>): ThreadView {
  const sourceCluster = (raw.source_cluster as string) ?? '';
  const targetCluster = (raw.target_cluster as string) ?? '';

  return {
    id: (raw.id as string) ?? '',
    label: (raw.label as string) ?? '',
    sourceClusterId: parseClusterIdFromString(sourceCluster),
    targetClusterId: parseClusterIdFromString(targetCluster),
    similarity: (raw.similarity as number) ?? 0,
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
  const rawSourceType = (raw.source_type as string) ?? 'vault';
  return {
    id: (raw.id as string) ?? '',
    content: (raw.content as string) ?? '',
    sourceType: normalizeSourceType(rawSourceType),
    sourcePath: (raw.source_path as string) ?? '',
    distance: (raw.distance as number) ?? Infinity,
    metadata: (raw.metadata as Record<string, unknown>) ?? {},
  };
}

// ── Status ──────────────────────────────────────────────────

interface SidecarStatus {
  chroma_running: boolean;
  python_running: boolean;
  python_model_ready: boolean;
}

interface PipelineStats {
  total_chunks: number;
}

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
