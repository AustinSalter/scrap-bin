import { useMemo, useState, useRef, useEffect } from 'react';
import { useAppStore } from '../stores/appStore';

export function ClusterEdges() {
  const clusterPositions = useAppStore((s) => s.clusterPositions);
  const threads = useAppStore((s) => s.threads);
  const streamItems = useAppStore((s) => s.streamItems);
  const selectedClusterFragments = useAppStore((s) => s.selectedClusterFragments);
  const uiState = useAppStore((s) => s.uiState);
  const highlightedClusterIds = useAppStore((s) => s.highlightedClusterIds);
  const editingThreadId = useAppStore((s) => s.editingThreadId);
  const setEditingThread = useAppStore((s) => s.setEditingThread);
  const renameThread = useAppStore((s) => s.renameThread);

  // Derive edges from threads.
  const edges = useMemo(
    () =>
      threads.map((t) => ({
        sourceClusterId: t.sourceClusterId,
        targetClusterId: t.targetClusterId,
        thread: t,
      })),
    [threads]
  );

  // Derive new-item pulse dots from stream items marked as new.
  const newDots = useMemo(() => {
    const clusterIds = new Set(
      streamItems.filter((i) => i.isNew).map((i) => i.clusterId)
    );
    const dots: { x: number; y: number }[] = [];
    for (const cid of clusterIds) {
      const pos = clusterPositions.get(cid);
      if (pos) {
        dots.push({ x: pos.x + 0.03, y: pos.y - 0.04 });
      }
    }
    return dots;
  }, [streamItems, clusterPositions]);

  // Derive your-note diamond markers from clusters that have user notes.
  const noteMarkers = useMemo(() => {
    const markers: { x: number; y: number }[] = [];
    const noteClusterIds = new Set(
      selectedClusterFragments.filter((f) => f.isYourNote).map((f) => f.clusterId)
    );
    for (const cid of noteClusterIds) {
      const pos = clusterPositions.get(cid);
      if (pos) {
        markers.push({ x: pos.x + 0.03, y: pos.y + 0.04 });
      }
    }
    return markers;
  }, [selectedClusterFragments, clusterPositions]);

  const isThreaded = uiState === 'threaded';

  return (
    <div className="c-edges-layer">
      {/* SVG edges */}
      <svg className="c-edges-svg">
        {edges.map((edge, i) => {
          const a = clusterPositions.get(edge.sourceClusterId);
          const b = clusterPositions.get(edge.targetClusterId);
          if (!a || !b) return null;
          const edgeHighlighted =
            isThreaded &&
            highlightedClusterIds.includes(edge.sourceClusterId) &&
            highlightedClusterIds.includes(edge.targetClusterId);
          return (
            <line
              key={i}
              x1={`${a.x * 100}%`}
              y1={`${a.y * 100}%`}
              x2={`${b.x * 100}%`}
              y2={`${b.y * 100}%`}
              className={`c-edge${edgeHighlighted ? ' is-highlighted' : ''}`}
            />
          );
        })}
      </svg>

      {/* Thread label pills (only in threaded state) */}
      {isThreaded &&
        edges.map((edge) => {
          const a = clusterPositions.get(edge.sourceClusterId);
          const b = clusterPositions.get(edge.targetClusterId);
          if (!a || !b) return null;
          const edgeHighlighted =
            highlightedClusterIds.includes(edge.sourceClusterId) &&
            highlightedClusterIds.includes(edge.targetClusterId);
          if (!edgeHighlighted) return null;

          const midX = ((a.x + b.x) / 2) * 100;
          const midY = ((a.y + b.y) / 2) * 100;

          return (
            <ThreadPill
              key={`pill-${edge.thread.id}`}
              threadId={edge.thread.id}
              label={edge.thread.label}
              x={midX}
              y={midY}
              isEditing={editingThreadId === edge.thread.id}
              setEditingThread={setEditingThread}
              renameThread={renameThread}
            />
          );
        })}

      {/* New-item pulse dots */}
      {newDots.map((pos, i) => (
        <div
          key={`new-${i}`}
          className="c-new-dot"
          style={{ left: `${pos.x * 100}%`, top: `${pos.y * 100}%` }}
        />
      ))}

      {/* Your-note diamond markers */}
      {noteMarkers.map((pos, i) => (
        <div
          key={`note-${i}`}
          className="c-note-marker"
          style={{ left: `${pos.x * 100}%`, top: `${pos.y * 100}%` }}
        />
      ))}
    </div>
  );
}

// ── Thread pill sub-component ──

interface ThreadPillProps {
  threadId: string;
  label: string;
  x: number;
  y: number;
  isEditing: boolean;
  setEditingThread: (id: string | null) => void;
  renameThread: (id: string, label: string) => Promise<void>;
}

function ThreadPill({
  threadId,
  label,
  x,
  y,
  isEditing,
  setEditingThread,
  renameThread,
}: ThreadPillProps) {
  const [value, setValue] = useState(label);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (isEditing && inputRef.current) {
      inputRef.current.focus();
      inputRef.current.select();
    }
  }, [isEditing]);

  useEffect(() => {
    setValue(label);
  }, [label]);

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      const trimmed = value.trim();
      if (trimmed && trimmed !== label) {
        renameThread(threadId, trimmed);
      } else {
        setEditingThread(null);
      }
    } else if (e.key === 'Escape') {
      setValue(label);
      setEditingThread(null);
    }
  };

  const handleBlur = () => {
    const trimmed = value.trim();
    if (trimmed && trimmed !== label) {
      renameThread(threadId, trimmed);
    } else {
      setEditingThread(null);
    }
  };

  if (isEditing) {
    return (
      <div
        className="c-thread-pill"
        style={{ left: `${x}%`, top: `${y}%` }}
      >
        <input
          ref={inputRef}
          className="c-node-label-input"
          value={value}
          onChange={(e) => setValue(e.target.value)}
          onKeyDown={handleKeyDown}
          onBlur={handleBlur}
          style={{ fontSize: '11px', width: '100px' }}
        />
      </div>
    );
  }

  return (
    <div
      className="c-thread-pill"
      style={{ left: `${x}%`, top: `${y}%` }}
      onDoubleClick={(e) => {
        e.stopPropagation();
        setEditingThread(threadId);
      }}
    >
      {label || 'Thread'}
    </div>
  );
}
