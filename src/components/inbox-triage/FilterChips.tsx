import { useAppStore } from '../../stores/appStore';
import type { InboxSourceFilter } from '../../stores/appStore';

const FILTERS: { key: InboxSourceFilter; label: string }[] = [
  { key: 'all', label: 'All' },
  { key: 'rss', label: 'RSS' },
  { key: 'readwise', label: 'Readwise' },
  { key: 'twitter', label: 'Twitter' },
  { key: 'vault', label: 'Obsidian' },
  { key: 'podcast', label: 'Podcast' },
  { key: 'apple_notes', label: 'Notes' },
];

export function FilterChips() {
  const activeFilter = useAppStore((s) => s.inboxSourceFilter);
  const setFilter = useAppStore((s) => s.setInboxSourceFilter);

  return (
    <div className="inbox-filters">
      {FILTERS.map((f) => (
        <button
          key={f.key}
          className={`filter-chip${activeFilter === f.key ? ' is-active' : ''}`}
          onClick={() => setFilter(f.key)}
        >
          {f.label}
        </button>
      ))}
    </div>
  );
}
