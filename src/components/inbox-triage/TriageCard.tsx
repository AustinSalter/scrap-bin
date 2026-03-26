import type { Fragment, HighlightRange } from '../../types';
import { deriveTitle } from '../../utils/fragments';
import { HighlightableText } from '../shared/HighlightableText';

const SOURCE_ICONS: Record<string, string> = {
  rss: '\u25EB',       // ◫
  readwise: '\u25C9',  // ◉
  twitter: '\u2B21',   // ⬡
  vault: '\u25C7',     // ◇
  podcast: '\u25CE',   // ◎
  apple_notes: '\u25A2', // ▢
};

function deriveSourceName(fragment: Fragment): string {
  const meta = fragment.metadata;
  switch (fragment.sourceType) {
    case 'rss': {
      const feed = typeof meta.feed_title === 'string' ? meta.feed_title : '';
      return feed ? `RSS \u00B7 ${feed}` : 'RSS';
    }
    case 'readwise': {
      const cat = typeof meta.category === 'string' ? meta.category : '';
      return cat ? `Readwise \u00B7 ${cat}` : 'Readwise';
    }
    case 'twitter':
      return 'Twitter';
    case 'vault':
      return 'Obsidian';
    case 'podcast': {
      return 'Podcast';
    }
    case 'apple_notes':
      return 'Apple Notes';
    default:
      return fragment.sourceLabel || 'Unknown';
  }
}

function deriveAuthor(fragment: Fragment): string | null {
  const meta = fragment.metadata;
  if (typeof meta.author === 'string' && meta.author) return meta.author;
  if (typeof meta.feed_title === 'string' && fragment.sourceType === 'rss') return meta.feed_title;
  if (typeof meta.book_author === 'string' && meta.book_author) return meta.book_author;
  if (typeof meta.username === 'string' && meta.username) return `@${meta.username}`;
  return null;
}

function formatRelativeTime(ts: number): string {
  if (!ts) return '';
  const diffMs = Date.now() - ts;
  const diffMins = Math.floor(diffMs / 60000);
  if (diffMins < 1) return 'now';
  if (diffMins < 60) return `${diffMins}m ago`;
  const diffHours = Math.floor(diffMins / 60);
  if (diffHours < 24) return `${diffHours}h ago`;
  const diffDays = Math.floor(diffHours / 24);
  if (diffDays < 7) return `${diffDays}d ago`;
  const diffWeeks = Math.floor(diffDays / 7);
  if (diffWeeks < 4) return `${diffWeeks}w ago`;
  const diffMonths = Math.floor(diffDays / 30);
  return `${diffMonths}mo ago`;
}

interface TriageCardProps {
  fragment: Fragment;
  position: 'is-current' | 'is-next' | 'is-third';
  animating?: string | null;
  onHighlightSave?: (highlights: HighlightRange[]) => void;
}

export function TriageCard({ fragment, position, animating, onHighlightSave }: TriageCardProps) {
  const icon = SOURCE_ICONS[fragment.sourceType] ?? '\u25CB'; // ○
  const sourceName = deriveSourceName(fragment);
  const title = deriveTitle(fragment);
  const author = deriveAuthor(fragment);
  const time = formatRelativeTime(fragment.timestamp);

  const className = `triage-card ${position}${animating ? ` ${animating}` : ''}`;
  const isCurrent = position === 'is-current' && !!onHighlightSave;

  return (
    <div className={className}>
      <div className="card-source">
        <span className="card-source-icon">{icon}</span>
        <span className="card-source-name">{sourceName}</span>
        <span className="card-source-time">{time}</span>
      </div>
      <div className="card-title">{title}</div>
      {isCurrent ? (
        <HighlightableText
          content={fragment.content}
          highlights={fragment.highlights}
          onHighlightSave={onHighlightSave}
          className="card-body"
        />
      ) : (
        <div className="card-body">{fragment.content}</div>
      )}
      {author && <div className="card-author">{author}</div>}
    </div>
  );
}
