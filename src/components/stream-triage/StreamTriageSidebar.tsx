import { useEffect, useRef } from 'react';
import { useAppStore } from '../../stores/appStore';
import { MarkPips } from '../shared/MarkPips';
import type { Fragment, TriageTab } from '../../types';
import { deriveTitle } from '../../utils/fragments';
import '../../styles/stream-triage.css';

const SOURCE_COLORS: Record<string, string> = {
  vault: 'var(--signal-purple)',
  twitter: 'var(--signal-blue)',
  readwise: 'var(--signal-green)',
  podcast: 'var(--signal-amber)',
  rss: 'var(--accent)',
  apple_notes: 'var(--signal-red)',
};

const DISPOSITION_INDICATORS: Record<string, string> = {
  signal: 'var(--signal-green)',
  inbox: 'var(--ghost)',
  ignored: 'var(--rule)',
};

function formatTime(ts: number): string {
  if (!ts) return '';
  const d = new Date(ts);
  const now = new Date();
  const diffMs = now.getTime() - d.getTime();
  const diffDays = Math.floor(diffMs / 86400000);
  if (diffDays === 0) {
    return d.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
  }
  if (diffDays < 7) return `${diffDays}d`;
  return d.toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
}

function SidebarItem({ fragment, isSelected }: { fragment: Fragment; isSelected: boolean }) {
  const setTriageSelectedId = useAppStore((s) => s.setTriageSelectedId);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (isSelected && ref.current) {
      ref.current.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
    }
  }, [isSelected]);

  const title = deriveTitle(fragment);

  return (
    <div
      ref={ref}
      className={`triage-sidebar-item${isSelected ? ' is-selected' : ''}`}
      onClick={() => setTriageSelectedId(fragment.id)}
    >
      <div className="triage-sidebar-item-top">
        <span
          className="triage-sidebar-item-dot"
          style={{ background: SOURCE_COLORS[fragment.sourceType] ?? 'var(--ghost)' }}
        />
        <span className="triage-sidebar-item-source">{fragment.sourceLabel}</span>
        <span className="triage-sidebar-item-time">{formatTime(fragment.timestamp)}</span>
      </div>
      <div className="triage-sidebar-item-title">{title}</div>
      <MarkPips highlights={fragment.highlights} />
      <div className="triage-sidebar-item-bottom">
        {fragment.clusterId >= 0 && (
          <span className="triage-sidebar-item-cluster">c{fragment.clusterId}</span>
        )}
        <span
          className="triage-sidebar-item-disposition"
          style={{ background: DISPOSITION_INDICATORS[fragment.disposition] }}
          title={fragment.disposition}
        />
      </div>
    </div>
  );
}

const TABS: { key: TriageTab; label: string }[] = [
  { key: 'all', label: 'All' },
  { key: 'signal', label: 'Signal' },
  { key: 'inbox', label: 'Inbox' },
];

export function StreamTriageSidebar() {
  const triageTab = useAppStore((s) => s.triageTab);
  const triageFragments = useAppStore((s) => s.triageFragments);
  const triageSelectedId = useAppStore((s) => s.triageSelectedId);
  const triageTotal = useAppStore((s) => s.triageTotal);
  const triageCounts = useAppStore((s) => s.triageCounts);
  const triageLoading = useAppStore((s) => s.triageLoading);
  const setTriageTab = useAppStore((s) => s.setTriageTab);
  const goInboxTriage = useAppStore((s) => s.goInboxTriage);

  function countForTab(tab: TriageTab): number {
    if (tab === 'all') return triageCounts.signal + triageCounts.inbox + triageCounts.ignored;
    return triageCounts[tab];
  }

  return (
    <div className="triage-sidebar">
      <div className="triage-sidebar-header">
        <span className="triage-sidebar-title">Stream</span>
        <span className="triage-sidebar-count">{triageTotal}</span>
      </div>

      <div className="triage-tabs">
        {TABS.map((tab) => (
          <button
            key={tab.key}
            className={`triage-tab${triageTab === tab.key ? ' is-active' : ''}`}
            onClick={() => setTriageTab(tab.key)}
          >
            {tab.label}
            <span className="triage-tab-count">{countForTab(tab.key)}</span>
          </button>
        ))}
      </div>

      {triageTab === 'inbox' && (
        <button className="card-triage-btn" onClick={goInboxTriage}>
          Card Triage
        </button>
      )}

      <div className="triage-sidebar-body">
        {triageLoading && triageFragments.length === 0 ? (
          <div className="triage-sidebar-empty">Loading...</div>
        ) : triageFragments.length === 0 ? (
          <div className="triage-sidebar-empty">No fragments</div>
        ) : (
          triageFragments.map((f) => (
            <SidebarItem
              key={f.id}
              fragment={f}
              isSelected={f.id === triageSelectedId}
            />
          ))
        )}
      </div>
    </div>
  );
}
