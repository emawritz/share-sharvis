import { describe, it, expect, vi, beforeEach } from 'vitest';

// Pure functions extracted from TasksTab.svelte for isolated testing.
// No Svelte mounting — all logic is tested as plain TypeScript functions.

// statusBadgeClass: maps task status → CSS class.
function statusBadgeClass(status: string): string {
  switch (status) {
    case 'running': return 'sb-running';
    case 'done': return 'sb-done';
    case 'error': case 'killed': return 'sb-error';
    case 'timeout': return 'sb-timeout';
    case 'pending': return 'sb-pending';
    default: return 'sb-pending';
  }
}

// targetBadgeClass: maps task target → CSS class.
function targetBadgeClass(target: string): string {
  switch (target) {
    case 'atlas': return 'tb-atlas';
    case 'pixel': return 'tb-pixel';
    case 'both': return 'tb-both';
    default: return 'tb-atlas';
  }
}

// taskDuration: returns human-readable duration string or null if incomplete.
function taskDuration(task: { startedAt?: number; finishedAt?: number }): string | null {
  if (!task.startedAt || !task.finishedAt) return null;
  const secs = Math.round((task.finishedAt - task.startedAt) / 1000);
  if (secs < 60) return `${secs}s`;
  const m = Math.floor(secs / 60);
  const s = secs % 60;
  return `${m}m${s}s`;
}

// formatDuration: formats elapsed_secs from history entries.
function formatDuration(secs: number): string {
  if (secs < 60) return `${secs}s`;
  const m = Math.floor(secs / 60);
  const s = secs % 60;
  return `${m}m${s}s`;
}

// getDisplay: strips the pixel task marker and trailing whitespace from output.
function getDisplay(output: string): string {
  const mi = output.indexOf('===PIXEL-TASK===');
  if (mi !== -1) return output.substring(0, mi).trim();
  return output;
}

// hasDeps: returns true when a task has one or more dependency ids.
function hasDeps(task: { dependsOn?: number[] }): boolean {
  return !!task.dependsOn && task.dependsOn.length > 0;
}

// conditionLabel: converts run condition key to display label.
function conditionLabel(cond?: string): string {
  switch (cond) {
    case 'on_success': return 'If OK';
    case 'on_failure': return 'If Fail';
    case 'always': return 'Always';
    default: return '';
  }
}

// statusLabel: converts task status to display label.
function statusLabel(status: string): string {
  switch (status) {
    case 'running': return 'Active';
    case 'pending': return 'Pending';
    case 'error': return 'Error';
    case 'killed': return 'Killed';
    case 'timeout': return 'Timeout';
    default: return 'Done';
  }
}

// formatTime: formats ISO timestamp to HH:MM string.
function formatTime(ts: string): string {
  try {
    const d = new Date(ts);
    return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  } catch { return ts; }
}

// promptSlice: how the component truncates the prompt in the history row.
function promptSlice(prompt: string, limit = 60): string {
  return prompt.slice(0, limit);
}

// filterTasks: implements the derived filteredTasks logic from the component.
interface Task {
  id: number;
  prompt: string;
  target: string;
  status: string;
  output?: string;
  startedAt?: number;
}

function filterTasks(
  tasks: Task[],
  searchQuery: string,
  statusFilter: string,
  targetFilter: string,
  sortOrder: 'newest' | 'oldest'
): Task[] {
  const q = searchQuery.toLowerCase().trim();
  let result = [...tasks];

  if (q) {
    result = result.filter((t) =>
      t.prompt.toLowerCase().includes(q) ||
      t.target.toLowerCase().includes(q) ||
      t.status.toLowerCase().includes(q) ||
      (t.output && t.output.toLowerCase().includes(q))
    );
  }

  if (statusFilter !== 'all') {
    result = result.filter((t) => t.status === statusFilter);
  }

  if (targetFilter !== 'all') {
    result = result.filter((t) => t.target === targetFilter);
  }

  result.sort((a, b) => {
    const aTime = a.startedAt ?? a.id;
    const bTime = b.startedAt ?? b.id;
    return sortOrder === 'newest' ? bTime - aTime : aTime - bTime;
  });

  return result;
}

