import type { Fragment } from '../types';

export function deriveTitle(fragment: Fragment): string {
  const meta = fragment.metadata;
  switch (fragment.sourceType) {
    case 'rss': {
      const t = typeof meta.entry_title === 'string' ? meta.entry_title : '';
      if (t) return t;
      break;
    }
    case 'readwise': {
      const t = typeof meta.book_title === 'string' ? meta.book_title : '';
      if (t) return t;
      break;
    }
    case 'apple_notes': {
      const p = typeof meta.source_path === 'string' ? meta.source_path : '';
      const name = p.split('/').pop()?.replace(/\.[^.]+$/, '');
      if (name) return name;
      break;
    }
    case 'podcast': {
      const p = typeof meta.source_path === 'string' ? meta.source_path : '';
      const name = p.replace(/\.[^.]+$/, '');
      if (name) return name;
      break;
    }
  }
  return fragment.content.slice(0, 100).replace(/\n/g, ' ').trim() || 'Untitled';
}
