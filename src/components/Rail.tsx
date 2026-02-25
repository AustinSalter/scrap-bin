import { useAppStore } from '../stores/appStore';
import '../styles/rail.css';

export function Rail() {
  const activeRailIcon = useAppStore((s) => s.activeRailIcon);
  const hasNewItems = useAppStore((s) => s.hasNewItems);
  const goOverview = useAppStore((s) => s.goOverview);
  const toggleStream = useAppStore((s) => s.toggleStream);
  const goThreaded = useAppStore((s) => s.goThreaded);

  return (
    <nav className="rail">
      <div className="rail-logo drag-region-exempt">δ</div>

      <button
        className={`rail-icon drag-region-exempt${activeRailIcon === 'landscape' ? ' active' : ''}`}
        onClick={goOverview}
        title="Landscape"
      >
        ◎
      </button>

      <button
        className={`rail-icon drag-region-exempt${activeRailIcon === 'stream' ? ' active' : ''}${hasNewItems ? ' has-badge' : ''}`}
        onClick={toggleStream}
        title="Stream"
      >
        ↓
      </button>

      <button
        className={`rail-icon drag-region-exempt${activeRailIcon === 'search' ? ' active' : ''}`}
        onClick={() => goThreaded()}
        title="Search"
      >
        ⌕
      </button>

      <div className="rail-spacer" />

      <button
        className={`rail-icon drag-region-exempt${activeRailIcon === 'settings' ? ' active' : ''}`}
        title="Settings"
      >
        ⚙
      </button>
    </nav>
  );
}
