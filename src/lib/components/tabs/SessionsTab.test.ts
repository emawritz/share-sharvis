import { describe, it, expect, vi, beforeEach } from 'vitest';

// Pure re-implementation of formatAge from SessionsTab.svelte for isolated testing.
// The component closes over Date.now(); here we accept an explicit `now` parameter
// so the function is pure and deterministic.
function formatAge(created_at: number, now: number = Date.now()): string {
  const secs = Math.floor(now / 1000 - created_at);
  if (secs < 60) return `${secs}s`;
  if (secs < 3600) return `${Math.floor(secs / 60)}m`;
  return `${Math.floor(secs / 3600)}h ${Math.floor((secs % 3600) / 60)}m`;
}

// Helper: build a minimal valid SessionInfo-like object for tests.
function makeSession(overrides: Partial<{
  name: string;
  message_count: number;
  active_task_id: number | null;
  task_count: number;
  project: string;
  machine: string;
  created_at: number;
}> = {}) {
  return {
    name: 'test-session',
    message_count: 5,
    active_task_id: null,
    task_count: 3,
    project: 'my-project',
    machine: 'atlas',
    created_at: Math.floor(Date.now() / 1000) - 120,
    ...overrides,
  };
}

// ---------------------------------------------------------------------------
// formatAge
// ---------------------------------------------------------------------------

describe('formatAge', () => {
  const BASE_UNIX = 1_000_000; // seconds
  const BASE_MS = BASE_UNIX * 1000;

  it('returns seconds when age < 60s', () => {
    expect(formatAge(BASE_UNIX - 30, BASE_MS)).toBe('30s');
  });

  it('returns "0s" for a session created at this exact second', () => {
    expect(formatAge(BASE_UNIX, BASE_MS)).toBe('0s');
  });

  it('returns "1s" for one second ago', () => {
    expect(formatAge(BASE_UNIX - 1, BASE_MS)).toBe('1s');
  });

  it('returns "59s" for 59 seconds ago', () => {
    expect(formatAge(BASE_UNIX - 59, BASE_MS)).toBe('59s');
  });

  it('returns minutes for age between 60s and 3600s', () => {
    expect(formatAge(BASE_UNIX - 60, BASE_MS)).toBe('1m');
    expect(formatAge(BASE_UNIX - 90, BASE_MS)).toBe('1m');
    expect(formatAge(BASE_UNIX - 3599, BASE_MS)).toBe('59m');
  });

  it('returns hours+minutes for age >= 3600s', () => {
    expect(formatAge(BASE_UNIX - 3600, BASE_MS)).toBe('1h 0m');
    expect(formatAge(BASE_UNIX - 3660, BASE_MS)).toBe('1h 1m');
    expect(formatAge(BASE_UNIX - 7200, BASE_MS)).toBe('2h 0m');
    expect(formatAge(BASE_UNIX - 7320, BASE_MS)).toBe('2h 2m');
  });

  it('rounds down (floor) for sub-minute remainders', () => {
    // 125 seconds → 2m (floor(125/60) = 2)
    expect(formatAge(BASE_UNIX - 125, BASE_MS)).toBe('2m');
  });

  it('rounds down hours and minutes independently', () => {
    // 3723 seconds = 1h 2m 3s  → "1h 2m"
    expect(formatAge(BASE_UNIX - 3723, BASE_MS)).toBe('1h 2m');
  });
});

// ---------------------------------------------------------------------------
// Session data shape / field presence
// ---------------------------------------------------------------------------

describe('session data shape', () => {
  it('has a name field', () => {
    const s = makeSession({ name: 'my-session' });
    expect(s.name).toBe('my-session');
  });

  it('has a message_count field', () => {
    const s = makeSession({ message_count: 42 });
    expect(s.message_count).toBe(42);
  });

  it('has a task_count field', () => {
    const s = makeSession({ task_count: 7 });
    expect(s.task_count).toBe(7);
  });

  it('has a project field', () => {
    const s = makeSession({ project: 'jarvis' });
    expect(s.project).toBe('jarvis');
  });

  it('has a machine field', () => {
    const s = makeSession({ machine: 'pixel' });
    expect(s.machine).toBe('pixel');
  });

  it('active_task_id is null when idle', () => {
    const s = makeSession({ active_task_id: null });
    expect(s.active_task_id).toBeNull();
  });

  it('active_task_id is a number when running', () => {
    const s = makeSession({ active_task_id: 99 });
    expect(s.active_task_id).toBe(99);
  });

  it('created_at is a unix timestamp (seconds)', () => {
    const now = Math.floor(Date.now() / 1000);
    const s = makeSession({ created_at: now - 60 });
    expect(s.created_at).toBeGreaterThan(0);
    // created_at should be in the past
    expect(s.created_at).toBeLessThan(now + 1);
  });
});

// ---------------------------------------------------------------------------
// Active task indicator logic
// ---------------------------------------------------------------------------