// ---------------------------------------------------------------------------
// statusBadgeClass
// ---------------------------------------------------------------------------

describe('statusBadgeClass', () => {
  it('maps "running" to sb-running', () => {
    expect(statusBadgeClass('running')).toBe('sb-running');
  });

  it('maps "done" to sb-done', () => {
    expect(statusBadgeClass('done')).toBe('sb-done');
  });

  it('maps "error" to sb-error', () => {
    expect(statusBadgeClass('error')).toBe('sb-error');
  });

  it('maps "killed" to sb-error', () => {
    expect(statusBadgeClass('killed')).toBe('sb-error');
  });

  it('maps "timeout" to sb-timeout', () => {
    expect(statusBadgeClass('timeout')).toBe('sb-timeout');
  });

  it('maps "pending" to sb-pending', () => {
    expect(statusBadgeClass('pending')).toBe('sb-pending');
  });

  it('falls back to sb-pending for unknown status', () => {
    expect(statusBadgeClass('cancelled')).toBe('sb-pending');
    expect(statusBadgeClass('')).toBe('sb-pending');
  });
});

// ---------------------------------------------------------------------------
// targetBadgeClass
// ---------------------------------------------------------------------------

describe('targetBadgeClass', () => {
  it('maps "atlas" to tb-atlas', () => {
    expect(targetBadgeClass('atlas')).toBe('tb-atlas');
  });

  it('maps "pixel" to tb-pixel', () => {
    expect(targetBadgeClass('pixel')).toBe('tb-pixel');
  });

  it('maps "both" to tb-both', () => {
    expect(targetBadgeClass('both')).toBe('tb-both');
  });

  it('defaults to tb-atlas for unknown target', () => {
    expect(targetBadgeClass('unknown')).toBe('tb-atlas');
    expect(targetBadgeClass('')).toBe('tb-atlas');
  });
});

// ---------------------------------------------------------------------------
// taskDuration
// ---------------------------------------------------------------------------

describe('taskDuration', () => {
  it('returns null when startedAt is missing', () => {
    expect(taskDuration({ finishedAt: 1_000_000 })).toBeNull();
  });

  it('returns null when finishedAt is missing', () => {
    expect(taskDuration({ startedAt: 1_000_000 })).toBeNull();
  });

  it('returns null when both are missing', () => {
    expect(taskDuration({})).toBeNull();
  });

  it('returns seconds for short tasks', () => {
    expect(taskDuration({ startedAt: 1_000_000, finishedAt: 1_030_000 })).toBe('30s');
    expect(taskDuration({ startedAt: 1_000_000, finishedAt: 1_001_000 })).toBe('1s');
  });

  it('returns minutes+seconds for longer tasks', () => {
    expect(taskDuration({ startedAt: 1_000_000, finishedAt: 1_090_000 })).toBe('1m30s');
    expect(taskDuration({ startedAt: 1_000_000, finishedAt: 1_060_000 })).toBe('1m0s');
  });

  it('formats 2m 30s correctly', () => {
    expect(taskDuration({ startedAt: 1_000_000, finishedAt: 1_150_000 })).toBe('2m30s');
  });

  it('rounds to nearest second', () => {
    // 30.4s → 30s, 30.6s → 31s
    expect(taskDuration({ startedAt: 1_000_000, finishedAt: 1_030_400 })).toBe('30s');
    expect(taskDuration({ startedAt: 1_000_000, finishedAt: 1_030_600 })).toBe('31s');
  });
});

// ---------------------------------------------------------------------------
// formatDuration (history entries)
// ---------------------------------------------------------------------------

describe('formatDuration', () => {
  it('returns seconds for values below 60', () => {
    expect(formatDuration(0)).toBe('0s');
    expect(formatDuration(1)).toBe('1s');
    expect(formatDuration(59)).toBe('59s');
  });

  it('returns minutes+seconds for 60+', () => {
    expect(formatDuration(60)).toBe('1m0s');
    expect(formatDuration(90)).toBe('1m30s');
    expect(formatDuration(3600)).toBe('60m0s');
  });

  it('handles 2m 30s', () => {
    expect(formatDuration(150)).toBe('2m30s');
  });
});

