import { useEffect } from 'react';
import { listen } from '@tauri-apps/api/event';
import { useAppStore } from '../stores/appStore';
import { pipelineIndexFile } from '../api/commands';
import { transformFragment } from '../api/transforms';
import type { FileChangeEvent } from '../types';

export function useVaultWatcher() {
  const vaultPath = useAppStore((s) => s.vaultPath);
  const addStreamItems = useAppStore((s) => s.addStreamItems);
  const setHasNewItems = useAppStore((s) => s.setHasNewItems);
  const fetchStatus = useAppStore((s) => s.fetchStatus);

  useEffect(() => {
    if (!vaultPath) return;

    const unlisten = listen<FileChangeEvent[]>('vault-file-changed', async (event) => {
      const changes = event.payload;

      for (const change of changes) {
        if (change.event_type === 'Deleted') continue;

        try {
          const result = await pipelineIndexFile(vaultPath, change.absolute_path);

          if (!result.skipped && result.chunks_created > 0) {
            // Build a minimal fragment representation for the stream.
            const rawFragment = {
              id: `watcher-${Date.now()}`,
              content: change.path,
              source_type: 'vault',
              metadata: {
                source_type: 'vault',
                source_path: change.path,
                modified_at: change.timestamp,
                tags: '',
                heading_path: '',
                cluster_id: -1,
              },
            };
            const fragment = transformFragment(rawFragment as Record<string, unknown>);
            addStreamItems([fragment]);
            setHasNewItems(true);
          }
        } catch (e) {
          console.error('Failed to index file change:', change.path, e);
        }
      }

      // Update status counts after processing.
      fetchStatus();
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, [vaultPath, addStreamItems, setHasNewItems, fetchStatus]);
}
