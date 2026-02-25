import { invoke } from '@tauri-apps/api/core';
import type { ClusterView, Fragment, ThreadView, SearchResult } from '../types';
import {
  transformCluster,
  transformFragment,
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