// ---------------------------------------------------------------------------
// getDisplay
// ---------------------------------------------------------------------------

describe('getDisplay', () => {
  it('returns output unchanged when marker is absent', () => {
    expect(getDisplay('hello world')).toBe('hello world');
  });

  it('strips everything from ===PIXEL-TASK=== onwards', () => {
    const raw = 'atlas output\n===PIXEL-TASK===\npixel output';
    expect(getDisplay(raw)).toBe('atlas output');
  });

  it('trims leading/trailing whitespace before the marker', () => {
    const raw = '  atlas output  \n===PIXEL-TASK===\npixel output';
    expect(getDisplay(raw)).toBe('atlas output');
  });

  it('returns empty string when output starts with marker', () => {
    expect(getDisplay('===PIXEL-TASK===\npixel stuff')).toBe('');
  });

  it('handles output with no marker and no whitespace', () => {
    expect(getDisplay('done')).toBe('done');
  });
});

// ---------------------------------------------------------------------------
// hasDeps
// ---------------------------------------------------------------------------

describe('hasDeps', () => {
  it('returns true when dependsOn has at least one id', () => {
    expect(hasDeps({ dependsOn: [1] })).toBe(true);
    expect(hasDeps({ dependsOn: [1, 2, 3] })).toBe(true);
  });

  it('returns false when dependsOn is an empty array', () => {
    expect(hasDeps({ dependsOn: [] })).toBe(false);
  });

  it('returns false when dependsOn is undefined', () => {
    expect(hasDeps({})).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// conditionLabel
// ---------------------------------------------------------------------------

describe('conditionLabel', () => {
  it('maps on_success to "If OK"', () => {
    expect(conditionLabel('on_success')).toBe('If OK');
  });

  it('maps on_failure to "If Fail"', () => {
    expect(conditionLabel('on_failure')).toBe('If Fail');
  });

  it('maps always to "Always"', () => {
    expect(conditionLabel('always')).toBe('Always');
  });

  it('returns empty string for undefined', () => {
    expect(conditionLabel(undefined)).toBe('');
  });

  it('returns empty string for unknown condition', () => {
    expect(conditionLabel('never')).toBe('');
  });
});

// ---------------------------------------------------------------------------
// statusLabel
// ---------------------------------------------------------------------------

describe('statusLabel', () => {
  it('maps "running" to "Active"', () => {
    expect(statusLabel('running')).toBe('Active');
  });

  it('maps "pending" to "Pending"', () => {
    expect(statusLabel('pending')).toBe('Pending');
  });

  it('maps "error" to "Error"', () => {
    expect(statusLabel('error')).toBe('Error');
  });

  it('maps "killed" to "Killed"', () => {
    expect(statusLabel('killed')).toBe('Killed');
  });

  it('maps "timeout" to "Timeout"', () => {
    expect(statusLabel('timeout')).toBe('Timeout');
  });

  it('maps "done" to "Done"', () => {
    expect(statusLabel('done')).toBe('Done');
  });

  it('falls back to "Done" for unknown status', () => {
    expect(statusLabel('unknown')).toBe('Done');
    expect(statusLabel('')).toBe('Done');
  });
});

// ---------------------------------------------------------------------------
// promptSlice
// ---------------------------------------------------------------------------

describe('promptSlice', () => {
  it('returns full prompt when shorter than limit', () => {
    expect(promptSlice('short prompt', 60)).toBe('short prompt');
  });

  it('truncates at exactly 60 characters', () => {
    const long = 'a'.repeat(80);
    expect(promptSlice(long, 60)).toHaveLength(60);
  });

  it('preserves prompts exactly at the limit', () => {
    const exact = 'x'.repeat(60);
    expect(promptSlice(exact, 60)).toBe(exact);
  });

  it('handles empty string', () => {
    expect(promptSlice('', 60)).toBe('');
  });
});

// ---------------------------------------------------------------------------
// filterTasks
// ---------------------------------------------------------------------------

const TASKS: Task[] = [
  { id: 1, prompt: 'Run linter', target: 'atlas', status: 'done', startedAt: 1000 },
  { id: 2, prompt: 'Run tests', target: 'pixel', status: 'running', startedAt: 2000 },
  { id: 3, prompt: 'Deploy app', target: 'atlas', status: 'error', startedAt: 3000 },
  { id: 4, prompt: 'Build frontend', target: 'pixel', status: 'pending', startedAt: 4000 },
  { id: 5, prompt: 'Fix bug in auth', target: 'atlas', status: 'done', startedAt: 5000, output: 'fixed auth issue' },
];

describe('filterTasks — search query', () => {
  it('returns all tasks when query is empty', () => {
    expect(filterTasks(TASKS, '', 'all', 'all', 'newest')).toHaveLength(5);
  });

  it('filters by prompt text (case-insensitive)', () => {
    const result = filterTasks(TASKS, 'lint', 'all', 'all', 'newest');
    expect(result).toHaveLength(1);
    expect(result[0].id).toBe(1);
  });

  it('filters by target text', () => {
    const result = filterTasks(TASKS, 'pixel', 'all', 'all', 'newest');
    expect(result.every(t => t.target === 'pixel')).toBe(true);
  });

  it('filters by status text', () => {
    const result = filterTasks(TASKS, 'running', 'all', 'all', 'newest');
    expect(result.every(t => t.status === 'running')).toBe(true);
  });

  it('filters by output content', () => {
    const result = filterTasks(TASKS, 'auth issue', 'all', 'all', 'newest');
    expect(result).toHaveLength(1);
    expect(result[0].id).toBe(5);
  });

  it('returns empty array when no tasks match', () => {
    expect(filterTasks(TASKS, 'xyznonexistent', 'all', 'all', 'newest')).toHaveLength(0);
  });
});

describe('filterTasks — status filter', () => {
  it('filters to only done tasks', () => {
    const result = filterTasks(TASKS, '', 'done', 'all', 'newest');
    expect(result.every(t => t.status === 'done')).toBe(true);
    expect(result).toHaveLength(2);
  });

  it('filters to only running tasks', () => {
    const result = filterTasks(TASKS, '', 'running', 'all', 'newest');
    expect(result).toHaveLength(1);
    expect(result[0].status).toBe('running');
  });

  it('filters to only error tasks', () => {
    const result = filterTasks(TASKS, '', 'error', 'all', 'newest');
    expect(result).toHaveLength(1);
    expect(result[0].status).toBe('error');
  });

  it('"all" status filter returns all tasks', () => {
    expect(filterTasks(TASKS, '', 'all', 'all', 'newest')).toHaveLength(5);
  });
});

describe('filterTasks — target filter', () => {
  it('filters to only atlas tasks', () => {
    const result = filterTasks(TASKS, '', 'all', 'atlas', 'newest');
    expect(result.every(t => t.target === 'atlas')).toBe(true);
    expect(result).toHaveLength(3);
  });

  it('filters to only pixel tasks', () => {
    const result = filterTasks(TASKS, '', 'all', 'pixel', 'newest');
    expect(result.every(t => t.target === 'pixel')).toBe(true);
    expect(result).toHaveLength(2);
  });

  it('"all" target filter returns all tasks', () => {
    expect(filterTasks(TASKS, '', 'all', 'all', 'newest')).toHaveLength(5);
  });
});

describe('filterTasks — sort order', () => {
  it('newest sort returns highest startedAt first', () => {
    const result = filterTasks(TASKS, '', 'all', 'all', 'newest');
    expect(result[0].startedAt).toBeGreaterThanOrEqual(result[1].startedAt!);
  });

  it('oldest sort returns lowest startedAt first', () => {
    const result = filterTasks(TASKS, '', 'all', 'all', 'oldest');
    expect(result[0].startedAt).toBeLessThanOrEqual(result[1].startedAt!);
  });

  it('newest sort: last element has the smallest startedAt', () => {
    const result = filterTasks(TASKS, '', 'all', 'all', 'newest');
    const last = result[result.length - 1];
    expect(last.startedAt).toBe(1000);
  });

  it('oldest sort: first element has the smallest startedAt', () => {
    const result = filterTasks(TASKS, '', 'all', 'all', 'oldest');
    expect(result[0].startedAt).toBe(1000);
  });
});
