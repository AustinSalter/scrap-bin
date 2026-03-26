import { Toolbar } from './Toolbar';
import { StatusBar } from './StatusBar';
import { ClusterEdges } from './ClusterEdges';
import { ClusterNode } from './ClusterNode';
import { ClusterGrid } from './clusters/ClusterGrid';
import { useAppStore } from '../stores/appStore';
import '../styles/landscape.css';

export function Landscape() {
  const clusters = useAppStore((s) => s.clusters);
  const clusterPositions = useAppStore((s) => s.clusterPositions);
  const uiState = useAppStore((s) => s.uiState);
  const highlightedClusterIds = useAppStore((s) => s.highlightedClusterIds);
  const clusterViewMode = useAppStore((s) => s.clusterViewMode);

  const isThreaded = uiState === 'threaded';

  return (
    <div className="landscape">
      <Toolbar />
      {clusterViewMode === 'grid' ? (
        <ClusterGrid />
      ) : (
        <div className={`landscape-canvas${isThreaded ? ' is-threaded' : ''}`}>
          <div className="cluster-map">
            <ClusterEdges />
            {clusters.map((cluster) => {
              const pos = clusterPositions.get(cluster.label);
              if (!pos) return null;
              const isHighlighted =
                highlightedClusterIds.length === 0 ||
                highlightedClusterIds.includes(cluster.label);
              return (
                <ClusterNode
                  key={cluster.label}
                  cluster={cluster}
                  x={pos.x}
                  y={pos.y}
                  isHighlighted={isHighlighted}
                />
              );
            })}
          </div>
        </div>
      )}
      <StatusBar />
    </div>
  );
}
