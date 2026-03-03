import { useState, useEffect } from 'react';
import { open } from '@tauri-apps/plugin-shell';
import {
  updateSource,
  removeSource,
  syncSource,
  sourceTwitterCheckConnection,
  sourceTwitterAuthStart,
} from '../../api/commands';
import type { SourceConfig, Disposition, TwitterConnectionInfo } from '../../types';
import './source-drawer.css';

interface SourceDrawerProps {
  source: SourceConfig;
  onClose: () => void;
  onSaved: () => void;
  onRemoved: () => void;
}

const SCHEDULE_OPTIONS = [
  { value: 'hourly', label: 'Every hour' },
  { value: '6h', label: 'Every 6 hours' },
  { value: '12h', label: 'Every 12 hours' },
  { value: 'daily', label: 'Daily' },
  { value: '', label: 'Manual only' },
];

export function SourceDrawer({ source, onClose, onSaved, onRemoved }: SourceDrawerProps) {
  const [enabled, setEnabled] = useState(source.enabled);
  const [disposition, setDisposition] = useState<Disposition>(source.default_disposition);
  const [schedule, setSchedule] = useState(source.sync_schedule ?? '');
  const [vaultSubfolder, setVaultSubfolder] = useState(source.vault_subfolder ?? '');
  const [syncing, setSyncing] = useState(false);
  const [syncMessage, setSyncMessage] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [removing, setRemoving] = useState(false);
  const [confirmRemove, setConfirmRemove] = useState(false);

  // Twitter-specific
  const [twitterInfo, setTwitterInfo] = useState<TwitterConnectionInfo | null>(null);

  const handleSave = async () => {
    setSaving(true);
    try {
      const updated: SourceConfig = {
        ...source,
        enabled,
        default_disposition: disposition,
        sync_schedule: schedule || null,
        vault_subfolder: vaultSubfolder || null,
      };
      await updateSource(updated);
      onSaved();
    } finally {
      setSaving(false);
    }
  };

  const handleRemove = async () => {
    if (!confirmRemove) {
      setConfirmRemove(true);
      return;
    }
    setRemoving(true);
    try {
      await removeSource(source.id);
      onRemoved();
    } finally {
      setRemoving(false);
    }
  };

  const handleSync = async () => {
    setSyncing(true);
    setSyncMessage(null);
    try {
      const result = await syncSource(source.id);
      setSyncMessage(result.message);
    } catch (e) {
      setSyncMessage(`Sync failed: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setSyncing(false);
    }
  };

  const handleCheckTwitter = async () => {
    try {
      const clientId = (source.config.client_id as string) ?? '';
      const info = await sourceTwitterCheckConnection(clientId || undefined);
      setTwitterInfo(info);
    } catch {
      // ignore
    }
  };

  const handleReauthorizeTwitter = async () => {
    try {
      const clientId = (source.config.client_id as string) ?? '';
      const result = await sourceTwitterAuthStart(clientId);
      await open(result.auth_url);
    } catch {
      // ignore
    }
  };

  // Load Twitter info on mount for twitter sources
  useEffect(() => {
    if (source.source_type === 'twitter') {
      handleCheckTwitter();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [source.id]);

  return (
    <>
      <div
        className="drawer-overlay open"
        onClick={onClose}
        onKeyDown={(e) => { if (e.key === 'Escape') onClose(); }}
        role="presentation"
      />
      <div className="drawer open">
        <div className="drawer-header">
          <span className="drawer-title">{source.display_name}</span>
          <button className="drawer-close" onClick={onClose}>✕</button>
        </div>

        <div className="drawer-body">
          {/* ── Enabled toggle ──────────────────────── */}
          <div className="field">
            <div className="field-label">Connection</div>
            <div className="field-row">
              <span className="toggle-row-label">Enabled</span>
              <button
                className={`toggle${enabled ? ' on' : ''}`}
                onClick={() => setEnabled(!enabled)}
                role="switch"
                aria-checked={enabled}
                aria-label="Enabled"
              />
            </div>
          </div>

          {/* ── Source-specific fields ──────────────── */}
          {source.source_type === 'readwise' && (
            <>
              <div className="field">
                <div className="field-label">API Token</div>
                <input
                  className="field-input"
                  type="password"
                  value={(source.config.api_key as string) ?? ''}
                  readOnly
                />
                <div className="field-hint">Get your token at readwise.io/access_token</div>
              </div>
              <div className="field">
                <div className="field-label">Syncing Content</div>
                <div className="readwise-feeds">
                  <div className="readwise-feed-item">
                    <span className="readwise-feed-label">Books</span>
                    <span>Kindle highlights</span>
                  </div>
                  <div className="readwise-feed-item">
                    <span className="readwise-feed-label">Articles</span>
                    <span>Reader highlights</span>
                  </div>
                  <div className="readwise-feed-item">
                    <span className="readwise-feed-label">Podcasts</span>
                    <span>via Snipd</span>
                  </div>
                </div>
                <div className="field-hint" style={{ marginTop: 8 }}>
                  Podcast highlights arrive from Snipd. Triple-tap headphones while listening.
                </div>
              </div>
            </>
          )}

          {source.source_type === 'twitter' && (
            <div className="field">
              <div className="field-label">Connection</div>
              {twitterInfo?.connected ? (
                <div className="field-row">
                  <span className="toggle-row-label">
                    @{twitterInfo.username ?? 'unknown'}
                  </span>
                  <button className="btn-sync" onClick={handleReauthorizeTwitter}>
                    Reauthorize
                  </button>
                </div>
              ) : (
                <div className="field-hint">Not connected. Reauthorize to sync.</div>
              )}
            </div>
          )}

          {source.source_type === 'rss' && (
            <div className="field">
              <div className="field-label">Feeds</div>
              {Array.isArray(source.config.feeds) && (source.config.feeds as string[]).length > 0 ? (
                <div className="readwise-feeds">
                  {(source.config.feeds as string[]).map((url) => (
                    <div key={url} className="readwise-feed-item">
                      <span className="readwise-feed-label" style={{ fontFamily: "'Cousine', monospace", fontSize: 12 }}>{url}</span>
                    </div>
                  ))}
                </div>
              ) : (
                <input
                  className="field-input"
                  value={(source.config.url as string) ?? ''}
                  readOnly
                />
              )}
            </div>
          )}

          {source.source_type === 'apple_notes' && (
            <div className="field">
              <div className="field-label">Directory Path</div>
              <input
                className="field-input"
                value={(source.config.path as string) ?? ''}
                readOnly
              />
            </div>
          )}

          {source.source_type === 'vault' && (
            <div className="field">
              <div className="field-label">Path</div>
              <input
                className="field-input"
                value={(source.config.path as string) ?? ''}
                readOnly
              />
            </div>
          )}

          {/* ── Vault Destination ──────────────────── */}
          <div className="field">
            <div className="field-label">Vault Destination</div>
            <input
              className="field-input"
              value={vaultSubfolder}
              onChange={(e) => setVaultSubfolder(e.target.value)}
              placeholder="/sources/"
            />
          </div>

          {/* ── Disposition ────────────────────────── */}
          <div className="field">
            <div className="field-label">Default Disposition</div>
            <div className="disposition-picker">
              <button
                className={`disposition-option${disposition === 'signal' ? ' selected signal' : ''}`}
                onClick={() => setDisposition('signal')}
              >
                <div className="disposition-option-label">Signal</div>
                <div className="disposition-option-hint">Auto-cluster</div>
              </button>
              <button
                className={`disposition-option${disposition === 'inbox' ? ' selected inbox' : ''}`}
                onClick={() => setDisposition('inbox')}
              >
                <div className="disposition-option-label">Inbox</div>
                <div className="disposition-option-hint">Needs triage</div>
              </button>
            </div>
          </div>

          {/* ── Sync Schedule ──────────────────────── */}
          <div className="field">
            <div className="field-label">Sync Schedule</div>
            <select
              className="field-select"
              value={schedule}
              onChange={(e) => setSchedule(e.target.value)}
            >
              {SCHEDULE_OPTIONS.map((opt) => (
                <option key={opt.label} value={opt.value}>
                  {opt.label}
                </option>
              ))}
            </select>
          </div>

          {/* ── Sync Now ───────────────────────────── */}
          <div className="field">
            <div className="field-label">Sync</div>
            <div className="sync-row">
              <span className="sync-info">
                {syncMessage ?? 'Ready to sync'}
              </span>
              <button
                className="btn-sync"
                onClick={handleSync}
                disabled={syncing}
              >
                {syncing ? 'Syncing...' : 'Sync Now'}
              </button>
            </div>
          </div>
        </div>

        <div className="drawer-footer">
          <button
            className={`btn-secondary${confirmRemove ? ' confirm-remove' : ''}`}
            onClick={handleRemove}
            onBlur={() => setConfirmRemove(false)}
            disabled={removing}
          >
            {removing ? 'Removing...' : confirmRemove ? 'Confirm Disconnect' : 'Disconnect'}
          </button>
          <button
            className="btn-primary"
            onClick={handleSave}
            disabled={saving}
          >
            {saving ? 'Saving...' : 'Save'}
          </button>
        </div>
      </div>
    </>
  );
}
