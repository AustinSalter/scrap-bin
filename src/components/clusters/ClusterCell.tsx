import type { ClusterView, Fragment } from '../../types';
import { useAppStore } from '../../stores/appStore';
import { FragmentPreview } from './FragmentPreview';

const CLUSTER_COLORS = [
  'var(--signal-blue)',
  'var(--signal-green)',
  'var(--signal-amber)',
  'var(--signal-purple)',
  'var(--signal-red)',
  'var(--accent)',
];

interface ClusterCellProps {
  cluster: ClusterView;
  fragments: Fragment[];
}

export function ClusterCell({ cluster, fragments }: ClusterCellProps) {
  const goBrowsing = useAppStore((s) => s.goBrowsing);
  const color = CLUSTER_COLORS[cluster.label % CLUSTER_COLORS.length];
  const top3 = fragments.slice(0, 3);
  const remaining = cluster.size - top3.length;

  return (
    <div className="cluster-cell" onClick={() => goBrowsing(cluster.label)}>
      <div className="cluster-cell-header">
        <span className="cluster-cell-bar" style={{ background: color }} />
        <span className="cluster-cell-name">{cluster.displayLabel}</span>
        <span className="cluster-cell-count">{cluster.size}</span>
      </div>
      <div className="cluster-cell-fragments">
        {top3.map((f) => (
          <FragmentPreview key={f.id} fragment={f} />
        ))}
      </div>
      {remaining > 0 && (
        <div className="cluster-cell-more">+{remaining} more</div>
      )}
    </div>
  );
}
