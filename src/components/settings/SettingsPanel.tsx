import { useState, useEffect, useCallback } from 'react';
import { useAppStore } from '../../stores/appStore';
import { StatusBar } from '../StatusBar';
import { SourceDrawer } from './SourceDrawer';
import { AddSourceModal } from './AddSourceModal';
import {
  listSources,
  watcherGetVaultInfo,
  watcherIsActive,
  sidecarStatus,
  pipelineGetStats,
  pipelineIndexVault,
} from '../../api/commands';
import type { SidecarStatus, PipelineStats } from '../../api/commands';
import type { SourceConfig, VaultInfo } from '../../types';
import './settings.css';

const SOURCE_ICONS: Record<string, string> = {
  vault: '◆',
  rss: '◫',
  readwise: '◉',
  twitter: '𝕏',
  apple_notes: '✎',
  podcast: '♫',
};

function abbreviatePath(path: string): string {
  return path.replace(/^\/Users\/[^/]+/, '~');
}

function extractVaultName(path: string): string {
  return path.split('/').filter(Boolean).pop() ?? 'Vault';
}

function sourceDetail(source: SourceConfig): string {
  switch (source.source_type) {
    case 'vault':
      return `${abbreviatePath((source.config.path as string) ?? '')} · file watcher`;
    case 'rss': {
      const feeds = (source.config.feeds as string[]) ?? [];
      return `${feeds.length} feed${feeds.length !== 1 ? 's' : ''} · polling`;
    }
    case 'readwise':
      return 'API v2 · books, articles, podcasts';
    case 'twitter':
      return (source.config.username as string) ?? 'Twitter bookmarks';
    case 'apple_notes':
      return 'one-time import';
    default:
      return '';
  }
}

