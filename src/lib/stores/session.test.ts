import { describe, it, expect, vi, beforeAll } from 'vitest';

// The session module imports from svelte/store and ../api (Tauri), which aren't
// available in the node test environment. We mock both before importing.
vi.mock('svelte/store', () => ({
  writable: (v: unknown) => ({ subscribe: vi.fn(), set: vi.fn(), update: vi.fn(), _val: v }),
  get: (store: { _val: unknown }) => store._val,
}));

vi.mock('../api', () => ({
  fetchSessionData: vi.fn(),
  fetchAtlasActivity: vi.fn(),
  fetchPixelActivity: vi.fn(),
  onSessionUpdate: vi.fn(),
  onActivityUpdate: vi.fn(),
  onCommitsUpdate: vi.fn(),
}));

// localStorage is not present in node environment — provide a minimal stub
if (typeof globalThis.localStorage === 'undefined') {
  const store: Record<string, string> = {};
  Object.defineProperty(globalThis, 'localStorage', {
    value: {
      getItem: (k: string) => store[k] ?? null,
      setItem: (k: string, v: string) => { store[k] = v; },
      removeItem: (k: string) => { delete store[k]; },
    },
  });
}

import { activityKey, mergeActivities } from './session';
import type { Activity } from '../types';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------
function makeActivity(overrides: Partial<Activity>): Activity {
  return { type: 'text', content: '', ...overrides };
}

function makeTool(name: string, detail = ''): Activity {
  return { type: 'tool', name, detail };
}

// ---------------------------------------------------------------------------
// activityKey
// ---------------------------------------------------------------------------
describe('activityKey', () => {
  it('returns t:name:detail for tool activities', () => {
    expect(activityKey(makeTool('Bash', 'ls -la'))).toBe('t:Bash:ls -la');
  });

  it('uses empty string for missing detail in tool activities', () => {
    expect(activityKey(makeTool('Read'))).toBe('t:Read:');
  });

  it('returns p: prefix for prompt activities', () => {
    const a = makeActivity({ type: 'prompt', content: 'Write a test' });
    expect(activityKey(a)).toBe('p:Write a test');
  });

  it('returns x: prefix for other (text) activities', () => {
    const a = makeActivity({ type: 'text', content: 'Some output' });
    expect(activityKey(a)).toBe('x:Some output');
  });

  it('truncates content at 100 characters for prompt activities', () => {
    const long = 'A'.repeat(150);
    const a = makeActivity({ type: 'prompt', content: long });
    const key = activityKey(a);
    expect(key).toBe('p:' + 'A'.repeat(100));
    expect(key.length).toBe(102); // 'p:' + 100 chars
  });

  it('truncates content at 100 characters for text activities', () => {
    const long = 'B'.repeat(200);
    const a = makeActivity({ type: 'text', content: long });
    const key = activityKey(a);
    expect(key).toBe('x:' + 'B'.repeat(100));
  });

  it('handles missing content gracefully (empty string fallback)', () => {
    const a = makeActivity({ type: 'prompt', content: undefined });
    expect(activityKey(a)).toBe('p:');
  });
});

// ---------------------------------------------------------------------------
// mergeActivities
// ---------------------------------------------------------------------------
describe('mergeActivities', () => {
  it('returns incoming (capped at 500) when existing is empty', () => {
    const incoming: Activity[] = Array.from({ length: 10 }, (_, i) =>
      makeActivity({ content: `item-${i}` })
    );
    const result = mergeActivities([], incoming);
    expect(result).toHaveLength(10);
  });

  it('caps incoming at 500 when existing is empty and incoming is very large', () => {
    const incoming: Activity[] = Array.from({ length: 600 }, (_, i) =>
      makeActivity({ content: `item-${i}` })
    );
    const result = mergeActivities([], incoming);
    expect(result).toHaveLength(500);
    // Should keep the last 500
    expect(result[0]).toEqual(makeActivity({ content: 'item-100' }));
  });

  it('returns existing unchanged when incoming is empty', () => {
    const existing: Activity[] = [makeActivity({ content: 'old' })];
    const result = mergeActivities(existing, []);
    expect(result).toBe(existing); // same reference
  });

  it('deduplicates: does not add activities already in existing', () => {
    const a = makeActivity({ content: 'hello' });
    const existing = [a];
    const incoming = [a, makeActivity({ content: 'world' })];
    const result = mergeActivities(existing, incoming);
    expect(result).toHaveLength(2);
    expect(result[1].content).toBe('world');
  });

  it('appends genuinely new items to existing', () => {
    const existing: Activity[] = [makeActivity({ content: 'first' })];
    const incoming: Activity[] = [
      makeActivity({ content: 'first' }),  // duplicate
      makeActivity({ content: 'second' }), // new
      makeActivity({ content: 'third' }),  // new
    ];
    const result = mergeActivities(existing, incoming);
    expect(result).toHaveLength(3);
    expect(result.map(a => a.content)).toEqual(['first', 'second', 'third']);
  });

  it('returns existing unchanged when all incoming items are duplicates', () => {
    const a = makeActivity({ content: 'dup' });
    const existing = [a];
    const result = mergeActivities(existing, [a]);
    expect(result).toBe(existing); // same reference, no allocation
  });

  it('caps merged result at 500 items', () => {
    const existing: Activity[] = Array.from({ length: 490 }, (_, i) =>
      makeActivity({ content: `existing-${i}` })
    );
    const incoming: Activity[] = Array.from({ length: 20 }, (_, i) =>
      makeActivity({ content: `new-${i}` })
    );
    const result = mergeActivities(existing, incoming);
    expect(result).toHaveLength(500);
  });

  it('deduplicates tool activities by name and detail', () => {
    const existing = [makeTool('Bash', 'ls')];
    const incoming = [makeTool('Bash', 'ls'), makeTool('Bash', 'pwd')];
    const result = mergeActivities(existing, incoming);
    expect(result).toHaveLength(2);
    expect(result[1].detail).toBe('pwd');
  });
});
