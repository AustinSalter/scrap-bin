import { useCallback, useEffect, useRef, useState } from 'react';
import { open } from '@tauri-apps/plugin-shell';
import { useAppStore } from '../../stores/appStore';
import { HighlightableText } from '../shared/HighlightableText';
import { HighlightGutter } from '../shared/HighlightGutter';
import type { HighlightRange } from '../../types';
import '../../styles/reader.css';

function formatReadTime(wordCount: number): string {
  const mins = Math.max(1, Math.round(wordCount / 230));
  return `${mins} min read`;
}

function SourcePill({ url }: { url: string }) {
  let domain = '';
  try { domain = new URL(url).hostname.replace('www.', ''); } catch { /* */ }

  return (
    <span className="reader-source-pill">
      <span className="reader-source-icon">{domain.charAt(0).toUpperCase()}</span>
      {domain}
    </span>
  );
}

export function Reader() {
  const reader = useAppStore((s) => s.activeReader);
  const closeReader = useAppStore((s) => s.closeReader);
  const saveHighlights = useAppStore((s) => s.saveHighlights);
  const bodyRef = useRef<HTMLDivElement>(null);
  const [scrollPct, setScrollPct] = useState(0);

  const handleScroll = useCallback(() => {
    if (!bodyRef.current) return;
    const { scrollTop, scrollHeight, clientHeight } = bodyRef.current;
    const pct = scrollHeight <= clientHeight ? 100 : (scrollTop / (scrollHeight - clientHeight)) * 100;
    setScrollPct(pct);
  }, []);

  useEffect(() => {
    const el = bodyRef.current;
    if (!el) return;
    el.addEventListener('scroll', handleScroll, { passive: true });
    return () => el.removeEventListener('scroll', handleScroll);
  }, [handleScroll]);

  const handleHighlightSave = useCallback(
    (highlights: HighlightRange[]) => {
      if (reader?.fragmentId) saveHighlights(reader.fragmentId, highlights);
    },
    [reader, saveHighlights],
  );

  if (!reader) return null;

  const { article, highlights } = reader;

  return (
    <div className="reader">
      {/* Progress bar */}
      <div className="reader-progress">
        <div className="reader-progress-fill" style={{ width: `${scrollPct}%` }} />
      </div>

      {/* Toolbar */}
      <div className="reader-toolbar">
        <button className="reader-back" onClick={closeReader} title="Back (Escape)">
          {'\u2190'}
        </button>
        <SourcePill url={article.url} />
        <div className="reader-toolbar-spacer" />
        <span className="reader-meta">
          {article.word_count.toLocaleString()} words · {formatReadTime(article.word_count)}
        </span>
        <button className="reader-action" onClick={() => open(article.url)}>
          Original {'\u2197'}
        </button>
      </div>

      {/* Scroll body */}
      <div className="reader-body" ref={bodyRef}>
        <div className="reader-content">
          {/* Article header */}
          <div className="reader-article-header">
            {article.title && (
              <h1 className="reader-article-title">{article.title}</h1>
            )}
          </div>

          {/* Article text with highlights */}
          <HighlightableText
            content={article.text}
            highlights={highlights}
            onHighlightSave={handleHighlightSave}
            className="reader-body-text"
          />
        </div>

        {/* Gutter minimap */}
        {highlights.length > 0 && (
          <HighlightGutter
            highlights={highlights}
            contentLength={article.text.length}
          />
        )}
      </div>
    </div>
  );
}
