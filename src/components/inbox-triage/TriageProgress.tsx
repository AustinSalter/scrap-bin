import { useAppStore } from '../../stores/appStore';

export function TriageProgress() {
  const triaged = useAppStore((s) => s.inboxTriagedCount);
  const total = useAppStore((s) => s.inboxTotalCount);

  const pct = total > 0 ? Math.round((triaged / total) * 100) : 0;

  return (
    <div className="inbox-progress">
      <div className="inbox-progress-text">
        <span>{triaged} of {total} triaged</span>
        <span>{pct}%</span>
      </div>
      <div className="progress-bar-track">
        <div className="progress-bar-fill" style={{ width: `${pct}%` }} />
      </div>
    </div>
  );
}
