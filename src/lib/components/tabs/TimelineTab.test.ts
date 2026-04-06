import { describe, it, expect } from 'vitest';

// Pure functions extracted from TimelineTab.svelte for isolated testing.
// No Svelte mounting — all logic is tested as plain TypeScript functions.

// ---------------------------------------------------------------------------
// fmtTokens: formats a raw token count to a human-readable string.
// Mirrors the component function exactly.
// ---------------------------------------------------------------------------
function fmtTokens(n: number): string {
  if (n >= 1_000_000) return (n / 1_000_000).toFixed(1) + 'M';
  if (n >= 1_000)     return (n / 1_000).toFixed(0) + 'K';
  return String(n);
}

// ---------------------------------------------------------------------------
// getErrorText: extracts a display string from an error value.
// Mirrors the component's getErrorText function.
// ---------------------------------------------------------------------------
function getErrorText(err: unknown): string {
  if (typeof err === 'string') return err;
  if (err && typeof err === 'object') {
    const obj = err as Record<string, unknown>;
    if (obj.error) {
      const text = String(obj.error).substring(0, 300);
      return obj.tool ? `[${obj.tool}] ${text}` : text;
    }
    if (obj.message) return String(obj.message);
    return JSON.stringify(err).substring(0, 200);
  }
  return String(err);
}

// ---------------------------------------------------------------------------
// getFileName: extracts a file path string from a file entry.
// Mirrors the component's getFileName function.
// ---------------------------------------------------------------------------
function getFileName(f: unknown): string {
  if (typeof f === 'string') return f;
  if (f && typeof f === 'object') {
    const obj = f as Record<string, unknown>;
    return String(obj.path || obj.file || '');
  }
  return '';
}

// ---------------------------------------------------------------------------
// getFileCount: extracts the edit count from a file entry.
// Mirrors the component's getFileCount function.
// ---------------------------------------------------------------------------
function getFileCount(f: unknown): number {
  if (f && typeof f === 'object') {
    return (f as Record<string, number>).count || 0;
  }
  return 0;
}

// ---------------------------------------------------------------------------
// fileBasename: the component renders fName.split('/').pop() for chip labels.
// ---------------------------------------------------------------------------
function fileBasename(path: string): string | undefined {
  return path.split('/').pop();
}

// ---------------------------------------------------------------------------
// toolCallsTotal: the derived value that sums toolCalls when it is a map.
// Mirrors the summaryItems derived block logic.
// ---------------------------------------------------------------------------
type ToolCallsInput = Record<string, number> | number | undefined | null;

function toolCallsTotal(toolCalls: ToolCallsInput): number | string {
  if (toolCalls && typeof toolCalls === 'object') {
    return Object.values(toolCalls as Record<string, number>).reduce((a, b) => a + b, 0);
  }
  if (typeof toolCalls === 'number') return toolCalls;
  return '-';
}

// ---------------------------------------------------------------------------
// heatmapCellHeight: maps a count to a percentage height, minimum 2%.
// Mirrors: Math.max(2, ((cell.count || 0) / maxHeatCount) * 100)
// ---------------------------------------------------------------------------
function heatmapCellHeight(count: number, maxCount: number): number {
  return Math.max(2, ((count || 0) / maxCount) * 100);
}

// ---------------------------------------------------------------------------
// maxHeatCount: the derived maxHeatCount = Math.max(...heatmap.map(h => h.count || 0), 1)
// Ensures minimum of 1 (avoids divide-by-zero).
// ---------------------------------------------------------------------------
function maxHeatCount(heatmap: { count?: number }[]): number {
  return Math.max(...heatmap.map(h => h.count || 0), 1);
}

// ---------------------------------------------------------------------------
// errorsSlice: the template renders errors.slice(0, 10) — test the guard.
// ---------------------------------------------------------------------------
function errorsSlice<T>(errors: T[], limit = 10): T[] {
  return errors.slice(0, limit);
}

// ---------------------------------------------------------------------------
// filesSlice: the template renders files.slice(0, 20) — test the guard.
// ---------------------------------------------------------------------------
function filesSlice<T>(files: T[], limit = 20): T[] {
  return files.slice(0, limit);
}

// ---------------------------------------------------------------------------
// isEmptyTimeline: the template shows empty-state when:
// !data.eventCount && errors.length === 0 && files.length === 0
// ---------------------------------------------------------------------------
function isEmptyTimeline(eventCount: number | undefined, errorsLen: number, filesLen: number): boolean {
  return !eventCount && errorsLen === 0 && filesLen === 0;
}

