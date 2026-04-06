import { describe, it, expect } from 'vitest';

// Pure functions extracted from GithubTab.svelte for isolated testing.
// No Svelte mounting — all logic is tested as plain TypeScript functions.

// ---------------------------------------------------------------------------
// PR state → CSS class
// The component renders: class="pr-state {(pr.state || 'open').toLowerCase()}"
// ---------------------------------------------------------------------------
function prStateClass(state: string | undefined | null): string {
  return (state || 'open').toLowerCase();
}

// ---------------------------------------------------------------------------
// PR state → display label (mirrors the template: pr.state || t('github.open'))
// For test purposes we use the English fallback 'Open'.
// ---------------------------------------------------------------------------
function prStateLabel(state: string | undefined | null, fallback = 'Open'): string {
  return state || fallback;
}

// ---------------------------------------------------------------------------
// isOpenPR: the template shows the merge button only when state.toUpperCase() === 'OPEN'
// ---------------------------------------------------------------------------
function isOpenPR(state: string | undefined | null): boolean {
  return (state || '').toUpperCase() === 'OPEN';
}

// ---------------------------------------------------------------------------
// extractAuthorLogin: mirrors the author rendering logic in the component.
// typeof pr.author === 'object' && 'login' in pr.author → login, else String(pr.author)
// ---------------------------------------------------------------------------
function extractAuthorLogin(author: unknown): string {
  if (author && typeof author === 'object' && 'login' in author) {
    return (author as { login: string }).login;
  }
  return String(author);
}

// ---------------------------------------------------------------------------
// truncateTitle: used by the component via the native title tooltip, but
// we test an explicit helper that caps display text for list views.
// ---------------------------------------------------------------------------
function truncateTitle(title: string, limit = 60): string {
  if (!title) return '';
  return title.length > limit ? title.slice(0, limit) + '…' : title;
}

// ---------------------------------------------------------------------------
// githubPRUrl: constructs the canonical PR URL from repo + PR number.
// Pattern: https://github.com/{repo}/pull/{number}
// ---------------------------------------------------------------------------
function githubPRUrl(repo: string, prNumber: number): string {
  return `https://github.com/${repo}/pull/${prNumber}`;
}

// ---------------------------------------------------------------------------
// githubRepoUrl: base URL for a repository.
// ---------------------------------------------------------------------------
function githubRepoUrl(repo: string): string {
  return `https://github.com/${repo}`;
}

// ---------------------------------------------------------------------------
// filterOpenPRs: returns only PRs whose state is OPEN (case-insensitive).
// Used when the "open only" filter is active.
// ---------------------------------------------------------------------------
interface MinimalPR {
  number: number;
  title: string;
  state?: string;
}

function filterOpenPRs(prs: MinimalPR[]): MinimalPR[] {
  return prs.filter(pr => (pr.state || 'open').toUpperCase() === 'OPEN');
}

// ---------------------------------------------------------------------------
// filterAllPRs: identity — returns every PR regardless of state.
// ---------------------------------------------------------------------------
function filterAllPRs(prs: MinimalPR[]): MinimalPR[] {
  return prs;
}

// ---------------------------------------------------------------------------
// diffStats: formats the +additions / -deletions display strings.
// ---------------------------------------------------------------------------
function additionsLabel(n: number | undefined): string {
  return `+${n ?? 0}`;
}

function deletionsLabel(n: number | undefined): string {
  return `-${n ?? 0}`;
}

// ---------------------------------------------------------------------------
// mergeConfirmMessage: mirrors the confirm modal message template.
// `PR #${id} — ${title} (${method})`
// ---------------------------------------------------------------------------
function mergeConfirmMessage(id: number, title: string, method: string): string {
  return `PR #${id} — ${title} (${method})`;
}

// ---------------------------------------------------------------------------
// deduplicateRepos: mirrors loadRepos() Set-based dedup logic.
// Returns repos in insertion order, skipping duplicate github slugs.
// ---------------------------------------------------------------------------
interface RepoEntry { name: string; github: string }

function deduplicateRepos(machines: { repos: RepoEntry[] }[]): RepoEntry[] {
  const seen = new Set<string>();
  const result: RepoEntry[] = [];
  for (const m of machines) {
    for (const r of m.repos) {
      if (r.github && !seen.has(r.github)) {
        seen.add(r.github);
        result.push({ name: r.name, github: r.github });
      }
    }
  }
  return result;
}