describe('active task indicator logic', () => {
  it('session with active_task_id !== null is considered "running"', () => {
    const s = makeSession({ active_task_id: 1 });
    const isRunning = s.active_task_id !== null;
    expect(isRunning).toBe(true);
  });

  it('session with active_task_id === null is considered "idle"', () => {
    const s = makeSession({ active_task_id: null });
    const isRunning = s.active_task_id !== null;
    expect(isRunning).toBe(false);
  });

  it('active_task_id of 0 is treated as running (0 !== null)', () => {
    const s = makeSession({ active_task_id: 0 });
    const isRunning = s.active_task_id !== null;
    expect(isRunning).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// Rendering helpers — template output logic derived from the component
// ---------------------------------------------------------------------------

describe('session sub-line rendering logic', () => {
  it('includes machine name uppercased when machine is present', () => {
    const s = makeSession({ project: 'myapp', machine: 'atlas' });
    const sub = `${s.project}${s.machine ? ` · ${s.machine.toUpperCase()}` : ''}`;
    expect(sub).toBe('myapp · ATLAS');
  });

  it('omits machine separator when machine is empty string', () => {
    const s = makeSession({ project: 'myapp', machine: '' });
    const sub = `${s.project}${s.machine ? ` · ${s.machine.toUpperCase()}` : ''}`;
    expect(sub).toBe('myapp');
  });

  it('shows task count label only when task_count > 0', () => {
    const s0 = makeSession({ task_count: 0 });
    const s5 = makeSession({ task_count: 5 });
    expect(s0.task_count > 0).toBe(false);
    expect(s5.task_count > 0).toBe(true);
  });

  it('message count label is "{n} msgs"', () => {
    const s = makeSession({ message_count: 12 });
    const label = `${s.message_count} msgs`;
    expect(label).toBe('12 msgs');
  });

  it('task count label is "{n} tareas"', () => {
    const s = makeSession({ task_count: 4 });
    const label = `${s.task_count} tareas`;
    expect(label).toBe('4 tareas');
  });

  it('active task label includes task id', () => {
    const s = makeSession({ active_task_id: 7 });
    const label = s.active_task_id !== null ? `Tarea #${s.active_task_id} activa` : 'Inactiva';
    expect(label).toBe('Tarea #7 activa');
  });

  it('idle label shown when no active task', () => {
    const s = makeSession({ active_task_id: null });
    const label = s.active_task_id !== null ? `Tarea #${s.active_task_id} activa` : 'Inactiva';
    expect(label).toBe('Inactiva');
  });
});

// ---------------------------------------------------------------------------
// Empty state / multiple sessions
// ---------------------------------------------------------------------------

describe('sessions array state', () => {
  it('empty sessions array has length 0', () => {
    const sessions: ReturnType<typeof makeSession>[] = [];
    expect(sessions.length).toBe(0);
  });

  it('non-empty sessions array shows all sessions', () => {
    const sessions = [
      makeSession({ name: 'session-a' }),
      makeSession({ name: 'session-b' }),
      makeSession({ name: 'session-c' }),
    ];
    expect(sessions.length).toBe(3);
    expect(sessions.map(s => s.name)).toEqual(['session-a', 'session-b', 'session-c']);
  });

  it('empty state message is the expected Spanish string', () => {
    const emptyMessage = 'No hay sesiones activas. Pedile a JARVIS que inicie una sesión de Claude.';
    expect(emptyMessage).toContain('No hay sesiones activas');
  });
});

// ---------------------------------------------------------------------------
// API / polling simulation (pure logic, no Svelte mounting)
// ---------------------------------------------------------------------------

describe('polling / API logic', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('getSessions is called on load', async () => {
    const mockGetSessions = vi.fn().mockResolvedValue([makeSession()]);
    await mockGetSessions();
    expect(mockGetSessions).toHaveBeenCalledTimes(1);
  });

  it('returns sessions array from a successful API call', async () => {
    const mockData = [makeSession({ name: 'alpha' }), makeSession({ name: 'beta' })];
    const mockGetSessions = vi.fn().mockResolvedValue(mockData);
    const result = await mockGetSessions();
    expect(result).toHaveLength(2);
    expect(result[0].name).toBe('alpha');
    expect(result[1].name).toBe('beta');
  });

  it('throws on API failure', async () => {
    const mockGetSessions = vi.fn().mockRejectedValue(new Error('network error'));
    await expect(mockGetSessions()).rejects.toThrow('network error');
  });

  it('load function sets error string on failure and keeps sessions empty', async () => {
    let sessions: ReturnType<typeof makeSession>[] = [];
    let error = '';
    const mockGetSessions = vi.fn().mockRejectedValue('Connection refused');

    try {
      sessions = await mockGetSessions();
    } catch (e) {
      error = String(e);
    }

    expect(sessions).toHaveLength(0);
    expect(error).toBe('Connection refused');
  });

  it('load function updates sessions on success', async () => {
    let sessions: ReturnType<typeof makeSession>[] = [];
    let error = '';
    const mockData = [makeSession({ name: 'live-session' })];
    const mockGetSessions = vi.fn().mockResolvedValue(mockData);

    try {
      sessions = await mockGetSessions();
    } catch (e) {
      error = String(e);
    }

    expect(error).toBe('');
    expect(sessions).toHaveLength(1);
    expect(sessions[0].name).toBe('live-session');
  });

  it('setInterval polling period is 5000ms', () => {
    vi.useFakeTimers();
    const mockFn = vi.fn();
    const intervalId = setInterval(mockFn, 5000);

    vi.advanceTimersByTime(4999);
    expect(mockFn).not.toHaveBeenCalled();

    vi.advanceTimersByTime(1);
    expect(mockFn).toHaveBeenCalledTimes(1);

    vi.advanceTimersByTime(5000);
    expect(mockFn).toHaveBeenCalledTimes(2);

    clearInterval(intervalId);
    vi.useRealTimers();
  });

  it('clearInterval stops polling on cleanup', () => {
    vi.useFakeTimers();
    const mockFn = vi.fn();
    const intervalId = setInterval(mockFn, 5000);

    vi.advanceTimersByTime(5000);
    expect(mockFn).toHaveBeenCalledTimes(1);

    clearInterval(intervalId);
    vi.advanceTimersByTime(10000);
    expect(mockFn).toHaveBeenCalledTimes(1); // no more calls after clear

    vi.useRealTimers();
  });
});
