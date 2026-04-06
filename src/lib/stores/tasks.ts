// JARVIS - Tasks store
import { writable, derived } from 'svelte/store';
import type { Task } from '../types';
import { fetchTasks, onTaskStarted, onTaskDone } from '../api';
import { addToast, sendSmartNotification } from './notifications';

export const tasks = writable<Task[]>([]);
export const runningCount = derived(tasks, ($tasks) =>
  $tasks.filter((t) => t.status === 'running').length
);

const knownDoneTasks = new Set<number>();
let initialized = false;
let pollInterval: ReturnType<typeof setInterval> | null = null;
let fetchGeneration = 0;
let listenersAttached = false;
let unlistenTasks: (() => void)[] = [];
let fetchDebounceTimer: ReturnType<typeof setTimeout> | null = null;

/** Debounced fetch to coalesce rapid event-driven refreshes */
function debouncedFetch(onList?: (list: Task[]) => void) {
  if (fetchDebounceTimer) clearTimeout(fetchDebounceTimer);
  const gen = ++fetchGeneration;
  fetchDebounceTimer = setTimeout(() => {
    fetchTasks().then((list) => {
      if (gen !== fetchGeneration) return;
      tasks.set(list);
      pruneKnownDoneTasks(list);
      onList?.(list);
    }).catch((e) => console.error('tasks: debounced fetch failed', e));
  }, 300);
}

/** Remove IDs from knownDoneTasks that no longer exist in the current task list */
function pruneKnownDoneTasks(currentList: Task[]) {
  const currentIds = new Set(currentList.map((t) => t.id));
  for (const id of knownDoneTasks) {
    if (!currentIds.has(id)) knownDoneTasks.delete(id);
  }
}

export async function initTasksStore() {
  if (initialized) return;
  initialized = true;

  // Initial fetch
  try {
    const list = await fetchTasks();
    tasks.set(list);
    pruneKnownDoneTasks(list);
    for (const t of list) {
      if (t.status === 'done') knownDoneTasks.add(t.id);
    }
  } catch (e) {
    console.error('tasks: initial fetch failed', e);
  }

  // Listen for events (only attach once)
  let hasEvents = false;
  if (!listenersAttached) {
    try {
      const ul1 = await onTaskStarted((_data) => {
        // Refresh full task list when a new task starts (debounced)
        debouncedFetch();
      });
      const ul2 = await onTaskDone((data) => {
        // Refresh full task list when a task completes (debounced)
        debouncedFetch((list) => {
          if (!knownDoneTasks.has(data.id)) {
            knownDoneTasks.add(data.id);
            const task = list.find((t) => t.id === data.id);
            const prompt = task?.prompt || '';
            const hasError = task?.status === 'error' || task?.status === 'timeout';
            if (hasError) {
              sendSmartNotification(
                'taskError',
                'Tarea fallida',
                `#${data.id} ${data.target.toUpperCase()}: ${prompt.substring(0, 100)}`,
                'error'
              );
            } else {
              sendSmartNotification(
                'taskComplete',
                'Tarea completada',
                `#${data.id} ${data.target.toUpperCase()}: ${prompt.substring(0, 100)}`,
                'success'
              );
            }
          }
        });
      });
      // Push both atomically after both awaits complete
      unlistenTasks.push(ul1, ul2);

      listenersAttached = true;
      hasEvents = true;
    } catch (e) {
      console.error('tasks: event listeners not available', e);
    }
  } else {
    hasEvents = true;
  }

  if (!hasEvents) {
    // Fallback poll
    pollInterval = setInterval(async () => {
      try {
        const gen = ++fetchGeneration;
        const list = await fetchTasks();
        if (gen !== fetchGeneration) return;
        for (const t of list) {
          if ((t.status === 'done' || t.status === 'error' || t.status === 'timeout') && !knownDoneTasks.has(t.id)) {
            knownDoneTasks.add(t.id);
            if (knownDoneTasks.size > 1) {
              const hasError = t.status === 'error' || t.status === 'timeout';
              if (hasError) {
                sendSmartNotification(
                  'taskError',
                  'Tarea fallida',
                  `#${t.id} ${t.target.toUpperCase()}: ${t.prompt.substring(0, 100)}`,
                  'error'
                );
              } else {
                sendSmartNotification(
                  'taskComplete',
                  'Tarea completada',
                  `#${t.id} ${t.target.toUpperCase()}: ${t.prompt.substring(0, 100)}`,
                  'success'
                );
              }
            }
          }
          if (t.status !== 'done') knownDoneTasks.delete(t.id);
        }
        pruneKnownDoneTasks(list);
        tasks.set(list);
      } catch (e) {
        console.error('tasks: poll failed', e);
      }
    }, 3000);
  }
}

export function destroyTasksStore() {
  if (pollInterval) {
    clearInterval(pollInterval);
    pollInterval = null;
  }
  if (fetchDebounceTimer) {
    clearTimeout(fetchDebounceTimer);
    fetchDebounceTimer = null;
  }
  unlistenTasks.forEach(fn => fn());
  unlistenTasks = [];
  listenersAttached = false;
  knownDoneTasks.clear();
  fetchGeneration = 0;
  initialized = false;
}
