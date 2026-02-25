import { useAppStore } from '../stores/appStore';
import { StreamItem } from './StreamItem';
import { groupByDay } from '../utils/time';
import '../styles/stream.css';

export function StreamPanel() {
  const streamOpen = useAppStore((s) => s.streamOpen);
  const streamItems = useAppStore((s) => s.streamItems);
  const selectedClusterId = useAppStore((s) => s.selectedClusterId);

  const newCount = streamItems.filter((i) => i.isNew).length;
  const groups = groupByDay(streamItems);

  return (
    <aside className={`stream${streamOpen ? ' is-open' : ''}`}>
      <div className="stream-header">
        <span className="stream-title">Stream</span>
        <span className={`stream-count${newCount > 0 ? ' has-new' : ''}`}>
          {newCount > 0 ? `${newCount} new` : streamItems.length}
        </span>
      </div>
      <div className="stream-body">
        {streamItems.length === 0 ? (
          <div className="stream-empty">No items yet</div>
        ) : (
          groups.map((group) => (
            <div key={group.label}>
              <div className="stream-day">{group.label}</div>
              {group.items.map((item) => (
                <StreamItem
                  key={item.id}
                  item={item}
                  isSelected={selectedClusterId === item.clusterId}
                />
              ))}
            </div>
          ))
        )}
      </div>
    </aside>
  );
}
