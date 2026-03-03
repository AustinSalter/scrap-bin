import { useEffect } from 'react';
import { useAppStore } from '../stores/appStore';

export function useKeyboardShortcuts() {
  const goOverview = useAppStore((s) => s.goOverview);
  const goThreaded = useAppStore((s) => s.goThreaded);
  const marginOpen = useAppStore((s) => s.marginOpen);
  const clearSelection = useAppStore((s) => s.clearSelection);
  const recluster = useAppStore((s) => s.recluster);
  const activeView = useAppStore((s) => s.activeView);
  const setActiveView = useAppStore((s) => s.setActiveView);

  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      // ⌘, — toggle settings
      if (e.key === ',' && e.metaKey) {
        e.preventDefault();
        setActiveView(activeView === 'settings' ? 'landscape' : 'settings');
        return;
      }

      // Escape — close settings, close margin, or return to overview
      if (e.key === 'Escape') {
        if (activeView === 'settings') {
          setActiveView('landscape');
          return;
        }
        if (marginOpen) {
          clearSelection();
        } else {
          goOverview();
        }
        return;
      }

      // ⌘K — enter threaded state and focus search input
      if (e.key === 'k' && e.metaKey) {
        e.preventDefault();
        goThreaded();
        setTimeout(() => {
          document.querySelector<HTMLInputElement>('.search-input')?.focus();
        }, 0);
        return;
      }

      // ⌘R — re-cluster
      if (e.key === 'r' && e.metaKey) {
        e.preventDefault();
        recluster();
        return;
      }
    }

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [goOverview, goThreaded, marginOpen, clearSelection, recluster, activeView, setActiveView]);
}
