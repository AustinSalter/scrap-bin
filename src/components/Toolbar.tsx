import { useState, useEffect } from 'react';
import { useAppStore } from '../stores/appStore';
import { useSearch } from '../hooks/useSearch';

export function Toolbar() {
  const uiState = useAppStore((s) => s.uiState);
  const toggleStream = useAppStore((s) => s.toggleStream);
  const recluster = useAppStore((s) => s.recluster);
  const loading = useAppStore((s) => s.loading);
  const searchQuery = useAppStore((s) => s.searchQuery);
  const clusterViewMode = useAppStore((s) => s.clusterViewMode);
  const setClusterViewMode = useAppStore((s) => s.setClusterViewMode);
  const { debouncedSearch, clearSearch } = useSearch();

  const [inputValue, setInputValue] = useState(searchQuery);

  // Sync store → local when the store clears the query.
  useEffect(() => {
    if (!searchQuery) setInputValue('');
  }, [searchQuery]);

  const isSearchActive = searchQuery.length > 0;

  return (
    <div className="landscape-toolbar drag-region-exempt">
      <div className={`search-box${isSearchActive ? ' is-active' : ''}`}>
        <span className="search-icon">⌕</span>
        <input
          className="search-input"
          type="text"
          placeholder="Search fragments, clusters, threads…"
          value={inputValue}
          onChange={(e) => {
            setInputValue(e.target.value);
            debouncedSearch(e.target.value);
          }}
          onKeyDown={(e) => {
            if (e.key === 'Escape') {
              clearSearch();
              setInputValue('');
            }
          }}
        />
        <kbd className="search-key">⌘K</kbd>
      </div>

      <div className="toolbar-toggle">
        <button
          className={`toolbar-toggle-btn${clusterViewMode === 'landscape' ? ' is-active' : ''}`}
          onClick={() => setClusterViewMode('landscape')}
          title="Landscape view"
        >
          {'\u25CE'}
        </button>
        <button
          className={`toolbar-toggle-btn${clusterViewMode === 'grid' ? ' is-active' : ''}`}
          onClick={() => setClusterViewMode('grid')}
          title="Grid view"
        >
          {'\u25A6'}
        </button>
      </div>

      <button
        className="toolbar-btn"
        onClick={recluster}
        disabled={loading.recluster}
      >
        {loading.recluster ? 'Clustering…' : 'Re-cluster'}
      </button>

      {uiState === 'overview' && (
        <button className="toolbar-btn is-accent" onClick={toggleStream}>
          ↓ Stream
        </button>
      )}
    </div>
  );
}
