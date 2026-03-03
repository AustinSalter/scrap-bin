export type SourceType = 'vault' | 'twitter' | 'readwise' | 'podcast' | 'rss' | 'apple_notes';

export type Disposition = 'signal' | 'inbox' | 'ignored';

export interface SourceConfig {
  id: string;
  source_type: SourceType;
  display_name: string;
  config: Record<string, unknown>;
  default_disposition: Disposition;
  sync_schedule: string | null;
  enabled: boolean;
  vault_subfolder: string | null;
}

export interface VaultInfo {
  path: string;
  file_count: number;
  folder_count: number;
  is_watching: boolean;
}

export interface TestSourceResult {
  success: boolean;
  message: string;
}

export interface SyncSourceResult {
  success: boolean;
  message: string;
  fragments_imported: number;
}

export interface AuthStartResult {
  auth_url: string;
  state: string;
}

export interface TwitterConnectionInfo {
  user_id: string | null;
  username: string | null;
  connected: boolean;
}

export interface TwitterSyncResult {
  imported: number;
  skipped: number;
  threads_detected: number;
  errors: string[];
}

export interface ReadwiseImportResult {
  imported: number;
  total_fetched: number;
}

export interface RssAddFeedResult {
  source_id: string;
  feed_title: string;
  feed_url: string;
}

export interface RssPollResult {
  imported: number;
  skipped: number;
  entries_fetched: number;
  feed_title: string;
}

export interface RssCheckResult {
  reachable: boolean;
  feed_title: string | null;
  entry_count: number;
}

export interface AppleNotesScanResult {
  imported: number;
  files_scanned: number;
  errors: string[];
}

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
