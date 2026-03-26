import { useRef, useState, useCallback } from 'react';
import type { HighlightRange } from '../../types';
import { FloatingToolbar } from './FloatingToolbar';
import '../../styles/highlights.css';

// ── Priority → CSS class mapping ─────────────────────────

const PRIORITY_CLASS: Record<number, string> = {
  1: 'mark-critical',
  2: 'mark-important',
  3: 'mark-interesting',
  4: 'mark-later',
  5: 'mark-reference',
};

// ── Segment computation ──────────────────────────────────

interface Segment {
  text: string;
  isHighlight: boolean;
  offset: number;
  priority: number;
}

/** Split content into alternating plain/highlight segments.
 *  Highlights are NOT merged here — each keeps its own priority. */
function computeSegments(content: string, highlights: HighlightRange[]): Segment[] {
  if (highlights.length === 0) return [{ text: content, isHighlight: false, offset: 0, priority: 0 }];

  const sorted = [...highlights].sort((a, b) => a.start - b.start);
  const segments: Segment[] = [];
  let cursor = 0;

  for (const hl of sorted) {
    const start = Math.max(hl.start, cursor);
    if (start > cursor) {
      segments.push({ text: content.slice(cursor, start), isHighlight: false, offset: cursor, priority: 0 });
    }
    if (start < hl.end) {
      segments.push({ text: content.slice(start, hl.end), isHighlight: true, offset: start, priority: hl.priority });
    }
    cursor = Math.max(cursor, hl.end);
  }
  if (cursor < content.length) {
    segments.push({ text: content.slice(cursor), isHighlight: false, offset: cursor, priority: 0 });
  }
  return segments;
}

// ── DOM offset helpers ───────────────────────────────────

function getCharOffset(
  container: HTMLElement,
  targetNode: Node,
  nodeOffset: number,
): number {
  const walker = document.createTreeWalker(container, NodeFilter.SHOW_TEXT);
  let charOffset = 0;
  let node: Node | null;

  while ((node = walker.nextNode())) {
    if (node === targetNode) {
      return charOffset + nodeOffset;
    }
    charOffset += (node.textContent?.length ?? 0);
  }
  return charOffset + nodeOffset;
}

// ── Component ────────────────────────────────────────────

interface HighlightableTextProps {
  content: string;
  highlights: HighlightRange[];
  onHighlightSave: (highlights: HighlightRange[]) => void;
  interactive?: boolean;
  className?: string;
}

interface ToolbarState {
  position: { top: number; left: number };
  start: number;
  end: number;
  canRemove: boolean;
}

export function HighlightableText({
  content,
  highlights,
  onHighlightSave,
  interactive = true,
  className,
}: HighlightableTextProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [toolbar, setToolbar] = useState<ToolbarState | null>(null);

  const handleMouseUp = useCallback(() => {
    if (!interactive || !containerRef.current) return;

    const sel = window.getSelection();
    if (!sel || sel.isCollapsed || sel.rangeCount === 0) return;

    const range = sel.getRangeAt(0);
    if (!containerRef.current.contains(range.commonAncestorContainer)) return;

    const start = getCharOffset(containerRef.current, range.startContainer, range.startOffset);
    const end = getCharOffset(containerRef.current, range.endContainer, range.endOffset);
    if (start === end) return;

    const [lo, hi] = start < end ? [start, end] : [end, start];
    const canRemove = highlights.some((h) => h.start === lo && h.end === hi);

    const rect = range.getBoundingClientRect();
    setToolbar({
      position: { top: rect.top - 40, left: rect.left + rect.width / 2 },
      start: lo,
      end: hi,
      canRemove,
    });
  }, [interactive, highlights]);

  const handleHighlight = useCallback((priority: number) => {
    if (!toolbar) return;
    const newRange: HighlightRange = {
      start: toolbar.start,
      end: toolbar.end,
      text: content.slice(toolbar.start, toolbar.end),
      priority,
    };
    // Add new highlight without merging (each highlight keeps its own priority)
    const updated = [...highlights.filter(
      (h) => !(h.start < newRange.end && h.end > newRange.start)
    ), newRange].sort((a, b) => a.start - b.start);
    onHighlightSave(updated);
    setToolbar(null);
    window.getSelection()?.removeAllRanges();
  }, [toolbar, highlights, content, onHighlightSave]);

  const handleRemove = useCallback(() => {
    if (!toolbar) return;
    const updated = highlights.filter(
      (h) => !(h.start === toolbar.start && h.end === toolbar.end),
    );
    onHighlightSave(updated);
    setToolbar(null);
    window.getSelection()?.removeAllRanges();
  }, [toolbar, highlights, onHighlightSave]);

  const handleDismiss = useCallback(() => {
    setToolbar(null);
    window.getSelection()?.removeAllRanges();
  }, []);

  const segments = computeSegments(content, highlights);

  return (
    <div
      ref={containerRef}
      className={className}
      onMouseUp={handleMouseUp}
    >
      {segments.map((seg) =>
        seg.isHighlight ? (
          <mark
            key={`hl-${seg.offset}`}
            className={PRIORITY_CLASS[seg.priority] || 'mark-interesting'}
          >
            {seg.text}
          </mark>
        ) : (
          <span key={`pl-${seg.offset}`}>{seg.text}</span>
        ),
      )}
      {toolbar && (
        <FloatingToolbar
          position={toolbar.position}
          canRemove={toolbar.canRemove}
          onHighlight={handleHighlight}
          onRemove={handleRemove}
          onDismiss={handleDismiss}
        />
      )}
    </div>
  );
}