// ---------------------------------------------------------------------------
// selectDefaultRepo: if current repo is not in the list, pick repos[0].github
// ---------------------------------------------------------------------------
function selectDefaultRepo(repos: RepoEntry[], current: string): string {
  if (repos.length === 0) return current;
  if (repos.find(r => r.github === current)) return current;
  return repos[0].github;
}

// ===========================================================================
// TESTS
// ===========================================================================

// ---------------------------------------------------------------------------
// prStateClass
// ---------------------------------------------------------------------------

describe('prStateClass', () => {
  it('returns "open" for state "OPEN"', () => {
    expect(prStateClass('OPEN')).toBe('open');
  });

  it('returns "closed" for state "CLOSED"', () => {
    expect(prStateClass('CLOSED')).toBe('closed');
  });

  it('returns "merged" for state "MERGED"', () => {
    expect(prStateClass('MERGED')).toBe('merged');
  });

  it('lowercases already-lowercase states unchanged', () => {
    expect(prStateClass('open')).toBe('open');
    expect(prStateClass('closed')).toBe('closed');
    expect(prStateClass('merged')).toBe('merged');
  });

  it('defaults to "open" for undefined state', () => {
    expect(prStateClass(undefined)).toBe('open');
  });

  it('defaults to "open" for null state', () => {
    expect(prStateClass(null)).toBe('open');
  });

  it('defaults to "open" for empty string state', () => {
    expect(prStateClass('')).toBe('open');
  });
});

// ---------------------------------------------------------------------------
// prStateLabel
// ---------------------------------------------------------------------------

describe('prStateLabel', () => {
  it('returns the state when provided', () => {
    expect(prStateLabel('OPEN')).toBe('OPEN');
    expect(prStateLabel('closed')).toBe('closed');
  });

  it('returns the fallback for undefined', () => {
    expect(prStateLabel(undefined, 'Open')).toBe('Open');
  });

  it('returns the fallback for empty string', () => {
    expect(prStateLabel('', 'Open')).toBe('Open');
  });

  it('uses default fallback "Open" when not specified', () => {
    expect(prStateLabel(null)).toBe('Open');
  });
});

// ---------------------------------------------------------------------------
// isOpenPR
// ---------------------------------------------------------------------------

