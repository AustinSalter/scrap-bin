import { StreamTriageSidebar } from './StreamTriageSidebar';
import { FragmentDetail } from './FragmentDetail';
import { TriageBar } from './TriageBar';
import '../../styles/stream-triage.css';

export function StreamTriageView() {
  return (
    <div className="triage-container">
      <div className="triage-layout">
        <StreamTriageSidebar />
        <FragmentDetail />
      </div>
      <TriageBar />
    </div>
  );
}
