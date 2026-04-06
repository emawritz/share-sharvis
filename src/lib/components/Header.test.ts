import { describe, it, expect } from 'vitest';

// Pure functions extracted from Header.svelte for isolated testing.
// No Svelte mounting — all logic is tested as plain TypeScript functions.

// ---------------------------------------------------------------------------
// displaySessionId: mirrors the $derived in Header.svelte:
//   sessionId.replace(/^(parallel|longrun)-/, '').substring(0, 10)
//   Falls back to '-' when sessionId is falsy.
// ---------------------------------------------------------------------------
function displaySessionId(sessionId: string | undefined | null): string {
  if (!sessionId) return '-';
  return sessionId.replace(/^(parallel|longrun)-/, '').substring(0, 10);
}

// ---------------------------------------------------------------------------
// describeActivity: mirrors the function in Header.svelte.
// Walks backwards through the feed looking for the most recent tool or text activity.
// ---------------------------------------------------------------------------
interface Activity {
  type: string;
  name?: string;
  detail?: string;
  content?: string;
}

function describeActivity(feed: Activity[]): string {
  for (let i = feed.length - 1; i >= 0; i--) {
    const a = feed[i];
    if (a.type === 'tool' && a.name) {
      const detail = a.detail ? `: ${a.detail}` : '';
      return `${a.name}${detail}`;
    }
    if (a.type === 'text' && a.content) {
      return a.content.substring(0, 80);
    }
  }
  return '';
}

// ---------------------------------------------------------------------------
// dynamicStatus: mirrors the $derived.by() in Header.svelte.
// Returns '' when nothing is running, otherwise builds "ATLAS: ..." | "PIXEL: ..."
// ---------------------------------------------------------------------------
function dynamicStatus(
  atlasRunning: boolean,
  pixelRunning: boolean,
  atlasFeed: Activity[],
  pixelFeed: Activity[]
): string {
  if (!atlasRunning && !pixelRunning) return '';
  const parts: string[] = [];
  if (atlasRunning) {
    const desc = describeActivity(atlasFeed);
    if (desc) parts.push(`ATLAS: ${desc}`);
  }
  if (pixelRunning) {
    const desc = describeActivity(pixelFeed);
    if (desc) parts.push(`PIXEL: ${desc}`);
  }
  return parts.join('  |  ');
}

// ---------------------------------------------------------------------------
// headerStatusText: mirrors `$derived(dynamicStatus || $session.objetivo || '')`
// ---------------------------------------------------------------------------
function headerStatusText(status: string, objetivo: string | undefined): string {
  return status || objetivo || '';
}

// ---------------------------------------------------------------------------
// roundPipClass: mirrors the round pip class assignment in roundPips $derived.by().
// done: all matching >= 2 and every one is done.
// active: some matching but not all done.
// pending: nothing matching yet.
// ---------------------------------------------------------------------------
interface Round { file?: string; done: boolean }

function roundPipClass(
  roundNum: number,
  rounds: Round[]
): 'done' | 'active' | 'pending' {
  const matching = rounds.filter((r) => r.file && r.file.includes('round-' + roundNum + '-'));
  const done = matching.length >= 2 && matching.every((r) => r.done);
  const active = matching.length > 0 && !done;
  return done ? 'done' : active ? 'active' : 'pending';
}

// ---------------------------------------------------------------------------
// roundPipText: mirrors the `text` field in roundPips.
// done → '✓', otherwise the round number as string.
// ---------------------------------------------------------------------------
function roundPipText(roundNum: number, cls: 'done' | 'active' | 'pending'): string {
  return cls === 'done' ? '\u2713' : String(roundNum);
}

// ---------------------------------------------------------------------------
// isLiveIndicatorOn: mirrors `class:on={$session.active}`.
// Returns true when session.active is truthy.
// ---------------------------------------------------------------------------
function isLiveIndicatorOn(active: boolean | undefined): boolean {
  return !!active;
}

// ---------------------------------------------------------------------------
// branchDisplayText: mirrors `repoBack?.branch || '...'` in the template.
// Shows '...' when branch is unavailable.
// ---------------------------------------------------------------------------
function branchDisplayText(branch: string | undefined | null): string {
  return branch || '...';
}

