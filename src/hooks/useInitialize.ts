import { useEffect, useRef } from 'react';
import { useAppStore } from '../stores/appStore';

export function useInitialize() {
  const initialized = useRef(false);
  const fetchInitialData = useAppStore((s) => s.fetchInitialData);

  useEffect(() => {
    if (initialized.current) return;
    initialized.current = true;
    fetchInitialData();
  }, [fetchInitialData]);
}
