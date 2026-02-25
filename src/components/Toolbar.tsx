import { useAppStore } from '../stores/appStore';
import { useSearch } from '../hooks/useSearch';

export function Toolbar() {
  const uiState = useAppStore((s) => s.uiState);
  const toggleStream = useAppStore((s) => s.toggleStream);
  const recluster = useAppStore((s) => s.recluster);
  const loading = useAppStore((s) => s.loading);
  const searchQuery = useAppStore((s) => s.searchQuery);
  const { debouncedSearch, clearSearch } = useSearch();

  const isSearchActive = searchQuery.length > 0;

  return (
    <div className="landscape-toolbar drag-region-exempt">
      <div className={`search-box${isSearchActive ? ' is-active' : ''}`}>
        <span className="search-icon">⌕</span>
        <input
          className="search-input"
          type="text"
          placeholder="Search fragments, clusters, threads…"
          defaultValue={searchQuery}
          onChange={(e) => debouncedSearch(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Escape') {
              clearSearch();
              (e.target as HTMLInputElement).value = '';
            }
          }}
        />
        <kbd className="search-key">⌘K</kbd>
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
