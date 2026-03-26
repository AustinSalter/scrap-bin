import { useCallback } from 'react';
import { useAppStore } from '../../stores/appStore';
import { TriageCard } from './TriageCard';
import { FilterChips } from './FilterChips';
import { TriageProgress } from './TriageProgress';
import type { HighlightRange } from '../../types';
import '../../styles/inbox-triage.css';

export function InboxTriageView() {
  const cards = useAppStore((s) => s.inboxCards);
  const loading = useAppStore((s) => s.inboxLoading);
  const animating = useAppStore((s) => s.inboxAnimating);
  const dismiss = useAppStore((s) => s.inboxDismiss);
  const skip = useAppStore((s) => s.inboxSkip);
  const promote = useAppStore((s) => s.inboxPromote);
  const saveHighlights = useAppStore((s) => s.saveHighlights);

  const handleHighlightSave = useCallback(
    (highlights: HighlightRange[]) => {
      if (cards.length > 0) saveHighlights(cards[0].id, highlights);
    },
    [cards, saveHighlights],
  );

  const remaining = cards.length;
  const isAnimating = animating !== null;

  return (
    <div className="inbox-triage-container">
      <div className="inbox-header">
        <span className="inbox-title">Inbox</span>
        <span className="inbox-count">{remaining}</span>
      </div>

      <FilterChips />

      {loading ? (
        <div className="inbox-loading">Loading inbox...</div>
      ) : remaining === 0 ? (
        <div className="inbox-empty">
          <span className="inbox-empty-title">Inbox Clear</span>
          <span className="inbox-empty-subtitle">All fragments have been triaged</span>
        </div>
      ) : (
        <>
          <div className="card-stack">
            {/* Render bottom-to-top for correct z-order */}
            {cards.length > 2 && (
              <TriageCard key={cards[2].id} fragment={cards[2]} position="is-third" />
            )}
            {cards.length > 1 && (
              <TriageCard key={cards[1].id} fragment={cards[1]} position="is-next" />
            )}
            <TriageCard
              key={cards[0].id}
              fragment={cards[0]}
              position="is-current"
              animating={animating}
              onHighlightSave={handleHighlightSave}
            />
          </div>

          <div className="inbox-actions">
            <button
              className="action-btn"
              onClick={dismiss}
              disabled={isAnimating}
              title="Dismiss (Ctrl+X)"
            >
              <span className="action-circle dismiss">X</span>
              <span className="action-label">Ignore</span>
              <span className="action-key">CTRL+X</span>
            </button>

            <button
              className="action-btn"
              onClick={skip}
              disabled={isAnimating}
              title="Skip (Right Arrow)"
            >
              <span className="action-circle skip">{'\u2192'}</span>
              <span className="action-label">Skip</span>
              <span className="action-key">{'\u2192'}</span>
            </button>

            <button
              className="action-btn"
              onClick={promote}
              disabled={isAnimating}
              title="Promote (Cmd+L)"
            >
              <span className="action-circle promote">+</span>
              <span className="action-label">Promote</span>
              <span className="action-key">CMD+L</span>
            </button>
          </div>

          <TriageProgress />
        </>
      )}
    </div>
  );
}
