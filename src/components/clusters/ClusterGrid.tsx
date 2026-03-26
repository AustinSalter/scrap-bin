import { useEffect, useState } from 'react';
import { useAppStore } from '../../stores/appStore';
import { clusteringGetFragments } from '../../api/commands';
import { ClusterCell } from './ClusterCell';
import type { ClusterView, Fragment } from '../../types';
import '../../styles/cluster-grid.css';

/** Fetch top 3 fragments for each cluster. */
async function fetchClusterPreviews(
  clusters: ClusterView[],
): Promise<Map<number, Fragment[]>> {
  const map = new Map<number, Fragment[]>();
  // Fetch in parallel, limit to first 3 per cluster
  const results = await Promise.allSettled(
    clusters.map(async (c) => {
      const fragments = await clusteringGetFragments(c.label);
      return { label: c.label, fragments: fragments.slice(0, 3) };
    }),
  );
  for (const result of results) {
    if (result.status === 'fulfilled') {
      map.set(result.value.label, result.value.fragments);
    }
  }
  return map;
}

export function ClusterGrid() {
  const clusters = useAppStore((s) => s.clusters);
  const [previews, setPreviews] = useState<Map<number, Fragment[]>>(new Map());
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    if (clusters.length === 0) return;
    let cancelled = false;
    setLoading(true);
    fetchClusterPreviews(clusters).then((map) => {
      if (!cancelled) {
        setPreviews(map);
        setLoading(false);
      }
    });
    return () => { cancelled = true; };
  }, [clusters]);

  // Sort by priority: clusters with more high-priority highlights first
  const sorted = [...clusters].sort((a, b) => {
    const aFrags = previews.get(a.label) ?? [];
    const bFrags = previews.get(b.label) ?? [];
    const aScore = aFrags.reduce((s, f) =>
      s + f.highlights.filter((h) => h.priority <= 2).length, 0);
    const bScore = bFrags.reduce((s, f) =>
      s + f.highlights.filter((h) => h.priority <= 2).length, 0);
    return bScore - aScore || b.size - a.size;
  });

  if (loading && previews.size === 0) {
    return <div className="cluster-grid-loading">Loading clusters...</div>;
  }

  if (clusters.length === 0) {
    return <div className="cluster-grid-empty">No clusters yet. Run clustering first.</div>;
  }

  return (
    <div className="cluster-grid">
      {sorted.map((cluster) => (
        <ClusterCell
          key={cluster.label}
          cluster={cluster}
          fragments={previews.get(cluster.label) ?? []}
        />
      ))}
    </div>
  );
}
