import { useCallback } from 'react';
import { open } from '@tauri-apps/plugin-shell';
import { useAppStore } from '../../stores/appStore';
import { HighlightableText } from '../shared/HighlightableText';
import { TweetCard } from '../cards/TweetCard';
import type { Fragment, HighlightRange } from '../../types';

function str(value: unknown): string {
  return typeof value === 'string' ? value : '';
}

function formatDate(ts: number): string {
  if (!ts) return '';
  return new Date(ts).toLocaleDateString(undefined, {
    year: 'numeric',
    month: 'long',
    day: 'numeric',
  });
}

function ExternalLink({ url, children }: { url: string; children: React.ReactNode }) {
  return (
    <button className="detail-link" onClick={() => open(url)}>
      {children}
    </button>
  );
}

interface DetailProps {
  fragment: Fragment;
  onHighlightSave: (highlights: HighlightRange[]) => void;
}

function RssDetail({ fragment, onHighlightSave }: DetailProps) {
  const meta = fragment.metadata;
  const entryTitle = str(meta.entry_title);
  const feedName = str(meta.feed_title) || fragment.sourceLabel;
  const author = str(meta.author);
  const originalUrl = str(meta.original_url) || str(meta.entry_url);

  return (
    <div className="detail-rss">
      {entryTitle && <h2 className="detail-rss-title">{entryTitle}</h2>}
      <div className="detail-rss-meta">
        <span>{feedName}</span>
        {author && <span> &middot; {author}</span>}
        <span> &middot; {formatDate(fragment.timestamp)}</span>
      </div>
      <HighlightableText
        content={fragment.content}
        highlights={fragment.highlights}
        onHighlightSave={onHighlightSave}
        className="detail-rss-body"
      />
      {originalUrl && <ExternalLink url={originalUrl}>Read original</ExternalLink>}
    </div>
  );
}

function ReadwiseDetail({ fragment, onHighlightSave }: DetailProps) {
  const meta = fragment.metadata;
  const bookTitle = str(meta.book_title) || str(meta.source_path);
  const author = str(meta.author);

  return (
    <div className="detail-readwise">
      <HighlightableText
        content={fragment.content}
        highlights={fragment.highlights}
        onHighlightSave={onHighlightSave}
        className="detail-readwise-quote"
      />
      <div className="detail-readwise-source">
        {bookTitle && <span className="detail-readwise-title">{bookTitle}</span>}
        {author && <span className="detail-readwise-author"> &mdash; {author}</span>}
      </div>
    </div>
  );
}

function VaultDetail({ fragment, onHighlightSave }: DetailProps) {
  return (
    <div className="detail-note">
      {fragment.headingPath.length > 0 && (
        <div className="detail-note-breadcrumb">
          {fragment.headingPath.join(' > ')}
        </div>
      )}
      <HighlightableText
        content={fragment.content}
        highlights={fragment.highlights}
        onHighlightSave={onHighlightSave}
        className="detail-note-content"
      />
      {fragment.tags.length > 0 && (
        <div className="detail-note-tags">
          {fragment.tags.map((t) => (
            <span key={t} className="detail-note-tag">#{t}</span>
          ))}
        </div>
      )}
    </div>
  );
}

function AppleNotesDetail({ fragment, onHighlightSave }: DetailProps) {
  const meta = fragment.metadata;
  const fileName = str(meta.source_path);
  const dirPath = fileName ? fileName.split('/').slice(0, -1).join(' / ') : '';

  return (
    <div className="detail-apple-note">
      {fileName && (
        <h2 className="detail-apple-note-title">
          {fileName.split('/').pop()?.replace(/\.[^.]+$/, '') ?? 'Note'}
        </h2>
      )}
      {dirPath && <div className="detail-apple-note-breadcrumb">{dirPath}</div>}
      <HighlightableText
        content={fragment.content}
        highlights={fragment.highlights}
        onHighlightSave={onHighlightSave}
        className="detail-apple-note-content"
      />
    </div>
  );
}

function PodcastDetail({ fragment, onHighlightSave }: DetailProps) {
  const meta = fragment.metadata;
  const fileName = str(meta.source_path) || 'Transcript';
  const format = fileName.match(/\.(srt|vtt|txt)$/)?.[1]?.toUpperCase() ?? 'TXT';

  return (
    <div className="detail-podcast">
      <div className="detail-podcast-header">
        <span className="detail-podcast-name">{fileName.replace(/\.[^.]+$/, '')}</span>
        <span className="detail-podcast-format">{format}</span>
      </div>
      <HighlightableText
        content={fragment.content}
        highlights={fragment.highlights}
        onHighlightSave={onHighlightSave}
        className="detail-podcast-content"
      />
    </div>
  );
}

function SourceRenderer({ fragment, onHighlightSave }: DetailProps) {
  switch (fragment.sourceType) {
    case 'twitter':
      return <TweetCard fragment={fragment} onHighlightSave={onHighlightSave} />;
    case 'rss':
      return <RssDetail fragment={fragment} onHighlightSave={onHighlightSave} />;
    case 'readwise':
      return <ReadwiseDetail fragment={fragment} onHighlightSave={onHighlightSave} />;
    case 'vault':
      return <VaultDetail fragment={fragment} onHighlightSave={onHighlightSave} />;
    case 'apple_notes':
      return <AppleNotesDetail fragment={fragment} onHighlightSave={onHighlightSave} />;
    case 'podcast':
      return <PodcastDetail fragment={fragment} onHighlightSave={onHighlightSave} />;
    default:
      return (
        <HighlightableText
          content={fragment.content}
          highlights={fragment.highlights}
          onHighlightSave={onHighlightSave}
          className="detail-note-content"
        />
      );
  }
}

export function FragmentDetail() {
  const triageFragments = useAppStore((s) => s.triageFragments);
  const triageSelectedId = useAppStore((s) => s.triageSelectedId);
  const saveHighlights = useAppStore((s) => s.saveHighlights);

  const fragment = triageFragments.find((f) => f.id === triageSelectedId);

  const handleHighlightSave = useCallback(
    (highlights: HighlightRange[]) => {
      if (fragment) saveHighlights(fragment.id, highlights);
    },
    [fragment, saveHighlights],
  );

  if (!fragment) {
    return (
      <div className="detail-pane">
        <div className="detail-empty">Select a fragment to view details</div>
      </div>
    );
  }

  return (
    <div className="detail-pane">
      <div className="detail-header">
        <span className="detail-source-badge" data-source={fragment.sourceType}>
          {fragment.sourceLabel}
        </span>
        <span className="detail-disposition" data-disposition={fragment.disposition}>
          {fragment.disposition}
        </span>
      </div>
      <div className="detail-body">
        <SourceRenderer fragment={fragment} onHighlightSave={handleHighlightSave} />
      </div>
    </div>
  );
}
