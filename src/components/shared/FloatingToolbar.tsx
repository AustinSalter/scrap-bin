import { useEffect, useRef, useCallback } from 'react';

const PENS = [
  { priority: 1, label: 'Critical', color: 'var(--mark-critical)' },
  { priority: 2, label: 'Important', color: 'var(--mark-important)' },
  { priority: 3, label: 'Interesting', color: 'var(--mark-interesting)' },
  { priority: 4, label: 'Revisit', color: 'var(--mark-later)' },
  { priority: 5, label: 'Reference', color: 'var(--mark-reference)' },
] as const;

interface FloatingToolbarProps {
  position: { top: number; left: number };
  canRemove: boolean;
  onHighlight: (priority: number) => void;
  onRemove: () => void;
  onDismiss: () => void;
}

export function FloatingToolbar({
  position,
  canRemove,
  onHighlight,
  onRemove,
  onDismiss,
}: FloatingToolbarProps) {
  const ref = useRef<HTMLDivElement>(null);

  const handleKey = useCallback((e: KeyboardEvent) => {
    if (e.key === 'Escape') {
      onDismiss();
      return;
    }
    const num = parseInt(e.key, 10);
    if (num >= 1 && num <= 5) {
      onHighlight(num);
    }
  }, [onDismiss, onHighlight]);

  useEffect(() => {
    function handleDown(e: MouseEvent) {
      if (ref.current && !ref.current.contains(e.target as Node)) {
        onDismiss();
      }
    }
    document.addEventListener('mousedown', handleDown);
    document.addEventListener('keydown', handleKey);
    return () => {
      document.removeEventListener('mousedown', handleDown);
      document.removeEventListener('keydown', handleKey);
    };
  }, [onDismiss, handleKey]);

  // Center toolbar horizontally on the selection, clamp to viewport
  const toolbarWidth = canRemove ? 240 : 190;
  const left = Math.max(8, Math.min(position.left - toolbarWidth / 2, window.innerWidth - toolbarWidth - 8));
  const top = position.top < 44 ? position.top + 44 : position.top;

  return (
    <div
      ref={ref}
      className="floating-toolbar"
      style={{ top, left }}
    >
      {PENS.map((pen) => (
        <button
          key={pen.priority}
          className="pen-btn"
          title={`${pen.label} (${pen.priority})`}
          onClick={() => onHighlight(pen.priority)}
        >
          <span className="pen-nib" style={{ background: pen.color }} />
          <span className="pen-key">{pen.priority}</span>
        </button>
      ))}
      {canRemove && (
        <>
          <div className="floating-toolbar-sep" />
          <button className="toolbar-action" onClick={onRemove}>
            Remove
          </button>
        </>
      )}
    </div>
  );
}
