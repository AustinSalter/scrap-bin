import { useState } from 'react';
import { open } from '@tauri-apps/plugin-shell';
import {
  addSource,
  sourceRssAddFeed,
  sourceReadwiseConfigure,
  sourceReadwiseCheckConnection,
  sourceTwitterAuthStart,
  sourceAppleNotesCheck,
} from '../../api/commands';
import type { SourceConfig, SourceType, Disposition } from '../../types';
import './add-source-modal.css';

interface AddSourceModalProps {
  onClose: () => void;
  onAdded: () => void;
}

type SourceChoice = 'vault' | 'rss' | 'readwise' | 'twitter' | 'apple_notes';

interface FeedEntry {
  id: string;
  title: string;
  url: string;
}

export function AddSourceModal({ onClose, onAdded }: AddSourceModalProps) {
  const [step, setStep] = useState<1 | 2>(1);
  const [selected, setSelected] = useState<SourceChoice | null>(null);

  // Step 2 shared state
  const [disposition, setDisposition] = useState<Disposition>('inbox');
  const [vaultSubfolder, setVaultSubfolder] = useState('/sources/');
  const [schedule, setSchedule] = useState('6h');
  const [connecting, setConnecting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // RSS
  const [feeds, setFeeds] = useState<FeedEntry[]>([]);
  const [feedUrl, setFeedUrl] = useState('');
  const [addingFeed, setAddingFeed] = useState(false);

  // Readwise
  const [apiKey, setApiKey] = useState('');
  const [readwiseChecked, setReadwiseChecked] = useState<boolean | null>(null);

  // Twitter
  const [twitterClientId, setTwitterClientId] = useState('');
  const [twitterAuthed, setTwitterAuthed] = useState(false);

  // Apple Notes
  const [notesPath, setNotesPath] = useState('');
  const [notesChecked, setNotesChecked] = useState<boolean | null>(null);

  // Local folder
  const [folderPath, setFolderPath] = useState('');

  const handleNext = () => {
    if (selected) {
      // Set sensible defaults per source type
      if (selected === 'rss') {
        setDisposition('inbox');
        setVaultSubfolder('/sources/rss/');
      } else if (selected === 'readwise') {
        setDisposition('signal');
        setVaultSubfolder('/sources/readwise/');
      } else if (selected === 'twitter') {
        setDisposition('signal');
        setVaultSubfolder('/sources/twitter/');
      } else if (selected === 'apple_notes') {
        setDisposition('signal');
        setVaultSubfolder('/sources/apple-notes/');
      } else if (selected === 'vault') {
        setDisposition('signal');
        setVaultSubfolder('');
      }
      setStep(2);
    }
  };

  const handleAddFeed = async () => {
    if (!feedUrl.trim()) return;
    setAddingFeed(true);
    setError(null);
    try {
      const result = await sourceRssAddFeed(feedUrl.trim());
      setFeeds((prev) => [
        ...prev,
        { id: result.source_id, title: result.feed_title, url: result.feed_url },
      ]);
      setFeedUrl('');
    } catch (e) {
      setError(`Failed to add feed: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setAddingFeed(false);
    }
  };

  const handleRemoveFeed = (id: string) => {
    setFeeds((prev) => prev.filter((f) => f.id !== id));
  };

  const handleTestReadwise = async () => {
    setError(null);
    try {
      await sourceReadwiseConfigure(apiKey);
      const ok = await sourceReadwiseCheckConnection();
      setReadwiseChecked(ok);
      if (!ok) setError('Readwise connection test failed. Check your API token.');
    } catch (e) {
      setReadwiseChecked(false);
      setError(`Readwise test failed: ${e instanceof Error ? e.message : String(e)}`);
    }
  };

  const handleTwitterAuth = async () => {
    setError(null);
    try {
      const result = await sourceTwitterAuthStart(twitterClientId);
      await open(result.auth_url);
      setTwitterAuthed(true);
    } catch (e) {
      setError(`Twitter auth failed: ${e instanceof Error ? e.message : String(e)}`);
    }
  };

  const handleCheckAppleNotes = async () => {
    setError(null);
    try {
      const result = await sourceAppleNotesCheck(notesPath);
      setNotesChecked(result.files_scanned > 0);
      if (result.files_scanned === 0) {
        setError('No Apple Notes files found at that path.');
      }
    } catch (e) {
      setNotesChecked(false);
      setError(`Check failed: ${e instanceof Error ? e.message : String(e)}`);
    }
  };

  const canConnect = (): boolean => {
    if (!selected) return false;
    switch (selected) {
      case 'rss': return feeds.length > 0;
      case 'readwise': return readwiseChecked === true;
      case 'twitter': return twitterAuthed;
      case 'apple_notes': return notesChecked === true;
      case 'vault': return folderPath.trim().length > 0;
    }
  };

  const handleConnect = async () => {
    if (!selected || !canConnect()) return;
    setConnecting(true);
    setError(null);

    try {
      const id = crypto.randomUUID();
      const sourceType: SourceType = selected;

      let config: Record<string, unknown> = {};
      let displayName = '';

      switch (selected) {
        case 'rss':
          config = { feeds: feeds.map((f) => f.url) };
          displayName = feeds.length === 1 ? feeds[0].title : `RSS Feeds (${feeds.length})`;
          break;
        case 'readwise':
          config = { api_key: apiKey };
          displayName = 'Readwise';
          break;
        case 'twitter':
          config = { client_id: twitterClientId };
          displayName = 'Twitter Bookmarks';
          break;
        case 'apple_notes':
          config = { path: notesPath };
          displayName = 'Apple Notes';
          break;
        case 'vault':
          config = { path: folderPath };
          displayName = folderPath.split('/').filter(Boolean).pop() ?? 'Local Folder';
          break;
      }

      const source: SourceConfig = {
        id,
        source_type: sourceType,
        display_name: displayName,
        config,
        default_disposition: disposition,
        sync_schedule: schedule || null,
        enabled: true,
        vault_subfolder: vaultSubfolder || null,
      };

      await addSource(source);
      onAdded();
    } catch (e) {
      setError(`Failed to add source: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setConnecting(false);
    }
  };

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <span className="modal-title">Add Source</span>
          <button className="modal-close" onClick={onClose}>✕</button>
        </div>

        <div className="modal-body">
          {step === 1 ? (
            /* ── Step 1: Source type selection ────── */
            <>
              <div className="tier-label">Configure in-app</div>
              <div className="tier-hint">These connect directly — no external tools needed.</div>
              <div className="source-list-modal">
                <button
                  className={`source-option${selected === 'vault' ? ' selected' : ''}`}
                  onClick={() => setSelected('vault')}
                >
                  <div className="source-option-icon">◆</div>
                  <div className="source-option-info">
                    <div className="source-option-name">Local Folder</div>
                    <div className="source-option-method">File watcher on any directory of .md files</div>
                  </div>
                  <span className="source-option-tag">ready</span>
                </button>
                <button
                  className={`source-option${selected === 'rss' ? ' selected' : ''}`}
                  onClick={() => setSelected('rss')}
                >
                  <div className="source-option-icon">◫</div>
                  <div className="source-option-info">
                    <div className="source-option-name">RSS Feeds</div>
                    <div className="source-option-method">URL polling · works for Substack too</div>
                  </div>
                  <span className="source-option-tag">ready</span>
                </button>
              </div>

              <div className="tier-label">Requires credentials</div>
              <div className="tier-hint">Paste an API key or token to connect.</div>
              <div className="source-list-modal">
                <button
                  className={`source-option${selected === 'readwise' ? ' selected' : ''}`}
                  onClick={() => setSelected('readwise')}
                >
                  <div className="source-option-icon">◉</div>
                  <div className="source-option-info">
                    <div className="source-option-name">Readwise</div>
                    <div className="source-option-method">API token · books, articles, Snipd podcasts</div>
                  </div>
                  <span className="source-option-tag needs-key">api token</span>
                </button>
                <button
                  className={`source-option${selected === 'twitter' ? ' selected' : ''}`}
                  onClick={() => setSelected('twitter')}
                >
                  <div className="source-option-icon" style={{ fontWeight: 700 }}>𝕏</div>
                  <div className="source-option-info">
                    <div className="source-option-name">Twitter Bookmarks</div>
                    <div className="source-option-method">OAuth · requires Twitter dev credentials</div>
                  </div>
                  <span className="source-option-tag needs-key">dev account</span>
                </button>
              </div>

              <div className="tier-label">Manual import</div>
              <div className="tier-hint">One-time setup outside the app, then the file watcher takes over.</div>
              <div className="source-list-modal">
                <button
                  className={`source-option${selected === 'apple_notes' ? ' selected' : ''}`}
                  onClick={() => setSelected('apple_notes')}
                >
                  <div className="source-option-icon">✎</div>
                  <div className="source-option-info">
                    <div className="source-option-name">Apple Notes</div>
                    <div className="source-option-method">Export via Obsidian Importer plugin, then watch folder</div>
                  </div>
                  <span className="source-option-tag manual">manual</span>
                </button>
              </div>
            </>
          ) : (
            /* ── Step 2: Source-specific config ───── */
            <div className="config-section">
              <div className="tier-label">
                Configure {selected === 'rss' ? 'RSS Feeds' :
                  selected === 'readwise' ? 'Readwise' :
                  selected === 'twitter' ? 'Twitter Bookmarks' :
                  selected === 'apple_notes' ? 'Apple Notes' :
                  'Local Folder'}
              </div>

              {error && (
                <div className="test-row error">
                  <div className="test-icon">!</div>
                  <div className="test-text">{error}</div>
                </div>
              )}

              {/* RSS Config */}
              {selected === 'rss' && (
                <>
                  <div className="field">
                    <div className="field-label">Feeds</div>
                    {feeds.length > 0 && (
                      <div className="feed-list">
                        {feeds.map((f) => (
                          <div key={f.id} className="feed-item">
                            <span className="feed-name">{f.title}</span>
                            <span className="feed-url">{f.url}</span>
                            <button
                              className="feed-remove"
                              onClick={() => handleRemoveFeed(f.id)}
                            >
                              ✕
                            </button>
                          </div>
                        ))}
                      </div>
                    )}
                    <div className="add-feed-row">
                      <input
                        className="add-feed-input"
                        placeholder="https://example.com/feed or substack URL"
                        value={feedUrl}
                        onChange={(e) => setFeedUrl(e.target.value)}
                        onKeyDown={(e) => e.key === 'Enter' && handleAddFeed()}
                      />
                      <button
                        className="add-feed-btn"
                        onClick={handleAddFeed}
                        disabled={addingFeed || !feedUrl.trim()}
                      >
                        {addingFeed ? 'Adding...' : '+ Add'}
                      </button>
                    </div>
                    <div className="field-hint">Paste a Substack URL and we'll resolve the feed automatically.</div>
                  </div>
                </>
              )}

              {/* Readwise Config */}
              {selected === 'readwise' && (
                <>
                  <div className="field">
                    <div className="field-label">API Token</div>
                    <div className="add-feed-row">
                      <input
                        className="add-feed-input"
                        type="password"
                        placeholder="Paste your Readwise API token"
                        value={apiKey}
                        onChange={(e) => setApiKey(e.target.value)}
                      />
                      <button
                        className="add-feed-btn"
                        onClick={handleTestReadwise}
                        disabled={!apiKey.trim()}
                      >
                        Test
                      </button>
                    </div>
                    <div className="field-hint">Get your token at readwise.io/access_token</div>
                  </div>
                  {readwiseChecked === true && (
                    <div className="test-row">
                      <div className="test-icon">✓</div>
                      <div className="test-text">Connected to Readwise</div>
                    </div>
                  )}
                </>
              )}

              {/* Twitter Config */}
              {selected === 'twitter' && (
                <>
                  <div className="field">
                    <div className="field-label">Twitter Client ID</div>
                    <input
                      className="field-input"
                      placeholder="Your Twitter API client ID"
                      value={twitterClientId}
                      onChange={(e) => setTwitterClientId(e.target.value)}
                    />
                  </div>
                  <div className="field">
                    <button
                      className="btn-connect"
                      style={{ width: '100%' }}
                      onClick={handleTwitterAuth}
                      disabled={!twitterClientId.trim()}
                    >
                      Authorize with Twitter
                    </button>
                  </div>
                  {twitterAuthed && (
                    <div className="test-row">
                      <div className="test-icon">✓</div>
                      <div className="test-text">Authorization started — complete in browser</div>
                    </div>
                  )}
                </>
              )}

              {/* Apple Notes Config */}
              {selected === 'apple_notes' && (
                <>
                  <div className="field">
                    <div className="field-label">Setup Instructions</div>
                    <div className="instruction-block">
                      <div className="instruction-step">
                        <div className="instruction-num">1</div>
                        <div>Install the <strong>Obsidian Importer</strong> community plugin</div>
                      </div>
                      <div className="instruction-step">
                        <div className="instruction-num">2</div>
                        <div>Open Importer → select <strong>Apple Notes</strong> as source</div>
                      </div>
                      <div className="instruction-step">
                        <div className="instruction-num">3</div>
                        <div>Run the import into your vault subfolder</div>
                      </div>
                      <div className="instruction-step">
                        <div className="instruction-num">4</div>
                        <div>Enter the export path below so Scrapbin can watch it</div>
                      </div>
                    </div>
                  </div>
                  <div className="field">
                    <div className="field-label">Export Directory</div>
                    <div className="add-feed-row">
                      <input
                        className="add-feed-input"
                        placeholder="/path/to/exported/apple-notes"
                        value={notesPath}
                        onChange={(e) => setNotesPath(e.target.value)}
                      />
                      <button
                        className="add-feed-btn"
                        onClick={handleCheckAppleNotes}
                        disabled={!notesPath.trim()}
                      >
                        Check
                      </button>
                    </div>
                  </div>
                  {notesChecked === true && (
                    <div className="test-row">
                      <div className="test-icon">✓</div>
                      <div className="test-text">Found Apple Notes files</div>
                    </div>
                  )}
                </>
              )}

              {/* Local Folder Config */}
              {selected === 'vault' && (
                <div className="field">
                  <div className="field-label">Folder Path</div>
                  <input
                    className="field-input"
                    placeholder="/path/to/folder"
                    value={folderPath}
                    onChange={(e) => setFolderPath(e.target.value)}
                  />
                  <div className="field-hint">Point to any directory containing .md files.</div>
                </div>
              )}

              {/* ── Shared fields ──────────────────── */}
              <div className="field">
                <div className="field-label">Vault Destination</div>
                <input
                  className="field-input"
                  value={vaultSubfolder}
                  onChange={(e) => setVaultSubfolder(e.target.value)}
                  placeholder="/sources/"
                />
              </div>

              {selected !== 'vault' && (
                <div className="field">
                  <div className="field-label">Sync Frequency</div>
                  <select
                    className="field-select"
                    value={schedule}
                    onChange={(e) => setSchedule(e.target.value)}
                  >
                    <option value="hourly">Every hour</option>
                    <option value="6h">Every 6 hours</option>
                    <option value="12h">Every 12 hours</option>
                    <option value="daily">Daily</option>
                    <option value="">Manual only</option>
                  </select>
                </div>
              )}

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
            </div>
          )}
        </div>

        <div className="modal-footer">
          <div className="step-indicator">
            <div className={`step-dot${step === 1 ? ' active' : ' done'}`} />
            <div className={`step-dot${step === 2 ? ' active' : ''}`} />
          </div>
          <div className="footer-actions">
            {step === 2 && (
              <button className="btn-back" onClick={() => setStep(1)}>
                Back
              </button>
            )}
            {step === 1 ? (
              <button
                className="btn-connect"
                onClick={handleNext}
                disabled={!selected}
              >
                Next
              </button>
            ) : (
              <button
                className="btn-connect"
                onClick={handleConnect}
                disabled={!canConnect() || connecting}
              >
                {connecting ? 'Connecting...' : 'Connect Source'}
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
