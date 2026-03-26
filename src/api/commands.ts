import { invoke } from '@tauri-apps/api/core';
import type {
  ClusterView,
  Disposition,
  DispositionCounts,
  Fragment,
  FragmentFilter,
  FragmentPage,
  HighlightRange,
  ThreadView,
  SearchResult,
  SourceConfig,
  VaultInfo,
  TestSourceResult,
  SyncSourceResult,
  AuthStartResult,
  TwitterConnectionInfo,
  TwitterSyncResult,
  ReadwiseImportResult,
  RssAddFeedResult,
  RssPollResult,
  RssCheckResult,
  AppleNotesScanResult,
} from '../types';
import {
  transformCluster,
  transformFragment,
  transformFragmentPage,
  transformThread,
  transformPositions,
  transformSearchResult,
} from './transforms';

// ── Sidecar ─────────────────────────────────────────────────

export interface SidecarStatus {
  chroma_running: boolean;
  chroma_port: number;
  python_running: boolean;
  python_port: number;
  python_model_ready: boolean;
}

export async function sidecarStartAll(): Promise<SidecarStatus> {
  return invoke<SidecarStatus>('sidecar_start_all');
}

export async function sidecarStatus(): Promise<SidecarStatus> {
  return invoke<SidecarStatus>('sidecar_status');
}

// ── Config ──────────────────────────────────────────────────

export interface AppConfig {
  chroma_port: number;
  sidecar_port: number;
  vault_path: string | null;
  readwise_api_key: string | null;
}

export async function configGet(): Promise<AppConfig> {
  return invoke<AppConfig>('config_get');
}

// ── Clustering ──────────────────────────────────────────────

export async function clusteringGetAll(): Promise<ClusterView[]> {
  const raw = await invoke<Record<string, unknown>[]>('clustering_get_all');
  return raw.map(transformCluster);
}

export async function clusteringRun(params?: {
  min_cluster_size?: number;
  min_samples?: number;
  collections?: string[];
}): Promise<ClusterView[]> {
  const raw = await invoke<Record<string, unknown>[]>('clustering_run', {
    params: params ?? {},
  });
  return raw.map(transformCluster);
}

export async function clusteringGetFragments(clusterId: number): Promise<Fragment[]> {
  const raw = await invoke<Record<string, unknown>[]>('clustering_get_fragments', {
    clusterId,
  });
  return raw.map(transformFragment);
}

export async function clusteringGetPositions(): Promise<Map<number, { x: number; y: number }>> {
  const raw = await invoke<Array<{ label: number; x: number; y: number }>>(
    'clustering_get_positions'
  );
  return transformPositions(raw);
}

// ── Clustering mutations ────────────────────────────────────

export async function clusteringMoveFragment(
  fragmentId: string, fromCluster: number, toCluster: number
): Promise<void> {
  return invoke('clustering_move_fragment', { fragmentId, fromCluster, toCluster });
}

export async function clusteringMerge(ids: number[]): Promise<ClusterView> {
  const raw = await invoke<Record<string, unknown>>('clustering_merge', { ids });
  return transformCluster(raw);
}

export async function clusteringRename(clusterId: number, label: string): Promise<void> {
  return invoke('clustering_rename', { clusterId, label });
}

// ── Threads ─────────────────────────────────────────────────

export async function threadsGetAll(): Promise<ThreadView[]> {
  const raw = await invoke<Record<string, unknown>[]>('threads_get_all');
  return raw.map(transformThread);
}

export async function threadsDetect(params?: {
  similarity_threshold?: number;
  max_threads?: number;
}): Promise<ThreadView[]> {
  const raw = await invoke<Record<string, unknown>[]>('threads_detect', {
    params: params ?? {},
  });
  return raw.map(transformThread);
}

export async function threadsName(id: string, label: string): Promise<void> {
  return invoke('threads_name', { id, label });
}

// ── Search ──────────────────────────────────────────────────

export async function searchAll(params: {
  query: string;
  n_results?: number;
  source_types?: string[];
  cluster_id?: number;
}): Promise<SearchResult[]> {
  const raw = await invoke<Record<string, unknown>[]>('search_all', {
    params,
  });
  return raw.map(transformSearchResult);
}

// ── Pipeline ────────────────────────────────────────────────

export interface PipelineStats {
  total_files_indexed: number;
  total_chunks: number;
  collections: Array<{ name: string; count: number }>;
  last_index_time: string | null;
}

export async function pipelineGetStats(): Promise<PipelineStats> {
  return invoke<PipelineStats>('pipeline_get_stats');
}

export async function pipelineGetRecent(limit?: number): Promise<Fragment[]> {
  const raw = await invoke<Record<string, unknown>[]>('pipeline_get_recent', {
    limit: limit ?? 50,
  });
  return raw.map(transformFragment);
}

export interface CreateNoteResult {
  id: string;
  cluster_id: number | null;
}

