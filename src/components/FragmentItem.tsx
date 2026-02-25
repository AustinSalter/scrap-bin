import { useState } from 'react';
import type { Fragment } from '../types';
import { relativeTime } from '../utils/time';

const SOURCE_COLORS: Record<string, string> = {
  vault:     'var(--signal-green)',
  twitter:   'var(--signal-blue)',
  readwise:  'var(--signal-purple)',
  podcast:   'var(--signal-amber)',
};

const MIME_FRAGMENT = 'application/x-scrapbin-fragment';

interface Props {
  fragment: Fragment;
}

export function FragmentItem({ fragment }: Props) {
  const [isDragging, setIsDragging] = useState(false);

  const dotColor = fragment.isYourNote
    ? 'var(--accent)'
    : SOURCE_COLORS[fragment.sourceType];

  const label = fragment.isYourNote
    ? `Your note \u00b7 ${relativeTime(fragment.timestamp)}`
    : `${fragment.sourceLabel} \u00b7 ${relativeTime(fragment.timestamp)}`;

  const handleDragStart = (e: React.DragEvent) => {
    e.dataTransfer.setData(
      MIME_FRAGMENT,
      JSON.stringify({ type: 'fragment', id: fragment.id, fromCluster: fragment.clusterId })
    );
    e.dataTransfer.effectAllowed = 'move';
    setIsDragging(true);
  };

  const handleDragEnd = () => {
    setIsDragging(false);
  };

  const classNames = [
    'frag',
    fragment.isYourNote && 'is-yours',
    isDragging && 'is-dragging',
  ]
    .filter(Boolean)
    .join(' ');

  return (
    <div
      className={classNames}
      draggable
      onDragStart={handleDragStart}
      onDragEnd={handleDragEnd}
    >
      <div className="frag-src">
        <span className="frag-src-dot" style={{ background: dotColor }} />
        <span className="frag-src-label">{label}</span>
      </div>
      <div className="frag-content">{fragment.content}</div>
      {fragment.tags.length > 0 && (
        <div className="frag-tags">
          {fragment.tags.map((tag) => (
            <span key={tag} className="frag-tag">{tag}</span>
          ))}
        </div>
      )}
    </div>
  );
}
