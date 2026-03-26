import type { HighlightRange } from '../../types';

const PRIORITY_COLOR: Record<number, string> = {
  1: 'var(--mark-critical)',
  2: 'var(--mark-important)',
  3: 'var(--mark-interesting)',
  4: 'var(--mark-later)',
  5: 'var(--mark-reference)',
};

interface MarkPipsProps {
  highlights: HighlightRange[];
}

/** Small colored bars showing highlight priorities for a fragment. */
export function MarkPips({ highlights }: MarkPipsProps) {
  if (highlights.length === 0) return null;

  // Show up to 6 pips, sorted by priority (highest first)
  const sorted = [...highlights].sort((a, b) => a.priority - b.priority).slice(0, 6);

  return (
    <div className="mark-pips">
      {sorted.map((hl) => (
        <span
          key={`${hl.start}-${hl.end}`}
          className="mark-pip"
          style={{ background: PRIORITY_COLOR[hl.priority] || PRIORITY_COLOR[3] }}
        />
      ))}
    </div>
  );
}