// ---------------------------------------------------------------------------
// isBranchDirty: mirrors `repoBack.changed > 0 || repoBack.staged > 0`.
// Returns true when the repo has any pending changes.
// ---------------------------------------------------------------------------
function isBranchDirty(changed: number, staged: number): boolean {
  return changed > 0 || staged > 0;
}

// ---------------------------------------------------------------------------
// workspaceSaveDisabled: mirrors the disabled condition on the save button:
//   `workspaceSaving || !newWorkspaceName.trim()`
// ---------------------------------------------------------------------------
function workspaceSaveDisabled(saving: boolean, name: string): boolean {
  return saving || !name.trim();
}

// ---------------------------------------------------------------------------
// sessionIdStripped: the session chip strips known prefixes but also has a 10-char cap.
// Specifically tests the prefix stripping + substring logic.
// ---------------------------------------------------------------------------
function sessionIdStripped(id: string): string {
  return id.replace(/^(parallel|longrun)-/, '').substring(0, 10);
}

// ===========================================================================
// TESTS
// ===========================================================================

// ---------------------------------------------------------------------------
// displaySessionId
// ---------------------------------------------------------------------------
describe('displaySessionId', () => {
  it('returns "-" for undefined sessionId', () => {
    expect(displaySessionId(undefined)).toBe('-');
  });

  it('returns "-" for null sessionId', () => {
    expect(displaySessionId(null)).toBe('-');
  });

  it('returns "-" for empty string', () => {
    expect(displaySessionId('')).toBe('-');
  });

  it('strips the "parallel-" prefix', () => {
    expect(displaySessionId('parallel-abc123')).toBe('abc123');
  });

  it('strips the "longrun-" prefix', () => {
    expect(displaySessionId('longrun-abc123')).toBe('abc123');
  });

  it('caps at 10 characters', () => {
    expect(displaySessionId('abcdefghijk')).toBe('abcdefghij');
  });

  it('caps after prefix stripping', () => {
    // 'parallel-' stripped → 'abcdefghijklmnop' → first 10 chars
    expect(displaySessionId('parallel-abcdefghijklmnop')).toBe('abcdefghij');
  });

  it('returns full value when shorter than 10 chars', () => {
    expect(displaySessionId('abc')).toBe('abc');
  });

  it('does not strip unrecognised prefixes', () => {
    expect(displaySessionId('session-abc')).toBe('session-ab');
  });
});

// ---------------------------------------------------------------------------
// describeActivity
// ---------------------------------------------------------------------------
describe('describeActivity', () => {
  it('returns empty string for empty feed', () => {
    expect(describeActivity([])).toBe('');
  });

  it('returns tool name for a tool activity', () => {
    const feed: Activity[] = [{ type: 'tool', name: 'Read' }];
    expect(describeActivity(feed)).toBe('Read');
  });

  it('includes detail when present', () => {
    const feed: Activity[] = [{ type: 'tool', name: 'Bash', detail: 'ls -la' }];
    expect(describeActivity(feed)).toBe('Bash: ls -la');
  });

  it('returns text content for a text activity', () => {
    const feed: Activity[] = [{ type: 'text', content: 'Hello world' }];
    expect(describeActivity(feed)).toBe('Hello world');
  });

  it('caps text content at 80 characters', () => {
    const long = 'x'.repeat(100);
    const feed: Activity[] = [{ type: 'text', content: long }];
    expect(describeActivity(feed)).toBe('x'.repeat(80));
  });

  it('prefers the last item in the feed', () => {
    const feed: Activity[] = [
      { type: 'tool', name: 'Read' },
      { type: 'tool', name: 'Write', detail: 'foo.ts' },
    ];
    expect(describeActivity(feed)).toBe('Write: foo.ts');
  });

  it('skips tool entries without a name', () => {
    const feed: Activity[] = [
      { type: 'tool' },
      { type: 'text', content: 'fallback text' },
    ];
    expect(describeActivity(feed)).toBe('fallback text');
  });

  it('returns empty string when no tool or text type found', () => {
    const feed: Activity[] = [
      { type: 'unknown' },
      { type: 'system' },
    ];
    expect(describeActivity(feed)).toBe('');
  });
});

