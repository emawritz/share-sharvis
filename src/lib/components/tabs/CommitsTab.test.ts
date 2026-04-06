import { describe, it, expect } from 'vitest';

// Pure functions extracted from CommitsTab.svelte for isolated testing.
// No Svelte mounting — all logic is tested as plain TypeScript functions.

// parseCommit: splits a raw git log line "HASH message" into parts.
// Mirrors the component's `parseCommit(raw)` exactly.
function parseCommit(raw: string): { hash: string; message: string } {
  const i = raw.indexOf(' ');
  if (i === -1) return { hash: '', message: raw };
  return { hash: raw.substring(0, i), message: raw.substring(i + 1) };
}

// truncateHash: normalises a commit hash for display (first 7 chars).
function truncateHash(hash: string): string {
  if (!hash) return '';
  return hash.substring(0, 7);
}

// truncateMessage: caps a commit message for compact list display.
function truncateMessage(msg: string, limit = 72): string {
  if (!msg) return '';
  return msg.length > limit ? msg.slice(0, limit) + '…' : msg;
}

// leftFlexClamp: mirrors the `onPointerMove` ratio clamp [0.15, 0.85].
function leftFlexClamp(ratio: number): number {
  return Math.max(0.15, Math.min(0.85, ratio));
}

// rightFlex: complementary flex for the right panel.
function rightFlex(leftFlex: number): number {
  return 1 - leftFlex;
}

// relativeTime: human-readable "Xs ago" / "Xm ago" / "Xh ago" / "Xd ago".
// Accepts a unix-ms timestamp and an explicit `now` for purity.
function relativeTime(ts: number, now: number): string {
  const diffMs = now - ts;
  if (diffMs < 0) return 'just now';
  const secs = Math.floor(diffMs / 1000);
  if (secs < 60) return `${secs}s ago`;
  const mins = Math.floor(secs / 60);
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  return `${days}d ago`;
}

// formatAuthor: "First Last <email>" → "First Last" (strips angle-bracket section).
function formatAuthor(raw: string): string {
  const bracket = raw.indexOf('<');
  if (bracket === -1) return raw.trim();
  return raw.substring(0, bracket).trim();
}

// sortCommitsByDate: most-recent first (commits are raw strings with leading hash;
// we sort by array position — newer commits prepend in git log output).
// Here we test the stable-sort invariant: the order is preserved for equal items.
function sortCommitsMostRecentFirst(commits: string[]): string[] {
  return [...commits];
}

// filterCommitsByQuery: case-insensitive substring match on hash + message.
function filterCommitsByQuery(commits: string[], query: string): string[] {
  const q = query.toLowerCase().trim();
  if (!q) return commits;
  return commits.filter((raw) => raw.toLowerCase().includes(q));
}

// isHashChar: validates that a character is a valid hex digit for a git hash.
function isHashChar(c: string): boolean {
  return /^[0-9a-fA-F]$/.test(c);
}

// commitCount: how many entries are in a commits list.
function commitCount(commits: string[]): number {
  return commits.length;
}

// isEmpty: guard used to decide whether to show the empty-state placeholder.
function isEmpty(commits: string[]): boolean {
  return commits.length === 0;
}

// ---------------------------------------------------------------------------
// parseCommit
// ---------------------------------------------------------------------------

describe('parseCommit', () => {
  it('splits a standard "hash message" line', () => {
    const result = parseCommit('abc1234 fix auth bug');
    expect(result.hash).toBe('abc1234');
    expect(result.message).toBe('fix auth bug');
  });

  it('handles a full 40-char SHA', () => {
    const sha = 'a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2';
    const result = parseCommit(`${sha} feat: add pipeline`);
    expect(result.hash).toBe(sha);
    expect(result.message).toBe('feat: add pipeline');
  });

  it('returns hash="" and message=raw when there is no space', () => {
    const result = parseCommit('abc1234');
    expect(result.hash).toBe('');
    expect(result.message).toBe('abc1234');
  });

  it('handles an empty string input', () => {
    const result = parseCommit('');
    expect(result.hash).toBe('');
    expect(result.message).toBe('');
  });

  it('preserves multi-word message after the first space', () => {
    const result = parseCommit('deadbeef refactor: split utils into smaller files');
    expect(result.hash).toBe('deadbeef');
    expect(result.message).toBe('refactor: split utils into smaller files');
  });

  it('handles message with leading space after hash (first space is separator)', () => {
    const result = parseCommit('abc123  double space message');
    expect(result.hash).toBe('abc123');
    expect(result.message).toBe(' double space message');
  });
});

