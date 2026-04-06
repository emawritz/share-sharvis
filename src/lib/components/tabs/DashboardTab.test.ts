import { describe, it, expect } from 'vitest';

// Pure functions copied from DashboardTab.svelte for isolated testing.
// The component versions close over reactive `now`; here we make `now` an
// explicit parameter so the functions are pure and testable without Svelte.

function formatDuration(secs: number): string {
  if (secs === 0) return '-';
  if (secs < 60) return `${secs}s`;
  return `${Math.floor(secs / 60)}m${secs % 60}s`;
}

function truncate(s: string, n: number): string {
  if (!s) return '';
  return s.length > n ? s.slice(0, n) + '…' : s;
}

// elapsed: adapted from component — accepts explicit `now` instead of closing
// over the reactive variable. Handles undefined startedAt gracefully.
function elapsed(startedAt: number | undefined, now: number): string {
  if (!startedAt) return '';
  const secs = Math.round((now - startedAt) / 1000);
  if (secs < 60) return `${secs}s`;
  const m = Math.floor(secs / 60);
  const s = secs % 60;
  return `${m}m${s}s`;
}

// duration: adapted from component — accepts task-like objects directly.
function duration(task: { startedAt?: number; finishedAt?: number }): string {
  if (!task.startedAt || !task.finishedAt) return '-';
  const secs = Math.round((task.finishedAt - task.startedAt) / 1000);
  if (secs < 60) return `${secs}s`;
  const m = Math.floor(secs / 60);
  const s = secs % 60;
  return `${m}m${s}s`;
}

function statusBadgeClass(status: string): string {
  switch (status) {
    case 'running': return 'sb-running';
    case 'done': return 'sb-done';
    case 'error': case 'killed': return 'sb-error';
    case 'pending': return 'sb-pending';
    default: return 'sb-pending';
  }
}

function buildHourlyActivity(
  tasks: { status: string; startedAt?: number }[],
  now: number
): { label: string; done: number; error: number }[] {
  const hours: { label: string; done: number; error: number }[] = [];
  const nowHour = new Date(now);
  nowHour.setMinutes(0, 0, 0);
  for (let i = 11; i >= 0; i--) {
    const hourStart = nowHour.getTime() - i * 3600000;
    const hourEnd = hourStart + 3600000;
    const label = new Date(hourStart).getHours().toString().padStart(2, '0') + 'h';
    const done = tasks.filter(t =>
      t.status === 'done' && (t.startedAt ?? 0) >= hourStart && (t.startedAt ?? 0) < hourEnd
    ).length;
    const error = tasks.filter(t =>
      (t.status === 'error' || t.status === 'killed') && (t.startedAt ?? 0) >= hourStart && (t.startedAt ?? 0) < hourEnd
    ).length;
    hours.push({ label, done, error });
  }
  return hours;
}

// ---------------------------------------------------------------------------

describe('formatDuration', () => {
  it('returns "-" for 0 seconds', () => {
    expect(formatDuration(0)).toBe('-');
  });
  it('returns seconds for values < 60', () => {
    expect(formatDuration(30)).toBe('30s');
    expect(formatDuration(1)).toBe('1s');
    expect(formatDuration(59)).toBe('59s');
  });
  it('returns minutes+seconds for values >= 60', () => {
    expect(formatDuration(60)).toBe('1m0s');
    expect(formatDuration(90)).toBe('1m30s');
    expect(formatDuration(3661)).toBe('61m1s');
  });
  it('handles large values', () => {
    expect(formatDuration(3600)).toBe('60m0s');
  });
});

describe('truncate', () => {
  it('returns empty string for empty input', () => {
    expect(truncate('', 10)).toBe('');
  });
  it('returns empty string for falsy input', () => {
    expect(truncate(null as unknown as string, 10)).toBe('');
    expect(truncate(undefined as unknown as string, 10)).toBe('');
  });
  it('returns full string if shorter than limit', () => {
    expect(truncate('hello', 10)).toBe('hello');
  });
  it('returns full string if exactly at limit', () => {
    expect(truncate('hello', 5)).toBe('hello');
  });
  it('truncates and appends ellipsis when over limit', () => {
    expect(truncate('hello world', 5)).toBe('hello…');
    expect(truncate('abcdef', 3)).toBe('abc…');
  });
  it('truncates to 1 character', () => {
    expect(truncate('hello', 1)).toBe('h…');
  });
});

describe('elapsed', () => {
  it('returns empty string when startedAt is undefined', () => {
    expect(elapsed(undefined, Date.now())).toBe('');
  });
  it('returns seconds for short elapsed time', () => {
    const now = 1_000_000;
    expect(elapsed(now - 30_000, now)).toBe('30s');
    expect(elapsed(now - 1_000, now)).toBe('1s');
    expect(elapsed(now - 59_000, now)).toBe('59s');
  });
  it('returns minutes+seconds for longer elapsed time', () => {
    const now = 1_000_000;
    expect(elapsed(now - 60_000, now)).toBe('1m0s');
    expect(elapsed(now - 90_000, now)).toBe('1m30s');
    expect(elapsed(now - 3_600_000, now)).toBe('60m0s');
  });
  it('rounds to nearest second', () => {
    const now = 1_000_000;
    // 30.4s rounds to 30s
    expect(elapsed(now - 30_400, now)).toBe('30s');
    // 30.6s rounds to 31s
    expect(elapsed(now - 30_600, now)).toBe('31s');
  });
});

