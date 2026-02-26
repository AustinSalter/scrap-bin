import { useState, useRef, useEffect } from 'react';
import type { ClusterView } from '../types';
import { useAppStore } from '../stores/appStore';

const MIME_FRAGMENT = 'application/x-scrapbin-fragment';
const MIME_CLUSTER = 'application/x-scrapbin-cluster';

interface Props {
  cluster: ClusterView;
  x: number;
  y: number;
  isHighlighted: boolean;
}

export function ClusterNode({ cluster, x, y, isHighlighted }: Props) {
  const selectedClusterId = useAppStore((s) => s.selectedClusterId);
  const goBrowsing = useAppStore((s) => s.goBrowsing);
  const editingClusterId = useAppStore((s) => s.editingClusterId);
  const setEditingCluster = useAppStore((s) => s.setEditingCluster);
  const renameCluster = useAppStore((s) => s.renameCluster);
  const moveFragment = useAppStore((s) => s.moveFragment);
  const mergeClusters = useAppStore((s) => s.mergeClusters);
  const setDragContext = useAppStore((s) => s.setDragContext);
  const clearDragContext = useAppStore((s) => s.clearDragContext);

  const [isDragOver, setIsDragOver] = useState(false);
  const [isDragging, setIsDragging] = useState(false);
  const [renameValue, setRenameValue] = useState(cluster.displayLabel);
  const inputRef = useRef<HTMLInputElement>(null);

  const isActive = selectedClusterId === cluster.label;
  const isEditing = editingClusterId === cluster.label;
  const diameter = Math.min(80, Math.max(28, cluster.size * 3));

  useEffect(() => {
    if (isEditing && inputRef.current) {
      inputRef.current.focus();
      inputRef.current.select();
    }
  }, [isEditing]);

  useEffect(() => {
    setRenameValue(cluster.displayLabel);
  }, [cluster.displayLabel]);

  const handleDoubleClickLabel = (e: React.MouseEvent) => {
    e.stopPropagation();
    setEditingCluster(cluster.label);
  };

  const handleRenameKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      const trimmed = renameValue.trim();
      if (trimmed && trimmed !== cluster.displayLabel) {
        renameCluster(cluster.label, trimmed);
      } else {
        setEditingCluster(null);
      }
    } else if (e.key === 'Escape') {
      setRenameValue(cluster.displayLabel);
      setEditingCluster(null);
    }
  };

  const handleRenameBlur = () => {
    const trimmed = renameValue.trim();
    if (trimmed && trimmed !== cluster.displayLabel) {
      renameCluster(cluster.label, trimmed);
    } else {
      setEditingCluster(null);
    }
  };

  // Drag source (cluster merge)
  const handleDragStart = (e: React.DragEvent) => {
    e.dataTransfer.setData(MIME_CLUSTER, JSON.stringify({ id: cluster.label }));
    e.dataTransfer.effectAllowed = 'move';
    setIsDragging(true);
    setDragContext({ type: 'cluster', id: String(cluster.label) });
  };

  const handleDragEnd = () => {
    setIsDragging(false);
    clearDragContext();
  };

  // Drop target (fragment move or cluster merge)
  const handleDragOver = (e: React.DragEvent) => {
    e.preventDefault();
    e.dataTransfer.dropEffect = 'move';
    setIsDragOver(true);
  };

  const handleDragLeave = () => {
    setIsDragOver(false);
  };

  const handleDrop = (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragOver(false);

    // Check for fragment drop.
    const fragData = e.dataTransfer.getData(MIME_FRAGMENT);
    if (fragData) {
      try {
        const parsed = JSON.parse(fragData);
        if (parsed.fromCluster !== cluster.label) {
          moveFragment(parsed.id, parsed.fromCluster, cluster.label);
        }
      } catch { /* ignore malformed data */ }
      return;
    }

    // Check for cluster merge drop.
    const clusterData = e.dataTransfer.getData(MIME_CLUSTER);
    if (clusterData) {
      try {
        const parsed = JSON.parse(clusterData);
        if (parsed.id !== cluster.label) {
          mergeClusters([parsed.id, cluster.label]);
        }
      } catch { /* ignore malformed data */ }
    }
  };

  const classNames = [
    'c-node',
    isActive && 'is-active',
    isHighlighted && 'is-highlighted',
    isDragOver && 'is-drop-target',
    isDragging && 'is-drag-source',
  ]
    .filter(Boolean)
    .join(' ');

  return (
    <div
      className={classNames}
      style={{
        left: `${x * 100}%`,
        top: `${y * 100}%`,
      }}
      onClick={() => goBrowsing(cluster.label)}
      draggable
      onDragStart={handleDragStart}
      onDragEnd={handleDragEnd}
      onDragOver={handleDragOver}
      onDragLeave={handleDragLeave}
      onDrop={handleDrop}
    >
      <div
        className="c-node-ring"
        style={{ width: diameter, height: diameter }}
      >
        <span className="c-node-count">{cluster.size}</span>
      </div>
      {isEditing ? (
        <input
          ref={inputRef}
          className="c-node-label-input"
          value={renameValue}
          onChange={(e) => setRenameValue(e.target.value)}
          onKeyDown={handleRenameKeyDown}
          onBlur={handleRenameBlur}
          onClick={(e) => e.stopPropagation()}
        />
      ) : (
        <div className="c-node-label" onDoubleClick={handleDoubleClickLabel}>
          {cluster.displayLabel}
        </div>
      )}
    </div>
  );
}