// ---------------------------------------------------------------------------
// hasErrors: guard for showing the errors section.
// ---------------------------------------------------------------------------
function hasErrors(errors: unknown[]): boolean {
  return errors.length > 0;
}

// ---------------------------------------------------------------------------
// hasFiles: guard for showing the files section.
// ---------------------------------------------------------------------------
function hasFiles(files: unknown[]): boolean {
  return files.length > 0;
}

// ---------------------------------------------------------------------------
// filterEnabledTargets: mirrors loadTargets() → cfg.machines.filter(m => m.enabled)
// ---------------------------------------------------------------------------
interface MachineEntry { id: string; name: string; enabled: boolean }

function filterEnabledTargets(machines: MachineEntry[]): { id: string; name: string }[] {
  return machines.filter(m => m.enabled).map(m => ({ id: m.id, name: m.name }));
}

// ---------------------------------------------------------------------------
// selectDefaultTarget: mirrors the "pick first if current not in list" logic.
// ---------------------------------------------------------------------------
function selectDefaultTarget(targets: { id: string }[], current: string): string {
  if (targets.length === 0) return current;
  if (targets.find(t => t.id === current)) return current;
  return targets[0].id;
}

// ===========================================================================
// TESTS
// ===========================================================================

// ---------------------------------------------------------------------------
// fmtTokens
// ---------------------------------------------------------------------------

describe('fmtTokens', () => {
  it('returns the raw number as a string for values below 1000', () => {
    expect(fmtTokens(0)).toBe('0');
    expect(fmtTokens(1)).toBe('1');
    expect(fmtTokens(999)).toBe('999');
  });

  it('returns "XK" for values in the thousands', () => {
    expect(fmtTokens(1000)).toBe('1K');
    expect(fmtTokens(1500)).toBe('2K');   // toFixed(0) rounds
    expect(fmtTokens(9999)).toBe('10K');
    expect(fmtTokens(50_000)).toBe('50K');
    expect(fmtTokens(999_999)).toBe('1000K');
  });

  it('returns "X.XM" for values in the millions', () => {
    expect(fmtTokens(1_000_000)).toBe('1.0M');
    expect(fmtTokens(1_500_000)).toBe('1.5M');
    expect(fmtTokens(2_000_000)).toBe('2.0M');
    expect(fmtTokens(10_000_000)).toBe('10.0M');
  });

  it('formats edge case of exactly 1000', () => {
    expect(fmtTokens(1000)).toBe('1K');
  });

  it('formats edge case of exactly 1_000_000', () => {
    expect(fmtTokens(1_000_000)).toBe('1.0M');
  });
});

// ---------------------------------------------------------------------------
// getErrorText
// ---------------------------------------------------------------------------

describe('getErrorText', () => {
  it('returns a plain string as-is', () => {
    expect(getErrorText('something went wrong')).toBe('something went wrong');
  });

  it('returns obj.error as string when present', () => {
    expect(getErrorText({ error: 'disk full' })).toBe('disk full');
  });

  it('prefixes with [tool] when obj.tool is set', () => {
    expect(getErrorText({ error: 'timeout', tool: 'bash' })).toBe('[bash] timeout');
  });

  it('truncates long obj.error strings to 300 chars', () => {
    const longErr = 'e'.repeat(400);
    const result = getErrorText({ error: longErr });
    expect(result).toHaveLength(300);
  });

  it('uses obj.message when obj.error is absent', () => {
    expect(getErrorText({ message: 'connection refused' })).toBe('connection refused');
  });

  it('falls back to JSON.stringify for unknown objects', () => {
    const obj = { code: 42 };
    expect(getErrorText(obj)).toBe(JSON.stringify(obj));
  });

  it('handles null gracefully', () => {
    expect(getErrorText(null)).toBe('null');
  });

  it('handles a number', () => {
    expect(getErrorText(500)).toBe('500');
  });

  it('truncates JSON.stringify output to 200 chars', () => {
    const bigObj: Record<string, string> = {};
    for (let i = 0; i < 50; i++) bigObj[`key${i}`] = 'val'.repeat(5);
    const result = getErrorText(bigObj);
    expect(result.length).toBeLessThanOrEqual(200);
  });
});

// ---------------------------------------------------------------------------
// getFileName
// ---------------------------------------------------------------------------

describe('getFileName', () => {
  it('returns a plain string as-is', () => {
    expect(getFileName('/src/lib/api.ts')).toBe('/src/lib/api.ts');
  });

  it('returns obj.path when available', () => {
    expect(getFileName({ path: '/foo/bar.ts', file: '/baz.ts' })).toBe('/foo/bar.ts');
  });

  it('returns obj.file when path is absent', () => {
    expect(getFileName({ file: '/baz.ts' })).toBe('/baz.ts');
  });

  it('returns empty string when neither path nor file', () => {
    expect(getFileName({ count: 3 })).toBe('');
  });

  it('returns empty string for null', () => {
    expect(getFileName(null)).toBe('');
  });

  it('returns empty string for undefined', () => {
    expect(getFileName(undefined)).toBe('');
  });
});