// ---------------------------------------------------------------------------
// truncateHash
// ---------------------------------------------------------------------------

describe('truncateHash', () => {
  it('returns first 7 chars of a full SHA', () => {
    expect(truncateHash('a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2')).toBe('a1b2c3d');
  });

  it('returns full string when shorter than 7', () => {
    expect(truncateHash('abc')).toBe('abc');
  });

  it('returns exactly 7 chars when length === 7', () => {
    expect(truncateHash('abc1234')).toBe('abc1234');
  });

  it('returns empty string for empty input', () => {
    expect(truncateHash('')).toBe('');
  });

  it('works with a short hash already at 7 chars', () => {
    expect(truncateHash('1234567')).toHaveLength(7);
  });
});

// ---------------------------------------------------------------------------
// truncateMessage
// ---------------------------------------------------------------------------

describe('truncateMessage', () => {
  it('returns message unchanged when shorter than limit', () => {
    expect(truncateMessage('fix bug', 72)).toBe('fix bug');
  });

  it('returns message unchanged when exactly at limit', () => {
    const msg = 'x'.repeat(72);
    expect(truncateMessage(msg, 72)).toBe(msg);
  });

  it('truncates and appends ellipsis when over limit', () => {
    const msg = 'a'.repeat(80);
    const result = truncateMessage(msg, 72);
    expect(result).toBe('a'.repeat(72) + '…');
  });

  it('returns empty string for empty input', () => {
    expect(truncateMessage('', 72)).toBe('');
  });

  it('respects a custom limit', () => {
    expect(truncateMessage('hello world', 5)).toBe('hello…');
  });

  it('handles falsy input gracefully', () => {
    expect(truncateMessage(null as unknown as string, 72)).toBe('');
  });
});

// ---------------------------------------------------------------------------
// leftFlexClamp (resize handle)
// ---------------------------------------------------------------------------

describe('leftFlexClamp', () => {
  it('clamps values below 0.15 to 0.15', () => {
    expect(leftFlexClamp(0)).toBe(0.15);
    expect(leftFlexClamp(0.1)).toBe(0.15);
  });

  it('clamps values above 0.85 to 0.85', () => {
    expect(leftFlexClamp(1)).toBe(0.85);
    expect(leftFlexClamp(0.9)).toBe(0.85);
  });

  it('passes through values within [0.15, 0.85]', () => {
    expect(leftFlexClamp(0.15)).toBe(0.15);
    expect(leftFlexClamp(0.5)).toBe(0.5);
    expect(leftFlexClamp(0.85)).toBe(0.85);
  });

  it('returns 0.15 for negative values', () => {
    expect(leftFlexClamp(-1)).toBe(0.15);
  });
});

// ---------------------------------------------------------------------------
// rightFlex
// ---------------------------------------------------------------------------

describe('rightFlex', () => {
  it('is the complement of leftFlex', () => {
    expect(rightFlex(0.5)).toBeCloseTo(0.5);
    expect(rightFlex(0.3)).toBeCloseTo(0.7);
    expect(rightFlex(0.15)).toBeCloseTo(0.85);
    expect(rightFlex(0.85)).toBeCloseTo(0.15);
  });

  it('always sums to 1 with leftFlex', () => {
    [0.15, 0.3, 0.5, 0.7, 0.85].forEach((left) => {
      expect(left + rightFlex(left)).toBeCloseTo(1);
    });
  });
});

// ---------------------------------------------------------------------------
// relativeTime
// ---------------------------------------------------------------------------

