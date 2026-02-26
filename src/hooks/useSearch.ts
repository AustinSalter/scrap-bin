import { useCallback, useEffect, useRef } from 'react';
import { useAppStore } from '../stores/appStore';

export function useSearch() {
  const runSearch = useAppStore((s) => s.runSearch);
  const clearSearch = useAppStore((s) => s.clearSearch);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Clear pending timeout on unmount.
  useEffect(() => {
    return () => {
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, []);

  const debouncedSearch = useCallback(
    (query: string) => {
      if (timerRef.current) {
        clearTimeout(timerRef.current);
      }

      if (!query.trim()) {
        clearSearch();
        return;
      }

      timerRef.current = setTimeout(() => {
        runSearch(query);
      }, 300);
    },
    [runSearch, clearSearch]
  );

  return { debouncedSearch, clearSearch };
}