describe('isOpenPR', () => {
  it('returns true for "OPEN"', () => {
    expect(isOpenPR('OPEN')).toBe(true);
  });

  it('returns true for "open" (case-insensitive)', () => {
    expect(isOpenPR('open')).toBe(true);
  });

  it('returns true for "Open"', () => {
    expect(isOpenPR('Open')).toBe(true);
  });

  it('returns false for "CLOSED"', () => {
    expect(isOpenPR('CLOSED')).toBe(false);
  });

  it('returns false for "MERGED"', () => {
    expect(isOpenPR('MERGED')).toBe(false);
  });

  it('returns false for undefined', () => {
    expect(isOpenPR(undefined)).toBe(false);
  });

  it('returns false for null', () => {
    expect(isOpenPR(null)).toBe(false);
  });

  it('returns false for empty string', () => {
    expect(isOpenPR('')).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// extractAuthorLogin
// ---------------------------------------------------------------------------

describe('extractAuthorLogin', () => {
  it('returns login from object with login property', () => {
    expect(extractAuthorLogin({ login: 'octocat' })).toBe('octocat');
  });

  it('returns string representation for plain string author', () => {
    expect(extractAuthorLogin('octouser')).toBe('octouser');
  });

  it('returns string representation for a number', () => {
    expect(extractAuthorLogin(42)).toBe('42');
  });

  it('returns string representation for null', () => {
    expect(extractAuthorLogin(null)).toBe('null');
  });

  it('returns string representation for undefined', () => {
    expect(extractAuthorLogin(undefined)).toBe('undefined');
  });

  it('handles object without login, falls back to String()', () => {
    // Objects without login use String() which gives [object Object]
    const result = extractAuthorLogin({ name: 'no-login' });
    expect(result).toBe('[object Object]');
  });
});

// ---------------------------------------------------------------------------
// truncateTitle
// ---------------------------------------------------------------------------

describe('truncateTitle', () => {
  it('returns title unchanged when within limit', () => {
    expect(truncateTitle('fix auth bug', 60)).toBe('fix auth bug');
  });

  it('returns title unchanged when exactly at limit', () => {
    const title = 'x'.repeat(60);
    expect(truncateTitle(title, 60)).toBe(title);
  });

  it('truncates and appends ellipsis when over limit', () => {
    const title = 'a'.repeat(70);
    expect(truncateTitle(title, 60)).toBe('a'.repeat(60) + '…');
  });

  it('returns empty string for empty title', () => {
    expect(truncateTitle('', 60)).toBe('');
  });

  it('respects a custom limit', () => {
    expect(truncateTitle('hello world', 5)).toBe('hello…');
  });

  it('handles very short limit of 1', () => {
    expect(truncateTitle('hello', 1)).toBe('h…');
  });
});

// ---------------------------------------------------------------------------
// githubPRUrl
// ---------------------------------------------------------------------------

describe('githubPRUrl', () => {
  it('constructs a correct PR URL', () => {
    expect(githubPRUrl('user/my-project', 42)).toBe('https://github.com/user/my-project/pull/42');
  });

  it('handles PR number 1', () => {
    expect(githubPRUrl('owner/repo', 1)).toBe('https://github.com/owner/repo/pull/1');
  });

  it('handles large PR numbers', () => {
    expect(githubPRUrl('owner/repo', 9999)).toBe('https://github.com/owner/repo/pull/9999');
  });
});

// ---------------------------------------------------------------------------
// githubRepoUrl
// ---------------------------------------------------------------------------

describe('githubRepoUrl', () => {
  it('constructs a correct repo URL', () => {
    expect(githubRepoUrl('user/my-project')).toBe('https://github.com/user/my-project');
  });

  it('works for org/repo slugs', () => {
    expect(githubRepoUrl('anthropics/claude-code')).toBe('https://github.com/anthropics/claude-code');
  });
});

// ---------------------------------------------------------------------------
// filterOpenPRs
// ---------------------------------------------------------------------------

describe('filterOpenPRs', () => {
  const prs: MinimalPR[] = [
    { number: 1, title: 'Open PR', state: 'OPEN' },
    { number: 2, title: 'Closed PR', state: 'CLOSED' },
    { number: 3, title: 'Merged PR', state: 'MERGED' },
    { number: 4, title: 'Lowercase open', state: 'open' },
    { number: 5, title: 'No state PR' },
  ];

  it('keeps only OPEN-state PRs', () => {
    const result = filterOpenPRs(prs);
    expect(result.map(p => p.number)).toEqual([1, 4, 5]);
  });

  it('returns empty array when no open PRs', () => {
    const closed = [
      { number: 1, title: 'Closed', state: 'CLOSED' },
      { number: 2, title: 'Merged', state: 'MERGED' },
    ];
    expect(filterOpenPRs(closed)).toHaveLength(0);
  });

  it('returns empty array for empty input', () => {
    expect(filterOpenPRs([])).toHaveLength(0);
  });

  it('treats missing state as OPEN', () => {
    const result = filterOpenPRs([{ number: 10, title: 'No state' }]);
    expect(result).toHaveLength(1);
  });
});

// ---------------------------------------------------------------------------
// filterAllPRs
// ---------------------------------------------------------------------------

describe('filterAllPRs', () => {
  it('returns all PRs regardless of state', () => {
    const prs: MinimalPR[] = [
      { number: 1, title: 'Open', state: 'OPEN' },
      { number: 2, title: 'Closed', state: 'CLOSED' },
      { number: 3, title: 'Merged', state: 'MERGED' },
    ];
    expect(filterAllPRs(prs)).toHaveLength(3);
  });

  it('returns empty for empty input', () => {
    expect(filterAllPRs([])).toHaveLength(0);
  });
});

// ---------------------------------------------------------------------------
// additionsLabel / deletionsLabel
// ---------------------------------------------------------------------------

describe('additionsLabel', () => {
  it('formats positive additions', () => {
    expect(additionsLabel(42)).toBe('+42');
  });

  it('formats zero additions', () => {
    expect(additionsLabel(0)).toBe('+0');
  });

  it('defaults to 0 for undefined', () => {
    expect(additionsLabel(undefined)).toBe('+0');
  });
});

describe('deletionsLabel', () => {
  it('formats positive deletions', () => {
    expect(deletionsLabel(10)).toBe('-10');
  });

  it('formats zero deletions', () => {
    expect(deletionsLabel(0)).toBe('-0');
  });

  it('defaults to 0 for undefined', () => {
    expect(deletionsLabel(undefined)).toBe('-0');
  });
});

// ---------------------------------------------------------------------------
// mergeConfirmMessage
// ---------------------------------------------------------------------------

describe('mergeConfirmMessage', () => {
  it('formats the confirm message correctly', () => {
    expect(mergeConfirmMessage(42, 'fix auth bug', 'squash')).toBe('PR #42 — fix auth bug (squash)');
  });

  it('works with merge method "rebase"', () => {
    expect(mergeConfirmMessage(7, 'feat: add pipeline', 'rebase')).toBe('PR #7 — feat: add pipeline (rebase)');
  });

  it('works with merge method "merge"', () => {
    expect(mergeConfirmMessage(1, 'chore: bump deps', 'merge')).toBe('PR #1 — chore: bump deps (merge)');
  });
});

// ---------------------------------------------------------------------------
// deduplicateRepos
// ---------------------------------------------------------------------------

describe('deduplicateRepos', () => {
  it('returns repos from a single machine', () => {
    const machines = [
      { repos: [{ name: 'jarvis', github: 'ema/jarvis' }] }
    ];
    expect(deduplicateRepos(machines)).toEqual([{ name: 'jarvis', github: 'ema/jarvis' }]);
  });

  it('deduplicates the same github slug across machines', () => {
    const machines = [
      { repos: [{ name: 'jarvis', github: 'ema/jarvis' }] },
      { repos: [{ name: 'jarvis-copy', github: 'ema/jarvis' }] },
    ];
    const result = deduplicateRepos(machines);
    expect(result).toHaveLength(1);
    expect(result[0].github).toBe('ema/jarvis');
  });

  it('keeps multiple distinct repos', () => {
    const machines = [
      { repos: [{ name: 'front', github: 'ema/front' }, { name: 'back', github: 'ema/back' }] },
    ];
    const result = deduplicateRepos(machines);
    expect(result).toHaveLength(2);
  });

  it('skips repos without a github slug', () => {
    const machines = [
      { repos: [{ name: 'local-only', github: '' }, { name: 'remote', github: 'ema/remote' }] }
    ];
    const result = deduplicateRepos(machines);
    expect(result).toHaveLength(1);
    expect(result[0].github).toBe('ema/remote');
  });

  it('returns empty for machines with no repos', () => {
    expect(deduplicateRepos([{ repos: [] }])).toHaveLength(0);
  });

  it('returns empty for empty machines array', () => {
    expect(deduplicateRepos([])).toHaveLength(0);
  });

  it('preserves insertion order', () => {
    const machines = [
      { repos: [{ name: 'a', github: 'ema/a' }, { name: 'b', github: 'ema/b' }] },
      { repos: [{ name: 'c', github: 'ema/c' }] },
    ];
    const result = deduplicateRepos(machines);
    expect(result.map(r => r.github)).toEqual(['ema/a', 'ema/b', 'ema/c']);
  });
});

// ---------------------------------------------------------------------------
// selectDefaultRepo
// ---------------------------------------------------------------------------

describe('selectDefaultRepo', () => {
  const repos: RepoEntry[] = [
    { name: 'front', github: 'ema/front' },
    { name: 'back', github: 'ema/back' },
  ];

  it('keeps current repo when it is in the list', () => {
    expect(selectDefaultRepo(repos, 'ema/back')).toBe('ema/back');
  });

  it('switches to first repo when current is not in the list', () => {
    expect(selectDefaultRepo(repos, 'ema/unknown')).toBe('ema/front');
  });

  it('returns current when repos is empty', () => {
    expect(selectDefaultRepo([], 'ema/jarvis')).toBe('ema/jarvis');
  });

  it('picks repos[0] when current is an empty string', () => {
    expect(selectDefaultRepo(repos, '')).toBe('ema/front');
  });
});
