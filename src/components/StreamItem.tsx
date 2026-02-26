import type { StreamItem as StreamItemType } from '../types';
import { relativeTime } from '../utils/time';
import { useAppStore } from '../stores/appStore';

const SOURCE_COLORS: Record<string, string> = {
  vault:     'var(--signal-green)',
  twitter:   'var(--signal-blue)',
  readwise:  'var(--signal-purple)',
  podcast:   'var(--signal-amber)',
};

interface Props {
  item: StreamItemType;
  isSelected: boolean;
}

export function StreamItem({ item, isSelected }: Props) {
  const goBrowsing = useAppStore((s) => s.goBrowsing);

  const classes = [
    'stream-item',
    item.isNew ? 'is-new' : '',
    isSelected ? 'is-selected' : '',
  ].filter(Boolean).join(' ');

  return (
    <div
      className={classes}
      role="button"
      tabIndex={0}
      onClick={() => goBrowsing(item.clusterId)}
      onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); goBrowsing(item.clusterId); } }}
    >
      <div className="stream-item-src">
        <span
          className="stream-item-dot"
          style={{ background: SOURCE_COLORS[item.sourceType] }}
        />
        <span className="stream-item-source">{item.sourceLabel}</span>
        <span className="stream-item-time">{relativeTime(item.timestamp)}</span>
      </div>
      <div className="stream-item-title">{item.title}</div>
      <div className="stream-item-cluster">
        <span className="stream-item-cluster-pill">{'\u2192'} {item.clusterLabel}</span>
      </div>
    </div>
  );
}
