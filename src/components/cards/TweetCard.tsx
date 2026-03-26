import { useState } from 'react';
import { open } from '@tauri-apps/plugin-shell';
import { HighlightableText } from '../shared/HighlightableText';
import type { Fragment, HighlightRange } from '../../types';
import '../../styles/tweet-card.css';

function str(value: unknown): string {
  return typeof value === 'string' ? value : '';
}

function num(value: unknown): number {
  return typeof value === 'number' ? value : 0;
}

function formatCount(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return n > 0 ? String(n) : '';
}

function formatDate(ts: number): string {
  if (!ts) return '';
  return new Date(ts).toLocaleDateString(undefined, {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
  });
}

function Avatar({ name, avatarUrl }: { name: string; avatarUrl: string }) {
  const [failed, setFailed] = useState(false);
  const initial = name.charAt(0).toUpperCase() || '?';

  if (!avatarUrl || failed) {
    return <div className="tweet-avatar tweet-avatar-initial">{initial}</div>;
  }

  return (
    <img
      className="tweet-avatar"
      src={avatarUrl}
      alt={name}
      onError={() => setFailed(true)}
    />
  );
}

interface TweetCardProps {
  fragment: Fragment;
  onHighlightSave: (highlights: HighlightRange[]) => void;
}

export function TweetCard({ fragment, onHighlightSave }: TweetCardProps) {
  const meta = fragment.metadata;
  const authorName = str(meta.author_name);
  const authorHandle = str(meta.author_handle);
  const avatarUrl = str(meta.author_avatar_url);
  const tweetUrl = str(meta.tweet_url) || str(meta.original_url);
  const likes = num(meta.like_count);
  const retweets = num(meta.retweet_count);
  const replies = num(meta.reply_count);
  const quotedText = str(meta.quoted_tweet_text);

  return (
    <div className="tweet-card">
      {/* Header: avatar + name/handle */}
      <div className="tweet-card-head">
        <Avatar name={authorName || authorHandle} avatarUrl={avatarUrl} />
        <div className="tweet-card-author">
          {authorName && <span className="tweet-card-name">{authorName}</span>}
          {authorHandle && <span className="tweet-card-handle">@{authorHandle}</span>}
        </div>
      </div>

      {/* Body: tweet text with highlight support */}
      <HighlightableText
        content={fragment.content}
        highlights={fragment.highlights}
        onHighlightSave={onHighlightSave}
        className="tweet-card-text"
      />

      {/* Quoted tweet (if present) */}
      {quotedText && (
        <div className="tweet-card-quote">
          <div className="tweet-card-quote-text">{quotedText}</div>
        </div>
      )}

      {/* Footer: metrics + date + original link */}
      <div className="tweet-card-footer">
        {likes > 0 && <span className="tweet-card-metric">{'\u2661'} {formatCount(likes)}</span>}
        {retweets > 0 && <span className="tweet-card-metric">{'\u21BA'} {formatCount(retweets)}</span>}
        {replies > 0 && <span className="tweet-card-metric">{'\u21A9'} {formatCount(replies)}</span>}
        <span className="tweet-card-date">{formatDate(fragment.timestamp)}</span>
        {tweetUrl && (
          <button className="tweet-card-link" onClick={() => open(tweetUrl)}>
            View original
          </button>
        )}
      </div>
    </div>
  );
}
