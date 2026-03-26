import { useEffect } from 'react';
import { useAppStore } from '../stores/appStore';

export function useKeyboardShortcuts() {
  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      const state = useAppStore.getState();

      // ⌘, — toggle settings
      if (e.key === ',' && e.metaKey) {
        e.preventDefault();
        state.setActiveView(state.activeView === 'settings' ? 'landscape' : 'settings');
        return;
      }

      // ── Inbox triage shortcuts ──
      if (state.activeView === 'inbox-triage') {
        if (e.key === 'Escape') {
          state.goOverview();
          return;
        }
        // CMD+L — promote
        if (e.key === 'l' && e.metaKey && !e.shiftKey) {
          e.preventDefault();
          state.inboxPromote();
          return;
        }
        // CTRL+X — dismiss
        if (e.key === 'x' && e.ctrlKey) {
          e.preventDefault();
          state.inboxDismiss();
          return;
        }
        // ArrowRight — skip
        if (e.key === 'ArrowRight') {
          e.preventDefault();
          state.inboxSkip();
          return;
        }
        return;
      }

      // ── Stream triage shortcuts ──
      if (state.activeView === 'stream-triage') {
        // Escape — return to landscape
        if (e.key === 'Escape') {
          state.goOverview();
          return;
        }

        // ArrowUp / ArrowDown — navigate sidebar list
        if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
          e.preventDefault();
          const { triageFragments, triageSelectedId } = state;
          const idx = triageFragments.findIndex((f) => f.id === triageSelectedId);
          const next = e.key === 'ArrowDown'
            ? Math.min(idx + 1, triageFragments.length - 1)
            : Math.max(idx - 1, 0);
          if (triageFragments[next]) {
            state.setTriageSelectedId(triageFragments[next].id);
          }
          return;
        }

        // ⌘L — signal
        if (e.key === 'l' && e.metaKey && !e.shiftKey) {
          e.preventDefault();
          if (state.triageSelectedId) state.triageDisposition(state.triageSelectedId, 'signal');
          return;
        }

        // ⌘I — inbox
        if (e.key === 'i' && e.metaKey && !e.shiftKey) {
          e.preventDefault();
          if (state.triageSelectedId) state.triageDisposition(state.triageSelectedId, 'inbox');
          return;
        }

        // ⌃X — ignore
        if (e.key === 'x' && e.ctrlKey) {
          e.preventDefault();
          if (state.triageSelectedId) state.triageDisposition(state.triageSelectedId, 'ignored');
          return;
        }

        return;
      }

      // Escape — close settings, close margin, or return to overview
      if (e.key === 'Escape') {
        if (state.activeView === 'settings') {
          state.setActiveView('landscape');
          return;
        }
        if (state.marginOpen) {
          state.clearSelection();
        } else {
          state.goOverview();
        }
        return;
      }

      // ⌘K — enter threaded state and focus search input
      if (e.key === 'k' && e.metaKey) {
        e.preventDefault();
        state.goThreaded();
        setTimeout(() => {
          document.querySelector<HTMLInputElement>('.search-input')?.focus();
        }, 0);
        return;
      }

      // ⌘R — re-cluster
      if (e.key === 'r' && e.metaKey) {
        e.preventDefault();
        state.recluster();
        return;
      }
    }

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, []);
}