// ---------------------------------------------------------------------------
// dynamicStatus
// ---------------------------------------------------------------------------
describe('dynamicStatus', () => {
  it('returns empty string when nothing is running', () => {
    expect(dynamicStatus(false, false, [], [])).toBe('');
  });

  it('returns ATLAS status when only atlas is running', () => {
    const atlasFeed: Activity[] = [{ type: 'tool', name: 'Read' }];
    expect(dynamicStatus(true, false, atlasFeed, [])).toBe('ATLAS: Read');
  });

  it('returns PIXEL status when only pixel is running', () => {
    const pixelFeed: Activity[] = [{ type: 'tool', name: 'Bash', detail: 'npm run build' }];
    expect(dynamicStatus(false, true, [], pixelFeed)).toBe('PIXEL: Bash: npm run build');
  });

  it('joins both statuses with " | " when both are running', () => {
    const atlasFeed: Activity[] = [{ type: 'tool', name: 'Read' }];
    const pixelFeed: Activity[] = [{ type: 'tool', name: 'Write' }];
    expect(dynamicStatus(true, true, atlasFeed, pixelFeed)).toBe('ATLAS: Read  |  PIXEL: Write');
  });

  it('omits a machine from the output if its feed produces no description', () => {
    // Feed has only unknown-type entries → describeActivity returns ''
    const emptyFeed: Activity[] = [{ type: 'system' }];
    const pixelFeed: Activity[] = [{ type: 'tool', name: 'Edit' }];
    expect(dynamicStatus(true, true, emptyFeed, pixelFeed)).toBe('PIXEL: Edit');
  });

  it('returns empty string when both running but both feeds are empty', () => {
    expect(dynamicStatus(true, true, [], [])).toBe('');
  });
});

// ---------------------------------------------------------------------------
// headerStatusText
// ---------------------------------------------------------------------------
describe('headerStatusText', () => {
  it('returns dynamic status when present', () => {
    expect(headerStatusText('ATLAS: Read', 'Build the feature')).toBe('ATLAS: Read');
  });

  it('falls back to objetivo when no dynamic status', () => {
    expect(headerStatusText('', 'Build the feature')).toBe('Build the feature');
  });

  it('returns empty string when both are empty', () => {
    expect(headerStatusText('', '')).toBe('');
  });

  it('returns empty string when both are undefined/falsy', () => {
    expect(headerStatusText('', undefined)).toBe('');
  });
});

// ---------------------------------------------------------------------------
// roundPipClass
// ---------------------------------------------------------------------------
describe('roundPipClass', () => {
  it('returns "pending" when no rounds match', () => {
    expect(roundPipClass(1, [])).toBe('pending');
  });

  it('returns "active" when fewer than 2 matching rounds', () => {
    const rounds: Round[] = [
      { file: 'round-1-atlas.jsonl', done: false },
    ];
    expect(roundPipClass(1, rounds)).toBe('active');
  });

  it('returns "active" when 2 matching but not all done', () => {
    const rounds: Round[] = [
      { file: 'round-1-atlas.jsonl', done: true },
      { file: 'round-1-pixel.jsonl', done: false },
    ];
    expect(roundPipClass(1, rounds)).toBe('active');
  });

  it('returns "done" when >= 2 matching and all done', () => {
    const rounds: Round[] = [
      { file: 'round-1-atlas.jsonl', done: true },
      { file: 'round-1-pixel.jsonl', done: true },
    ];
    expect(roundPipClass(1, rounds)).toBe('done');
  });

  it('only considers rounds matching the given round number', () => {
    const rounds: Round[] = [
      { file: 'round-2-atlas.jsonl', done: true },
      { file: 'round-2-pixel.jsonl', done: true },
    ];
    // round 1 has no matching files → pending
    expect(roundPipClass(1, rounds)).toBe('pending');
    // round 2 has 2 done → done
    expect(roundPipClass(2, rounds)).toBe('done');
  });

  it('returns "pending" when matching rounds have no file property', () => {
    const rounds: Round[] = [{ done: true }, { done: true }];
    expect(roundPipClass(1, rounds)).toBe('pending');
  });
});