export function SettingsPanel() {
  const vaultPath = useAppStore((s) => s.vaultPath);

  const [sources, setSources] = useState<SourceConfig[]>([]);
  const [vaultInfo, setVaultInfo] = useState<VaultInfo | null>(null);
  const [watching, setWatching] = useState(false);
  const [sidecar, setSidecar] = useState<SidecarStatus | null>(null);
  const [stats, setStats] = useState<PipelineStats | null>(null);
  const [reindexing, setReindexing] = useState(false);
  const [autoCluster, setAutoCluster] = useState(false);
  const [onlySignal, setOnlySignal] = useState(true);

  // Drawer / modal
  const [drawerSource, setDrawerSource] = useState<SourceConfig | null>(null);
  const [showAddModal, setShowAddModal] = useState(false);

  const loadData = useCallback(async () => {
    const results = await Promise.all([
      listSources().catch(() => [] as SourceConfig[]),
      vaultPath ? watcherGetVaultInfo(vaultPath).catch(() => null) : Promise.resolve(null),
      watcherIsActive().catch(() => false),
      sidecarStatus().catch(() => null),
      pipelineGetStats().catch(() => null),
    ]);
    setSources(results[0] as SourceConfig[]);
    setVaultInfo(results[1] as VaultInfo | null);
    setWatching(results[2] as boolean);
    setSidecar(results[3] as SidecarStatus | null);
    setStats(results[4] as PipelineStats | null);
  }, [vaultPath]);

  useEffect(() => {
    loadData();
  }, [loadData]);

  const handleReindex = async () => {
    setReindexing(true);
    try {
      await pipelineIndexVault();
      const newStats = await pipelineGetStats().catch(() => null);
      setStats(newStats);
    } finally {
      setReindexing(false);
    }
  };

  const handleDrawerDone = () => {
    setDrawerSource(null);
    loadData();
  };

  const handleSourceAdded = () => {
    setShowAddModal(false);
    loadData();
  };

  return (
    <div className="settings-container">
      <div className="settings-header">
        <span className="settings-title">Settings</span>
      </div>

      <div className="settings-body">
        {/* ── Vault ──────────────────────────────────── */}
        <div className="settings-section">
          <div className="section-header">
            <span className="section-label">Vault</span>
          </div>

          {vaultPath ? (
            <div className="vault-card">
              <div className="vault-row">
                <div className="vault-icon">◆</div>
                <div className="vault-info">
                  <div className="vault-name">{extractVaultName(vaultPath)}</div>
                  <div className="vault-path">{abbreviatePath(vaultPath)}</div>
                </div>
                <div className={`vault-status${watching ? '' : ' inactive'}`}>
                  <span className="dot" />
                  {watching ? 'Watching' : 'Inactive'}
                </div>
              </div>
              {vaultInfo && (
                <div className="vault-meta">
                  <div className="vault-stat">
                    <strong>{vaultInfo.file_count}</strong> files
                  </div>
                  <div className="vault-stat">
                    <strong>{vaultInfo.folder_count}</strong> folders
                  </div>
                </div>
              )}
              <div className="filter-row">
                <select className="filter-select" disabled>
                  <option>/ (entire vault)</option>
                </select>
              </div>
              <div className="scope-hint">
                Filter scopes the stream and clustering to a subfolder. Use <strong>/</strong> for everything.
              </div>
            </div>
          ) : (
            <div className="vault-card">
              <div className="vault-path">No vault configured. Set a vault path in config.</div>
            </div>
          )}
        </div>

        {/* ── Sources ────────────────────────────────── */}
        <div className="settings-section">
          <div className="section-header">
            <span className="section-label">Sources</span>
            <button className="section-action" onClick={() => setShowAddModal(true)}>
              + Add Source
            </button>
          </div>

          {sources.length > 0 ? (
            <div className="source-list">
              {sources.map((s) => (
                <div
                  key={s.id}
                  className="source-item"
                  role="button"
                  tabIndex={0}
                  onClick={() => setDrawerSource(s)}
                  onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); setDrawerSource(s); } }}
                >
                  <div
                    className="source-icon"
                    style={s.source_type === 'twitter' ? { fontWeight: 700 } : undefined}
                  >
                    {SOURCE_ICONS[s.source_type] ?? '◆'}
                  </div>
                  <div className="source-info">
                    <div className="source-name">{s.display_name}</div>
                    <div className="source-detail">{sourceDetail(s)}</div>
                  </div>
                  <span className={`source-disposition ${s.default_disposition}`}>
                    {s.default_disposition === 'signal' ? 'Signal' : 'Inbox'}
                  </span>
                  <div className={`source-status ${s.enabled ? 'connected' : 'disconnected'}`} />
                </div>
              ))}
            </div>
          ) : (
            <div className="source-note">
              No sources configured yet. Click <strong>+ Add Source</strong> to get started.
            </div>
          )}

          <div className="source-note">
            <strong>Podcasts</strong> come through Readwise via Snipd. Triple-tap headphones
            while listening to capture moments.
          </div>
          <div className="source-note" style={{ marginTop: 6 }}>
            <strong>Substack</strong> articles arrive via RSS. Add the feed URL
            (publication.substack.com/feed) as an RSS source.
          </div>
        </div>

        {/* ── Embeddings & Clustering ────────────────── */}
        <div className="settings-section">
          <div className="section-header">
            <span className="section-label">Embeddings & Clustering</span>
          </div>

          <div className="embed-card">
            <div className="embed-row">
              <span className="embed-label">Model</span>
              <span className="embed-value">nomic-embed-text</span>
            </div>
            <div className="embed-row">
              <span className="embed-label">Chroma</span>
              <span
                className="embed-value"
                style={{ color: sidecar?.chroma_running ? 'var(--ok)' : 'var(--err)' }}
              >
                {sidecar?.chroma_running
                  ? `● Connected :${sidecar.chroma_port}`
                  : '● Disconnected'}
              </span>
            </div>
            <div className="embed-row">
              <span className="embed-label">Indexed</span>
              <span className="embed-value">
                {stats?.total_chunks ?? 0} fragments
              </span>
            </div>

            {stats?.collections && stats.collections.length > 0 && (
              <div className="collection-list">
                {stats.collections.map((c) => (
                  <div key={c.name} className="collection-row">
                    <span className="collection-name">{c.name}</span>
                    <span className="collection-count">{c.count}</span>
                  </div>
                ))}
              </div>
            )}

            <div className="embed-actions">
              <button
                className="btn-outline"
                onClick={handleReindex}
                disabled={reindexing}
              >
                {reindexing ? 'Re-indexing...' : 'Re-index All'}
              </button>
            </div>
          </div>
        </div>

        {/* ── Clustering ─────────────────────────────── */}
        <div className="settings-section">
          <div className="section-header">
            <span className="section-label">Clustering</span>
          </div>

          <div className="embed-card">
            <div className="embed-row">
              <span className="embed-label">Algorithm</span>
              <span className="embed-value">HDBSCAN</span>
            </div>
            <div className="embed-row">
              <span className="embed-label">Reduction</span>
              <span className="embed-value">UMAP → 2D</span>
            </div>
            <div className="toggle-row">
              <span className="toggle-row-label">Auto-cluster on ingest</span>
              <button
                className={`toggle${autoCluster ? ' on' : ''}`}
                onClick={() => setAutoCluster(!autoCluster)}
                role="switch"
                aria-checked={autoCluster}
                aria-label="Auto-cluster on ingest"
              />
            </div>
            <div className="toggle-row">
              <span className="toggle-row-label">Only cluster signal fragments</span>
              <button
                className={`toggle${onlySignal ? ' on' : ''}`}
                onClick={() => setOnlySignal(!onlySignal)}
                role="switch"
                aria-checked={onlySignal}
                aria-label="Only cluster signal fragments"
              />
            </div>
          </div>
        </div>
      </div>

      <StatusBar />

      {/* Drawer for editing a source */}
      {drawerSource && (
        <SourceDrawer
          source={drawerSource}
          onClose={() => setDrawerSource(null)}
          onSaved={handleDrawerDone}
          onRemoved={handleDrawerDone}
        />
      )}

      {/* Modal for adding a new source */}
      {showAddModal && (
        <AddSourceModal
          onClose={() => setShowAddModal(false)}
          onAdded={handleSourceAdded}
        />
      )}
    </div>
  );
}