export async function pipelineCreateNote(params: {
  content: string;
  cluster_id?: number;
  tags?: string[];
}): Promise<CreateNoteResult> {
  return invoke<CreateNoteResult>('pipeline_create_note', { params });
}

export async function pipelineIndexFile(
  vaultPath: string,
  filePath: string,
): Promise<{ path: string; chunks_created: number; skipped: boolean }> {
  return invoke('pipeline_index_file', {
    vaultPath,
    filePath,
  });
}

// ── Watcher ─────────────────────────────────────────────────

export async function watcherStart(vaultPath: string): Promise<void> {
  return invoke('watcher_start', { vaultPath });
}

export async function watcherStop(): Promise<void> {
  return invoke('watcher_stop');
}

export async function watcherGetVaultInfo(vaultPath: string): Promise<VaultInfo> {
  return invoke<VaultInfo>('watcher_get_vault_info', { vaultPath });
}

export async function watcherIsActive(): Promise<boolean> {
  return invoke<boolean>('watcher_is_active');
}

// ── Source CRUD ──────────────────────────────────────────────

export async function listSources(): Promise<SourceConfig[]> {
  return invoke<SourceConfig[]>('list_sources');
}

export async function addSource(source: SourceConfig): Promise<void> {
  return invoke('add_source', { source });
}

export async function updateSource(source: SourceConfig): Promise<void> {
  return invoke('update_source', { source });
}

export async function removeSource(id: string): Promise<void> {
  return invoke('remove_source', { id });
}

// ── Source dispatchers ───────────────────────────────────────

export async function testSource(sourceId: string): Promise<TestSourceResult> {
  return invoke<TestSourceResult>('test_source', { sourceId });
}

export async function syncSource(sourceId: string): Promise<SyncSourceResult> {
  return invoke<SyncSourceResult>('sync_source', { sourceId });
}

// ── Twitter ──────────────────────────────────────────────────

export async function sourceTwitterAuthStart(clientId: string): Promise<AuthStartResult> {
  return invoke<AuthStartResult>('source_twitter_auth_start', { clientId });
}

export async function sourceTwitterCheckConnection(clientId?: string): Promise<TwitterConnectionInfo> {
  return invoke<TwitterConnectionInfo>('source_twitter_check_connection', { clientId: clientId ?? null });
}

export async function sourceTwitterSync(clientId: string): Promise<TwitterSyncResult> {
  return invoke<TwitterSyncResult>('source_twitter_sync', { clientId });
}

// ── Readwise ─────────────────────────────────────────────────

export async function sourceReadwiseConfigure(apiKey: string): Promise<void> {
  return invoke('source_readwise_configure', { apiKey });
}

export async function sourceReadwiseCheckConnection(): Promise<boolean> {
  return invoke<boolean>('source_readwise_check_connection');
}

export async function sourceReadwiseImport(): Promise<ReadwiseImportResult> {
  return invoke<ReadwiseImportResult>('source_readwise_import');
}

// ── RSS ──────────────────────────────────────────────────────

export async function sourceRssAddFeed(url: string): Promise<RssAddFeedResult> {
  return invoke<RssAddFeedResult>('source_rss_add_feed', { url });
}

export async function sourceRssPoll(sourceId: string): Promise<RssPollResult> {
  return invoke<RssPollResult>('source_rss_poll', { sourceId });
}

export async function sourceRssCheckConnection(sourceId: string): Promise<RssCheckResult> {
  return invoke<RssCheckResult>('source_rss_check_connection', { sourceId });
}

// ── Apple Notes ──────────────────────────────────────────────

export async function sourceAppleNotesScan(path: string): Promise<AppleNotesScanResult> {
  return invoke<AppleNotesScanResult>('source_apple_notes_scan', { path });
}

export async function sourceAppleNotesCheck(path: string): Promise<AppleNotesScanResult> {
  return invoke<AppleNotesScanResult>('source_apple_notes_check', { path });
}

// ── Pipeline (additional) ────────────────────────────────────

export async function pipelineIndexVault(): Promise<void> {
  return invoke('pipeline_index_vault');
}

// ── Fragment Triage ──────────────────────────────────────

export async function listFragments(filter: FragmentFilter): Promise<FragmentPage> {
  const raw = await invoke<Record<string, unknown>>('list_fragments', { filter });
  return transformFragmentPage(raw);
}

export async function getFragment(id: string): Promise<Fragment> {
  const raw = await invoke<Record<string, unknown>>('get_fragment', { id });
  return transformFragment(raw);
}

export async function setDisposition(id: string, disposition: Disposition): Promise<Fragment> {
  const raw = await invoke<Record<string, unknown>>('set_disposition', { id, disposition });
  return transformFragment(raw);
}

export async function setHighlights(id: string, highlights: HighlightRange[]): Promise<Fragment> {
  const raw = await invoke<Record<string, unknown>>('set_highlights', { id, highlights });
  return transformFragment(raw);
}

export async function getDispositionCounts(): Promise<DispositionCounts> {
  return invoke<DispositionCounts>('get_disposition_counts');
}
