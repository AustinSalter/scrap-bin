import { useAppStore } from '../stores/appStore';
import type { ServiceHealth } from '../types';

const HEALTH_COLOR: Record<ServiceHealth, string> = {
  ok:   'var(--ok)',
  warn: 'var(--warn)',
  err:  'var(--err)',
};

export function StatusBar() {
  const statusData = useAppStore((s) => s.statusData);
  const uiState = useAppStore((s) => s.uiState);
  const selectedThreadId = useAppStore((s) => s.selectedThreadId);
  const threads = useAppStore((s) => s.threads);

  const selectedThread =
    uiState === 'threaded' && selectedThreadId
      ? threads.find((t) => t.id === selectedThreadId)
      : null;

  return (
    <div className="landscape-status">
      <div className="status-item">
        <span className="status-dot" style={{ background: HEALTH_COLOR[statusData.chromaHealth] }} />
        Chroma
      </div>
      <div className="status-item">
        <span className="status-dot" style={{ background: HEALTH_COLOR[statusData.embeddingHealth] }} />
        Embeddings
      </div>

      <div className="status-sep" />

      <div className="status-item">{statusData.fragmentCount} fragments</div>
      <div className="status-item">{statusData.clusterCount} clusters</div>
      <div className="status-item">{statusData.threadCount} threads</div>

      {selectedThread && (
        <>
          <div className="status-sep" />
          <div className="status-item">
            Thread: {selectedThread.label || 'Unnamed'}
          </div>
          <div className="status-item">
            sim {(selectedThread.similarity * 100).toFixed(0)}%
          </div>
          <div className="status-item">2 clusters</div>
        </>
      )}
    </div>
  );
}
