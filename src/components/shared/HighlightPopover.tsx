import { useEffect, useRef } from 'react';

interface HighlightPopoverProps {
  position: { top: number; left: number };
  canRemove: boolean;
  onHighlight: () => void;
  onRemove: () => void;
  onDismiss: () => void;
}

export function HighlightPopover({
  position,
  canRemove,
  onHighlight,
  onRemove,
  onDismiss,
}: HighlightPopoverProps) {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    function handleDown(e: MouseEvent) {
      if (ref.current && !ref.current.contains(e.target as Node)) {
        onDismiss();
      }
    }
    function handleKey(e: KeyboardEvent) {
      if (e.key === 'Escape') onDismiss();
    }
    document.addEventListener('mousedown', handleDown);
    document.addEventListener('keydown', handleKey);
    return () => {
      document.removeEventListener('mousedown', handleDown);
      document.removeEventListener('keydown', handleKey);
    };
  }, [onDismiss]);

  return (
    <div
      ref={ref}
      className="highlight-popover"
      style={{ top: position.top, left: position.left }}
    >
      <button className="highlight-btn" onClick={onHighlight}>
        Highlight
      </button>
      {canRemove && (
        <button className="highlight-remove-btn" onClick={onRemove}>
          Remove
        </button>
      )}
    </div>
  );
}
