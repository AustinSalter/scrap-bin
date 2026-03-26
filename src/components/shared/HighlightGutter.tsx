import type { HighlightRange } from '../../types';

const PRIORITY_COLOR: Record<number, string> = {
  1: 'var(--mark-critical)',
  2: 'var(--mark-important)',
  3: 'var(--mark-interesting)',
  4: 'var(--mark-later)',
  5: 'var(--mark-reference)',
};

interface HighlightGutterProps {
  highlights: HighlightRange[];
  contentLength: number;
  onDotClick?: (highlight: HighlightRange) => void;
}

export function HighlightGutter({ highlights, contentLength, onDotClick }: HighlightGutterProps) {
  if (highlights.length === 0 || contentLength === 0) return null;

  return (
    <div className="highlight-gutter">
      {highlights.map((hl) => {
        const pct = Math.min(100, (hl.start / contentLength) * 100);
        return (
          <span
            key={`${hl.start}-${hl.end}`}
            className="gutter-dot"
            style={{
              top: `${pct}%`,
              background: PRIORITY_COLOR[hl.priority] || PRIORITY_COLOR[3],
            }}
            title={hl.text.slice(0, 40)}
            onClick={() => onDotClick?.(hl)}
          />
        );
      })}
    </div>
  );
}
