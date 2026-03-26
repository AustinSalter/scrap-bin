import type { Fragment } from '../../types';
import { MarkPips } from '../shared/MarkPips';

const SOURCE_LABELS: Record<string, string> = {
  vault: 'Obsidian',
  twitter: 'Twitter',
  readwise: 'Readwise',
  podcast: 'Podcast',
  rss: 'RSS',
  apple_notes: 'Notes',
};

interface FragmentPreviewProps {
  fragment: Fragment;
}

/** 2-line preview card for a fragment inside a cluster cell. */
export function FragmentPreview({ fragment }: FragmentPreviewProps) {
  const preview = fragment.content.slice(0, 120).replace(/\n/g, ' ').trim();
  const source = SOURCE_LABELS[fragment.sourceType] ?? fragment.sourceLabel;

  return (
    <div className="fragment-preview">
      <div className="fragment-preview-source">{source}</div>
      <div className="fragment-preview-text">{preview}</div>
      <MarkPips highlights={fragment.highlights} />
    </div>
  );
}
