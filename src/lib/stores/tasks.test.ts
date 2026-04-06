import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { get } from 'svelte/store';
import type { Task } from '../types';

// Mock the api module — must be before any imports of tasks.ts
vi.mock('../api', () => ({
  fetchTasks: vi.fn().mockResolvedValue([]),
  onTaskStarted: vi.fn().mockResolvedValue(() => {}),
  onTaskDone: vi.fn().mockResolvedValue(() => {}),
  fetchMachines: vi.fn().mockResolvedValue({}),
}));

// Stub notifications so audio / Tauri notification calls don't fire
vi.mock('./notifications', () => ({
  addToast: vi.fn(),
  sendSmartNotification: vi.fn(),
}));

// Import after mocks are registered
import { tasks, runningCount, initTasksStore, destroyTasksStore } from './tasks';
import { fetchTasks } from '../api';

const mockFetchTasks = fetchTasks as ReturnType<typeof vi.fn>;

describe('tasks store', () => {
  beforeEach(() => {
    // Reset to a clean state before each test
    destroyTasksStore();
    tasks.set([]);
    mockFetchTasks.mockResolvedValue([]);
    vi.clearAllMocks();
    // Reapply default mock after clearAllMocks
    mockFetchTasks.mockResolvedValue([]);
  });

  afterEach(() => {
    destroyTasksStore();
  });

  // ── Initial state ────────────────────────────────────────────────────

  it('initializes with an empty task list', () => {
    expect(get(tasks)).toEqual([]);
  });

  it('runningCount is 0 when there are no tasks', () => {
    expect(get(runningCount)).toBe(0);
  });

  // ── runningCount derived store ────────────────────────────────────────

  it('runningCount correctly counts tasks with status "running"', () => {
    const sampleTasks: Task[] = [
      { id: 1, target: 'atlas', prompt: 'A', status: 'running', orchestrate: false, output: '' },
      { id: 2, target: 'pixel', prompt: 'B', status: 'done', orchestrate: false, output: '' },
      { id: 3, target: 'atlas', prompt: 'C', status: 'running', orchestrate: false, output: '' },
    ];
    tasks.set(sampleTasks);
    expect(get(runningCount)).toBe(2);
  });

  it('runningCount is 0 when all tasks are done', () => {
    tasks.set([
      { id: 1, target: 'atlas', prompt: 'A', status: 'done', orchestrate: false, output: '' },
      { id: 2, target: 'pixel', prompt: 'B', status: 'error', orchestrate: false, output: '' },
    ]);
    expect(get(runningCount)).toBe(0);
  });

  // ── initTasksStore ────────────────────────────────────────────────────

  it('initTasksStore fetches and sets the task list', async () => {
    const list: Task[] = [
      { id: 10, target: 'atlas', prompt: 'hello', status: 'running', orchestrate: false, output: '' },
    ];
    mockFetchTasks.mockResolvedValueOnce(list);

    await initTasksStore();

    expect(get(tasks)).toEqual(list);
  });

  it('initTasksStore is idempotent — second call does not re-fetch', async () => {
    const list: Task[] = [
      { id: 1, target: 'atlas', prompt: 'once', status: 'done', orchestrate: false, output: '' },
    ];
    mockFetchTasks.mockResolvedValue(list);

    await initTasksStore();
    await initTasksStore(); // second call — should be a no-op

    // fetchTasks should only have been called once
    expect(mockFetchTasks).toHaveBeenCalledTimes(1);
  });

  // ── pruneKnownDoneTasks (via destroyTasksStore + reinit) ──────────────

  it('after destroyTasksStore, initTasksStore reflects a pruned task list', async () => {
    // First init: two done tasks
    const initial: Task[] = [
      { id: 1, target: 'atlas', prompt: 'A', status: 'done', orchestrate: false, output: '' },
      { id: 2, target: 'pixel', prompt: 'B', status: 'done', orchestrate: false, output: '' },
    ];
    mockFetchTasks.mockResolvedValueOnce(initial);
    await initTasksStore();
    expect(get(tasks)).toHaveLength(2);

    // Destroy resets internal state
    destroyTasksStore();

    // Second init: task 2 is gone from the server
    const pruned: Task[] = [
      { id: 1, target: 'atlas', prompt: 'A', status: 'done', orchestrate: false, output: '' },
    ];
    mockFetchTasks.mockResolvedValueOnce(pruned);
    await initTasksStore();

    expect(get(tasks)).toHaveLength(1);
    expect(get(tasks)[0].id).toBe(1);
  });

  // ── destroyTasksStore ─────────────────────────────────────────────────

  it('destroyTasksStore allows re-initialization after being called', async () => {
    mockFetchTasks.mockResolvedValueOnce([]);
    await initTasksStore();

    destroyTasksStore();

    // Can initialize again without throwing
    const secondList: Task[] = [
      { id: 99, target: 'atlas', prompt: 'fresh', status: 'running', orchestrate: false, output: '' },
    ];
    mockFetchTasks.mockResolvedValueOnce(secondList);
    await initTasksStore();

    expect(get(tasks)).toEqual(secondList);
  });
});
