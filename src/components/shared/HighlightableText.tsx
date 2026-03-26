import { useRef, useState, useCallback } from 'react';
import type { HighlightRange } from '../../types';
import { HighlightPopover } from './HighlightPopover';
import '../../styles/highlights.css';

// ── Segment computation ──────────────────────────────────────

interface Segment {
  text: string;
  isHighlight: boolean;
  offset: number;
}

/** Merge overlapping/adjacent ranges into a sorted, non-overlapping set. */
function mergeRanges(ranges: HighlightRange[], content: string): HighlightRange[] {
  if (ranges.length === 0) return [];
  const sorted = [...ranges].sort((a, b) => a.start - b.start);
  const merged: HighlightRange[] = [{ ...sorted[0] }];

  for (let i = 1; i < sorted.length; i++) {
    const last = merged[merged.length - 1];
    if (sorted[i].start <= last.end) {
      last.end = Math.max(last.end, sorted[i].end);
      last.text = content.slice(last.start, last.end);
    } else {
      merged.push({ ...sorted[i] });
    }
  }
  return merged;
}

/** Split content into alternating plain/highlight segments. */
function computeSegments(content: string, highlights: HighlightRange[]): Segment[] {
  if (highlights.length === 0) return [{ text: content, isHighlight: false, offset: 0 }];

  const merged = mergeRanges(highlights, content);
  const segments: Segment[] = [];
  let cursor = 0;

  for (const hl of merged) {
    if (hl.start > cursor) {
      segments.push({ text: content.slice(cursor, hl.start), isHighlight: false, offset: cursor });
    }
    segments.push({ text: content.slice(hl.start, hl.end), isHighlight: true, offset: hl.start });
    cursor = hl.end;
  }
  if (cursor < content.length) {
    segments.push({ text: content.slice(cursor), isHighlight: false, offset: cursor });
  }
  return segments;
}

// ── DOM offset helpers ───────────────────────────────────────

/** Walk text nodes in DOM order to convert a (node, offset) pair to a
 *  character offset within the container's full text content. */
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

  // Fallback: node not found (shouldn't happen for valid selections)
  return charOffset + nodeOffset;
}

// ── Component ────────────────────────────────────────────────

interface HighlightableTextProps {
  content: string;
  highlights: HighlightRange[];
  onHighlightSave: (highlights: HighlightRange[]) => void;
  interactive?: boolean;
  className?: string;
}

interface PopoverState {
  position: { top: number; left: number };
  start: number;
  end: number;
  /** Whether the selection exactly matches an existing highlight range. */
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
  const [popover, setPopover] = useState<PopoverState | null>(null);

  const handleMouseUp = useCallback(() => {
    if (!interactive || !containerRef.current) return;

    const sel = window.getSelection();
    if (!sel || sel.isCollapsed || sel.rangeCount === 0) return;

    const range = sel.getRangeAt(0);
    // Ensure the selection is within our container.
    if (!containerRef.current.contains(range.commonAncestorContainer)) {
      return;
    }

    const start = getCharOffset(containerRef.current, range.startContainer, range.startOffset);
    const end = getCharOffset(containerRef.current, range.endContainer, range.endOffset);

    if (start === end) return;

    const [lo, hi] = start < end ? [start, end] : [end, start];

    // Check if this selection exactly matches an existing highlight.
    const canRemove = highlights.some((h) => h.start === lo && h.end === hi);

    // Position popover above the selection.
    const rect = range.getBoundingClientRect();
    setPopover({
      position: { top: rect.top - 36, left: rect.left + rect.width / 2 - 50 },
      start: lo,
      end: hi,
      canRemove,
    });
  }, [interactive, highlights]);

  const handleHighlight = useCallback(() => {
    if (!popover) return;
    const newRange: HighlightRange = {
      start: popover.start,
      end: popover.end,
      text: content.slice(popover.start, popover.end),
    };
    const merged = mergeRanges([...highlights, newRange], content);
    onHighlightSave(merged);
    setPopover(null);
    window.getSelection()?.removeAllRanges();
  }, [popover, highlights, content, onHighlightSave]);

  const handleRemove = useCallback(() => {
    if (!popover) return;
    const updated = highlights.filter(
      (h) => !(h.start === popover.start && h.end === popover.end),
    );
    onHighlightSave(updated);
    setPopover(null);
    window.getSelection()?.removeAllRanges();
  }, [popover, highlights, onHighlightSave]);

  const handleDismiss = useCallback(() => {
    setPopover(null);
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
          <mark key={`hl-${seg.offset}`} className="fragment-highlight">{seg.text}</mark>
        ) : (
          <span key={`pl-${seg.offset}`}>{seg.text}</span>
        ),
      )}
      {popover && (
        <HighlightPopover
          position={popover.position}
          canRemove={popover.canRemove}
          onHighlight={handleHighlight}
          onRemove={handleRemove}
          onDismiss={handleDismiss}
        />
      )}
    </div>
  );
}
