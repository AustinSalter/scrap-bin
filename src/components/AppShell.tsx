import { useEffect } from 'react';
import { Rail } from './Rail';
import { StreamPanel } from './StreamPanel';
import { Landscape } from './Landscape';
import { MarginPanel } from './MarginPanel';
import { SettingsPanel } from './settings/SettingsPanel';
import { StreamTriageView } from './stream-triage/StreamTriageView';
import { InboxTriageView } from './inbox-triage/InboxTriageView';
import { Reader } from './reader/Reader';
import { ErrorToast } from './ErrorToast';
import { useKeyboardShortcuts } from '../hooks/useKeyboardShortcuts';
import { useInitialize } from '../hooks/useInitialize';
import { useVaultWatcher } from '../hooks/useVaultWatcher';
import { useAppStore } from '../stores/appStore';
import '../styles/app-shell.css';

const STATUS_POLL_INTERVAL = 30_000;

export function AppShell() {
  useKeyboardShortcuts();
  useInitialize();
  useVaultWatcher();

  const fetchStatus = useAppStore((s) => s.fetchStatus);
  const loading = useAppStore((s) => s.loading);
  const activeView = useAppStore((s) => s.activeView);

  // Status polling every 30 seconds after initialization.
  useEffect(() => {
    if (loading.clusters) return; // Still initializing.
    const interval = setInterval(fetchStatus, STATUS_POLL_INTERVAL);
    return () => clearInterval(interval);
  }, [fetchStatus, loading.clusters]);

  return (
    <>
      <div className="drag-region" />
      <div className="app-shell">
        <Rail />
        {activeView === 'settings' ? (
          <SettingsPanel />
        ) : activeView === 'reader' ? (
          <Reader />
        ) : activeView === 'inbox-triage' ? (
          <InboxTriageView />
        ) : activeView === 'stream-triage' ? (
          <StreamTriageView />
        ) : (
          <>
            <StreamPanel />
            <Landscape />
            <MarginPanel />
          </>
        )}
      </div>
      <ErrorToast />
    </>
  );
}