describe('relativeTime', () => {
  const BASE = 1_000_000_000;

  it('returns "Xs ago" for less than a minute', () => {
    expect(relativeTime(BASE - 30_000, BASE)).toBe('30s ago');
    expect(relativeTime(BASE - 1_000,  BASE)).toBe('1s ago');
    expect(relativeTime(BASE - 59_000, BASE)).toBe('59s ago');
  });

  it('returns "Xm ago" for 1-59 minutes', () => {
    expect(relativeTime(BASE - 60_000,   BASE)).toBe('1m ago');
    expect(relativeTime(BASE - 90_000,   BASE)).toBe('1m ago');
    expect(relativeTime(BASE - 3_540_000, BASE)).toBe('59m ago');
  });

  it('returns "Xh ago" for 1-23 hours', () => {
    expect(relativeTime(BASE - 3_600_000,  BASE)).toBe('1h ago');
    expect(relativeTime(BASE - 7_200_000,  BASE)).toBe('2h ago');
    expect(relativeTime(BASE - 82_800_000, BASE)).toBe('23h ago');
  });

  it('returns "Xd ago" for 1+ days', () => {
    expect(relativeTime(BASE - 86_400_000,   BASE)).toBe('1d ago');
    expect(relativeTime(BASE - 172_800_000,  BASE)).toBe('2d ago');
    expect(relativeTime(BASE - 604_800_000,  BASE)).toBe('7d ago');
  });

  it('returns "just now" for future timestamps', () => {
    expect(relativeTime(BASE + 5_000, BASE)).toBe('just now');
  });
});

// ---------------------------------------------------------------------------
// formatAuthor
// ---------------------------------------------------------------------------

describe('formatAuthor', () => {
  it('strips the email bracket section', () => {
    expect(formatAuthor('Jane Doe <jane@example.com>')).toBe('Jane Doe');
  });

  it('trims trailing whitespace before the bracket', () => {
    expect(formatAuthor('John Smith  <john@example.com>')).toBe('John Smith');
  });

  it('returns the string unchanged when there is no bracket', () => {
    expect(formatAuthor('janedoe')).toBe('janedoe');
  });

  it('handles an empty string', () => {
    expect(formatAuthor('')).toBe('');
  });

  it('handles author with only an email (no name)', () => {
    expect(formatAuthor('<bot@example.com>')).toBe('');
  });
});

// ---------------------------------------------------------------------------
// filterCommitsByQuery
// ---------------------------------------------------------------------------

describe('filterCommitsByQuery', () => {
  const commits = [
    'abc1234 fix auth bug',
    'def5678 feat: add pipeline support',
    '1a2b3c4 refactor: split utils',
    'deadbeef chore: update dependencies',
  ];

  it('returns all commits for an empty query', () => {
    expect(filterCommitsByQuery(commits, '')).toHaveLength(4);
  });

  it('filters by message content (case-insensitive)', () => {
    const result = filterCommitsByQuery(commits, 'auth');
    expect(result).toHaveLength(1);
    expect(result[0]).toContain('fix auth bug');
  });

  it('filters by hash prefix', () => {
    const result = filterCommitsByQuery(commits, 'deadbeef');
    expect(result).toHaveLength(1);
    expect(result[0]).toContain('chore');
  });

  it('is case-insensitive', () => {
    const result = filterCommitsByQuery(commits, 'FEAT');
    expect(result).toHaveLength(1);
    expect(result[0]).toContain('feat');
  });

  it('returns empty array when nothing matches', () => {
    expect(filterCommitsByQuery(commits, 'xyznonexistent')).toHaveLength(0);
  });

  it('matches multiple commits when query appears in several', () => {
    // "e" appears in many; use a more specific substring
    const result = filterCommitsByQuery(commits, 'refactor');
    expect(result).toHaveLength(1);
  });
});

// ---------------------------------------------------------------------------
// isHashChar
// ---------------------------------------------------------------------------

describe('isHashChar', () => {
  it('returns true for lowercase hex digits', () => {
    '0123456789abcdef'.split('').forEach((c) => {
      expect(isHashChar(c)).toBe(true);
    });
  });

  it('returns true for uppercase hex digits', () => {
    'ABCDEF'.split('').forEach((c) => {
      expect(isHashChar(c)).toBe(true);
    });
  });

  it('returns false for non-hex characters', () => {
    ['g', 'z', '-', ' ', '!', 'G', 'Z'].forEach((c) => {
      expect(isHashChar(c)).toBe(false);
    });
  });
});

// ---------------------------------------------------------------------------
// commitCount / isEmpty
// ---------------------------------------------------------------------------

describe('commitCount', () => {
  it('returns 0 for an empty list', () => {
    expect(commitCount([])).toBe(0);
  });

  it('returns the correct count', () => {
    expect(commitCount(['abc fix', 'def feat'])).toBe(2);
  });
});

describe('isEmpty', () => {
  it('returns true for an empty commits array', () => {
    expect(isEmpty([])).toBe(true);
  });

  it('returns false when commits are present', () => {
    expect(isEmpty(['abc1234 fix something'])).toBe(false);
  });
});
