import { useAppStore } from '../../stores/appStore';

export function TriageBar() {
  const triageSelectedId = useAppStore((s) => s.triageSelectedId);
  const triageFragments = useAppStore((s) => s.triageFragments);
  const triageDisposition = useAppStore((s) => s.triageDisposition);

  const selected = triageFragments.find((f) => f.id === triageSelectedId);
  const disabled = !selected;

  return (
    <div className="triage-bar">
      <button
        className={`triage-action${selected?.disposition === 'signal' ? ' is-current' : ''}`}
        disabled={disabled}
        onClick={() => triageSelectedId && triageDisposition(triageSelectedId, 'signal')}
      >
        Add to Signal
        <span className="triage-action-key">&#8984;L</span>
      </button>
      <button
        className={`triage-action${selected?.disposition === 'inbox' ? ' is-current' : ''}`}
        disabled={disabled}
        onClick={() => triageSelectedId && triageDisposition(triageSelectedId, 'inbox')}
      >
        Move to Inbox
        <span className="triage-action-key">&#8984;I</span>
      </button>
      <button
        className={`triage-action triage-action--ignore${selected?.disposition === 'ignored' ? ' is-current' : ''}`}
        disabled={disabled}
        onClick={() => triageSelectedId && triageDisposition(triageSelectedId, 'ignored')}
      >
        Ignore
        <span className="triage-action-key">&#8963;X</span>
      </button>
    </div>
  );
}