// ---------------------------------------------------------------------------
// getFileCount
// ---------------------------------------------------------------------------

describe('getFileCount', () => {
  it('returns the count property from an object', () => {
    expect(getFileCount({ path: '/foo.ts', count: 7 })).toBe(7);
  });

  it('returns 0 when count is absent', () => {
    expect(getFileCount({ path: '/foo.ts' })).toBe(0);
  });

  it('returns 0 for a plain string', () => {
    expect(getFileCount('/src/lib/api.ts')).toBe(0);
  });

  it('returns 0 for null', () => {
    expect(getFileCount(null)).toBe(0);
  });

  it('returns 0 for undefined', () => {
    expect(getFileCount(undefined)).toBe(0);
  });

  it('returns 0 when count is 0', () => {
    expect(getFileCount({ count: 0 })).toBe(0);
  });
});

// ---------------------------------------------------------------------------
// fileBasename
// ---------------------------------------------------------------------------

describe('fileBasename', () => {
  it('returns the last segment of a path', () => {
    expect(fileBasename('/src/lib/api.ts')).toBe('api.ts');
  });

  it('returns the filename for a relative path', () => {
    expect(fileBasename('components/Header.svelte')).toBe('Header.svelte');
  });

  it('returns the filename for a flat name', () => {
    expect(fileBasename('index.ts')).toBe('index.ts');
  });

  it('returns empty string for a path ending in /', () => {
    expect(fileBasename('/src/')).toBe('');
  });
});

// ---------------------------------------------------------------------------
// toolCallsTotal
// ---------------------------------------------------------------------------

describe('toolCallsTotal', () => {
  it('sums values when toolCalls is a map', () => {
    expect(toolCallsTotal({ bash: 10, read: 5, write: 3 })).toBe(18);
  });

  it('returns a number directly when toolCalls is a number', () => {
    expect(toolCallsTotal(42)).toBe(42);
  });

  it('returns "-" for undefined', () => {
    expect(toolCallsTotal(undefined)).toBe('-');
  });

  it('returns "-" for null', () => {
    expect(toolCallsTotal(null)).toBe('-');
  });

  it('returns 0 for an empty map', () => {
    expect(toolCallsTotal({})).toBe(0);
  });

  it('handles a map with a single tool', () => {
    expect(toolCallsTotal({ bash: 7 })).toBe(7);
  });
});

// ---------------------------------------------------------------------------
// heatmapCellHeight
// ---------------------------------------------------------------------------

describe('heatmapCellHeight', () => {
  it('returns 100% for a cell at maxCount', () => {
    expect(heatmapCellHeight(50, 50)).toBe(100);
  });

  it('returns 50% for a cell at half of maxCount', () => {
    expect(heatmapCellHeight(25, 50)).toBe(50);
  });

  it('enforces a minimum of 2% for zero count', () => {
    expect(heatmapCellHeight(0, 50)).toBe(2);
  });

  it('enforces minimum of 2% even for very low counts', () => {
    expect(heatmapCellHeight(1, 1000)).toBe(2);
  });

  it('handles count equal to 1 with maxCount 1', () => {
    expect(heatmapCellHeight(1, 1)).toBe(100);
  });
});

// ---------------------------------------------------------------------------
// maxHeatCount
// ---------------------------------------------------------------------------

describe('maxHeatCount', () => {
  it('returns 1 for an empty heatmap (avoids divide-by-zero)', () => {
    expect(maxHeatCount([])).toBe(1);
  });

  it('returns 1 when all counts are 0', () => {
    expect(maxHeatCount([{ count: 0 }, { count: 0 }])).toBe(1);
  });

  it('returns the maximum count', () => {
    expect(maxHeatCount([{ count: 3 }, { count: 10 }, { count: 5 }])).toBe(10);
  });

  it('handles cells without count property', () => {
    expect(maxHeatCount([{}, { count: 7 }])).toBe(7);
  });

  it('returns 1 for a heatmap with a single zero-count cell', () => {
    expect(maxHeatCount([{ count: 0 }])).toBe(1);
  });
});

// ---------------------------------------------------------------------------
// errorsSlice
// ---------------------------------------------------------------------------

