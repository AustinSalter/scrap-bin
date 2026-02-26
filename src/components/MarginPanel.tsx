import { useState, useRef, useEffect } from 'react';
import { useAppStore } from '../stores/appStore';
import { FragmentItem } from './FragmentItem';
import '../styles/margin.css';

export function MarginPanel() {
  const marginOpen = useAppStore((s) => s.marginOpen);
  const clearSelection = useAppStore((s) => s.clearSelection);
  const selectedClusterId = useAppStore((s) => s.selectedClusterId);
  const clusters = useAppStore((s) => s.clusters);
  const fragments = useAppStore((s) => s.selectedClusterFragments);
  const saveNote = useAppStore((s) => s.saveNote);
  const loading = useAppStore((s) => s.loading);
  const editingClusterId = useAppStore((s) => s.editingClusterId);
  const setEditingCluster = useAppStore((s) => s.setEditingCluster);
  const renameCluster = useAppStore((s) => s.renameCluster);

  const [noteText, setNoteText] = useState('');

  const cluster = clusters.find((c) => c.label === selectedClusterId);
  const displayLabel = cluster?.displayLabel ?? 'Cluster';

  const isEditingName = editingClusterId === selectedClusterId && selectedClusterId !== null;
  const [renameValue, setRenameValue] = useState(displayLabel);
  const renameInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    setRenameValue(displayLabel);
  }, [displayLabel]);

  useEffect(() => {
    if (isEditingName && renameInputRef.current) {
      renameInputRef.current.focus();
      renameInputRef.current.select();
    }
  }, [isEditingName]);

  const sourceCount = new Set(fragments.map((f) => f.sourceLabel)).size;
  const noteCount = fragments.filter((f) => f.isYourNote).length;

  const handleSave = async () => {
    if (!noteText.trim() || loading.saveNote) return;
    await saveNote(noteText);
    setNoteText('');
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && e.metaKey) {
      e.preventDefault();
      handleSave();
    }
  };

  const handleRenameKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      const trimmed = renameValue.trim();
      if (trimmed && trimmed !== displayLabel && selectedClusterId !== null) {
        renameCluster(selectedClusterId, trimmed);
      } else {
        setEditingCluster(null);
      }
    } else if (e.key === 'Escape') {
      setRenameValue(displayLabel);
      setEditingCluster(null);
    }
  };

  const handleRenameBlur = () => {
    const trimmed = renameValue.trim();
    if (trimmed && trimmed !== displayLabel && selectedClusterId !== null) {
      renameCluster(selectedClusterId, trimmed);
    } else {
      setEditingCluster(null);
    }
  };

  return (
    <aside className={`margin${marginOpen ? ' is-open' : ''}`}>
      <div className="margin-header">
        <div className="margin-header-info">
          {isEditingName ? (
            <input
              ref={renameInputRef}
              className="margin-cluster-name-input"
              value={renameValue}
              onChange={(e) => setRenameValue(e.target.value)}
              onKeyDown={handleRenameKeyDown}
              onBlur={handleRenameBlur}
            />
          ) : (
            <div
              className="margin-cluster-name"
              onDoubleClick={() => {
                if (selectedClusterId !== null) setEditingCluster(selectedClusterId);
              }}
            >
              {displayLabel}
            </div>
          )}
          <div className="margin-cluster-meta">
            {fragments.length} fragments &middot; {sourceCount} sources &middot; {noteCount} notes
          </div>
        </div>
        <button className="margin-close drag-region-exempt" onClick={clearSelection} aria-label="Close">
          &times;
        </button>
      </div>

      <div className="margin-body">
        {loading.fragments ? (
          <div className="margin-empty">Loading fragments…</div>
        ) : fragments.length === 0 ? (
          <div className="margin-empty">Select a cluster to view fragments</div>
        ) : (
          fragments.map((frag) => (
            <FragmentItem key={frag.id} fragment={frag} />
          ))
        )}
      </div>

      <div className="margin-compose">
        <div className="compose-label">
          <span className="compose-label-icon" />
          Your Note
        </div>
        <textarea
          className="compose-area"
          placeholder="Write a note…"
          value={noteText}
          onChange={(e) => setNoteText(e.target.value)}
          onKeyDown={handleKeyDown}
          disabled={loading.saveNote}
        />
        <div className="compose-hint">
          <span>Markdown supported</span>
          <span>
            {loading.saveNote ? (
              'Saving…'
            ) : (
              <><kbd>⌘</kbd> <kbd>↵</kbd> to save</>
            )}
          </span>
        </div>
      </div>
    </aside>
  );
}
