export type SourceType = 'vault' | 'twitter' | 'readwise' | 'podcast';

export interface Fragment {
  id: string;
  content: string;
  sourceType: SourceType;
  sourceLabel: string;
  tags: string[];
  timestamp: number;
  isYourNote: boolean;
  clusterId: number;
  headingPath: string[];
}

export interface ClusterView {
  label: number;
  displayLabel: string;
  size: number;
  pinned: boolean;
  fragmentIds: string[];
}

export interface StreamItem {
  id: string;
  title: string;
  sourceType: SourceType;
  sourceLabel: string;
  clusterLabel: string;
  clusterId: number;
  isNew: boolean;
  timestamp: number;
}

export interface ThreadView {
  id: string;
  label: string;
  sourceClusterId: number;
  targetClusterId: number;
  similarity: number;
}

export interface SearchResult {
  id: string;
  content: string;
  sourceType: SourceType;
  sourcePath: string;
  distance: number;
  metadata: Record<string, unknown>;
}

export interface FileChangeEvent {
  event_type: 'Created' | 'Modified' | 'Deleted';
  path: string;
  absolute_path: string;
  file_hash: string | null;
  timestamp: string;
}

export type ServiceHealth = 'ok' | 'warn' | 'err';

export interface StatusData {
  chromaHealth: ServiceHealth;
  embeddingHealth: ServiceHealth;
  fragmentCount: number;
  clusterCount: number;
  threadCount: number;
}