describe('errorsSlice', () => {
  it('returns up to 10 errors', () => {
    const errors = Array.from({ length: 15 }, (_, i) => `error ${i}`);
    expect(errorsSlice(errors)).toHaveLength(10);
  });

  it('returns all errors when fewer than 10', () => {
    const errors = ['e1', 'e2', 'e3'];
    expect(errorsSlice(errors)).toHaveLength(3);
  });

  it('returns empty for an empty list', () => {
    expect(errorsSlice([])).toHaveLength(0);
  });

  it('respects a custom limit', () => {
    const errors = ['e1', 'e2', 'e3', 'e4', 'e5'];
    expect(errorsSlice(errors, 3)).toHaveLength(3);
  });
});

// ---------------------------------------------------------------------------
// filesSlice
// ---------------------------------------------------------------------------

describe('filesSlice', () => {
  it('returns up to 20 files', () => {
    const files = Array.from({ length: 30 }, (_, i) => `file${i}.ts`);
    expect(filesSlice(files)).toHaveLength(20);
  });

  it('returns all files when fewer than 20', () => {
    const files = ['a.ts', 'b.ts'];
    expect(filesSlice(files)).toHaveLength(2);
  });

  it('returns empty for an empty list', () => {
    expect(filesSlice([])).toHaveLength(0);
  });
});

// ---------------------------------------------------------------------------
// isEmptyTimeline
// ---------------------------------------------------------------------------

describe('isEmptyTimeline', () => {
  it('returns true when eventCount is 0 and no errors or files', () => {
    expect(isEmptyTimeline(0, 0, 0)).toBe(true);
  });

  it('returns true when eventCount is undefined and no errors or files', () => {
    expect(isEmptyTimeline(undefined, 0, 0)).toBe(true);
  });

  it('returns false when eventCount > 0', () => {
    expect(isEmptyTimeline(5, 0, 0)).toBe(false);
  });

  it('returns false when errors are present', () => {
    expect(isEmptyTimeline(0, 2, 0)).toBe(false);
  });

  it('returns false when files are present', () => {
    expect(isEmptyTimeline(0, 0, 3)).toBe(false);
  });

  it('returns false when all three have values', () => {
    expect(isEmptyTimeline(10, 2, 5)).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// hasErrors / hasFiles
// ---------------------------------------------------------------------------

describe('hasErrors', () => {
  it('returns false for empty array', () => {
    expect(hasErrors([])).toBe(false);
  });

  it('returns true when errors are present', () => {
    expect(hasErrors(['err1', 'err2'])).toBe(true);
  });
});

describe('hasFiles', () => {
  it('returns false for empty array', () => {
    expect(hasFiles([])).toBe(false);
  });

  it('returns true when files are present', () => {
    expect(hasFiles(['/src/api.ts'])).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// filterEnabledTargets
// ---------------------------------------------------------------------------

describe('filterEnabledTargets', () => {
  const machines: MachineEntry[] = [
    { id: 'atlas', name: 'ATLAS', enabled: true },
    { id: 'pixel', name: 'PIXEL', enabled: true },
    { id: 'offline', name: 'OFFLINE', enabled: false },
  ];

  it('returns only enabled machines', () => {
    const result = filterEnabledTargets(machines);
    expect(result).toHaveLength(2);
    expect(result.map(t => t.id)).toEqual(['atlas', 'pixel']);
  });

  it('maps to {id, name} objects', () => {
    const result = filterEnabledTargets([{ id: 'atlas', name: 'ATLAS', enabled: true }]);
    expect(result[0]).toEqual({ id: 'atlas', name: 'ATLAS' });
  });

  it('returns empty array when all machines are disabled', () => {
    const disabled = [{ id: 'x', name: 'X', enabled: false }];
    expect(filterEnabledTargets(disabled)).toHaveLength(0);
  });

  it('returns empty array for empty input', () => {
    expect(filterEnabledTargets([])).toHaveLength(0);
  });
});

// ---------------------------------------------------------------------------
// selectDefaultTarget
// ---------------------------------------------------------------------------

describe('selectDefaultTarget', () => {
  const targets = [{ id: 'atlas' }, { id: 'pixel' }];

  it('keeps current target when it is in the list', () => {
    expect(selectDefaultTarget(targets, 'pixel')).toBe('pixel');
  });

  it('switches to first target when current is not in the list', () => {
    expect(selectDefaultTarget(targets, 'unknown')).toBe('atlas');
  });

  it('returns current when targets list is empty', () => {
    expect(selectDefaultTarget([], 'atlas')).toBe('atlas');
  });

  it('picks first when current is empty string', () => {
    expect(selectDefaultTarget(targets, '')).toBe('atlas');
  });
});