// ---------------------------------------------------------------------------
// roundPipText
// ---------------------------------------------------------------------------
describe('roundPipText', () => {
  it('returns checkmark for done pips', () => {
    expect(roundPipText(1, 'done')).toBe('\u2713');
  });

  it('returns round number string for active pips', () => {
    expect(roundPipText(3, 'active')).toBe('3');
  });

  it('returns round number string for pending pips', () => {
    expect(roundPipText(5, 'pending')).toBe('5');
  });
});

// ---------------------------------------------------------------------------
// isLiveIndicatorOn
// ---------------------------------------------------------------------------
describe('isLiveIndicatorOn', () => {
  it('returns true when session is active', () => {
    expect(isLiveIndicatorOn(true)).toBe(true);
  });

  it('returns false when session is inactive', () => {
    expect(isLiveIndicatorOn(false)).toBe(false);
  });

  it('returns false when active is undefined', () => {
    expect(isLiveIndicatorOn(undefined)).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// branchDisplayText
// ---------------------------------------------------------------------------
describe('branchDisplayText', () => {
  it('returns the branch name when present', () => {
    expect(branchDisplayText('main')).toBe('main');
    expect(branchDisplayText('feat/new-ui')).toBe('feat/new-ui');
  });

  it('returns "..." for undefined branch', () => {
    expect(branchDisplayText(undefined)).toBe('...');
  });

  it('returns "..." for null branch', () => {
    expect(branchDisplayText(null)).toBe('...');
  });

  it('returns "..." for empty string branch', () => {
    expect(branchDisplayText('')).toBe('...');
  });
});

// ---------------------------------------------------------------------------
// isBranchDirty
// ---------------------------------------------------------------------------
describe('isBranchDirty', () => {
  it('returns false when both changed and staged are 0', () => {
    expect(isBranchDirty(0, 0)).toBe(false);
  });

  it('returns true when changed > 0', () => {
    expect(isBranchDirty(3, 0)).toBe(true);
  });

  it('returns true when staged > 0', () => {
    expect(isBranchDirty(0, 2)).toBe(true);
  });

  it('returns true when both are > 0', () => {
    expect(isBranchDirty(1, 1)).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// workspaceSaveDisabled
// ---------------------------------------------------------------------------
describe('workspaceSaveDisabled', () => {
  it('returns true when saving is true', () => {
    expect(workspaceSaveDisabled(true, 'my-workspace')).toBe(true);
  });

  it('returns true when name is empty', () => {
    expect(workspaceSaveDisabled(false, '')).toBe(true);
  });

  it('returns true when name is only whitespace', () => {
    expect(workspaceSaveDisabled(false, '   ')).toBe(true);
  });

  it('returns false when not saving and name is valid', () => {
    expect(workspaceSaveDisabled(false, 'my-workspace')).toBe(false);
  });

  it('returns true when both saving and name is empty', () => {
    expect(workspaceSaveDisabled(true, '')).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// sessionIdStripped (prefix + length cap)
// ---------------------------------------------------------------------------
describe('sessionIdStripped', () => {
  it('strips parallel- prefix', () => {
    expect(sessionIdStripped('parallel-xyz')).toBe('xyz');
  });

  it('strips longrun- prefix', () => {
    expect(sessionIdStripped('longrun-xyz')).toBe('xyz');
  });

  it('keeps first 10 chars after prefix removal', () => {
    expect(sessionIdStripped('parallel-abcdefghijklmnop')).toHaveLength(10);
  });

  it('does not strip unrecognised prefixes', () => {
    const result = sessionIdStripped('session-abc123');
    expect(result).toBe('session-ab');
  });

  it('handles string shorter than 10 chars without truncation', () => {
    expect(sessionIdStripped('short')).toBe('short');
  });

  it('handles exactly 10 chars', () => {
    expect(sessionIdStripped('1234567890')).toBe('1234567890');
  });
});