describe('duration', () => {
  it('returns "-" when startedAt is missing', () => {
    expect(duration({ finishedAt: 1000 })).toBe('-');
  });
  it('returns "-" when finishedAt is missing', () => {
    expect(duration({ startedAt: 1000 })).toBe('-');
  });
  it('returns "-" when both are missing', () => {
    expect(duration({})).toBe('-');
  });
  it('returns seconds for short tasks', () => {
    // startedAt must be non-zero (0 is falsy and triggers the early guard)
    expect(duration({ startedAt: 1_000_000, finishedAt: 1_030_000 })).toBe('30s');
  });
  it('returns minutes+seconds for longer tasks', () => {
    expect(duration({ startedAt: 1_000_000, finishedAt: 1_090_000 })).toBe('1m30s');
    expect(duration({ startedAt: 1_000_000, finishedAt: 1_060_000 })).toBe('1m0s');
  });
});

describe('statusBadgeClass', () => {
  it('maps running to sb-running', () => {
    expect(statusBadgeClass('running')).toBe('sb-running');
  });
  it('maps done to sb-done', () => {
    expect(statusBadgeClass('done')).toBe('sb-done');
  });
  it('maps error to sb-error', () => {
    expect(statusBadgeClass('error')).toBe('sb-error');
  });
  it('maps killed to sb-error', () => {
    expect(statusBadgeClass('killed')).toBe('sb-error');
  });
  it('maps pending to sb-pending', () => {
    expect(statusBadgeClass('pending')).toBe('sb-pending');
  });
  it('falls back to sb-pending for unknown status', () => {
    expect(statusBadgeClass('unknown')).toBe('sb-pending');
    expect(statusBadgeClass('')).toBe('sb-pending');
  });
});

describe('buildHourlyActivity', () => {
  it('always produces exactly 12 hour slots', () => {
    expect(buildHourlyActivity([], Date.now())).toHaveLength(12);
  });

  it('all slots are zero for empty task list', () => {
    const result = buildHourlyActivity([], Date.now());
    result.forEach(h => {
      expect(h.done).toBe(0);
      expect(h.error).toBe(0);
    });
  });

  it('labels slots with padded hour + "h"', () => {
    // Use a fixed time anchored to 09:30 to check label format
    const fixedNow = new Date('2024-01-15T09:30:00.000Z').getTime();
    const result = buildHourlyActivity([], fixedNow);
    result.forEach(h => {
      expect(h.label).toMatch(/^\d{2}h$/);
    });
  });

  it('counts done tasks in the current hour', () => {
    const now = Date.now();
    const tasks = [
      { status: 'done', startedAt: now - 60_000 },
      { status: 'done', startedAt: now - 60_000 },
    ];
    const result = buildHourlyActivity(tasks, now);
    const current = result[result.length - 1];
    expect(current.done).toBe(2);
    expect(current.error).toBe(0);
  });

  it('counts error and killed tasks in the current hour', () => {
    const now = Date.now();
    const tasks = [
      { status: 'error', startedAt: now - 60_000 },
      { status: 'killed', startedAt: now - 60_000 },
    ];
    const result = buildHourlyActivity(tasks, now);
    const current = result[result.length - 1];
    expect(current.error).toBe(2);
    expect(current.done).toBe(0);
  });

  it('counts mixed done/error in the current hour', () => {
    const now = Date.now();
    const tasks = [
      { status: 'done', startedAt: now - 60_000 },
      { status: 'error', startedAt: now - 60_000 },
      { status: 'killed', startedAt: now - 60_000 },
    ];
    const result = buildHourlyActivity(tasks, now);
    const current = result[result.length - 1];
    expect(current.done).toBe(1);
    expect(current.error).toBe(2);
  });

  it('places a task from 2 hours ago in the correct bucket', () => {
    const now = Date.now();
    const tasks = [{ status: 'done', startedAt: now - 2 * 3600_000 - 60_000 }];
    const result = buildHourlyActivity(tasks, now);
    const total = result.reduce((sum, h) => sum + h.done, 0);
    expect(total).toBe(1);
    // Should NOT be in the last (current) bucket
    expect(result[result.length - 1].done).toBe(0);
  });

  it('does not count tasks older than 12 hours', () => {
    const now = Date.now();
    const tasks = [{ status: 'done', startedAt: now - 13 * 3600_000 }];
    const result = buildHourlyActivity(tasks, now);
    const total = result.reduce((sum, h) => sum + h.done, 0);
    expect(total).toBe(0);
  });

  it('does not count running tasks', () => {
    const now = Date.now();
    const tasks = [{ status: 'running', startedAt: now - 60_000 }];
    const result = buildHourlyActivity(tasks, now);
    const current = result[result.length - 1];
    expect(current.done).toBe(0);
    expect(current.error).toBe(0);
  });
});
