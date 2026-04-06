import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { get } from 'svelte/store';

// The notifications store has module-level state (nextId counter, writable stores).
// We use vi.resetModules() + dynamic imports so each test group gets a fresh module.
// vi.mock() calls below are re-applied after each resetModules() because they are
// hoisted before the module graph is resolved.

vi.mock('@tauri-apps/plugin-notification', () => ({
  isPermissionGranted: vi.fn().mockResolvedValue(false),
  requestPermission: vi.fn().mockResolvedValue('denied'),
  sendNotification: vi.fn(),
}));

vi.mock('./visibility', () => ({
  appVisible: { subscribe: (fn: (v: boolean) => void) => { fn(true); return () => {}; } },
}));

describe('notifications store', () => {
  beforeEach(() => {
    localStorage.clear();
    vi.useFakeTimers();
    vi.resetModules();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  // ── addToast ────────────────────────────────────────────────────────

  describe('addToast', () => {
    it('adds a toast with the given message and type', async () => {
      const { toasts, addToast } = await import('./notifications');
      addToast('Hello world', 'success');
      const list = get(toasts);
      expect(list.length).toBeGreaterThanOrEqual(1);
      const entry = list.find((t) => t.message === 'Hello world');
      expect(entry).toBeDefined();
      expect(entry?.type).toBe('success');
    });

    it('defaults toast type to "info" when no type is provided', async () => {
      const { toasts, addToast } = await import('./notifications');
      addToast('Default type');
      const list = get(toasts);
      const entry = list.find((t) => t.message === 'Default type');
      expect(entry?.type).toBe('info');
    });

    it('also pushes the toast to toastHistory', async () => {
      const { toastHistory, addToast } = await import('./notifications');
      addToast('History check', 'warning');
      const hist = get(toastHistory);
      const entry = hist.find((h) => h.message === 'History check');
      expect(entry).toBeDefined();
      expect(entry?.type).toBe('warning');
    });

    it('auto-removes the toast after 4 000 ms', async () => {
      const { toasts, addToast } = await import('./notifications');

      addToast('Expiring toast', 'info');
      expect(get(toasts).some((t) => t.message === 'Expiring toast')).toBe(true);

      vi.advanceTimersByTime(4000);

      expect(get(toasts).some((t) => t.message === 'Expiring toast')).toBe(false);
    });

    it('caps the list at MAX_TOASTS (5)', async () => {
      const { toasts, addToast } = await import('./notifications');
      for (let i = 0; i < 7; i++) addToast(`msg-${i}`, 'info');
      expect(get(toasts).length).toBeLessThanOrEqual(5);
    });
  });

  // ── clearToastHistory ───────────────────────────────────────────────

  describe('clearToastHistory', () => {
    it('empties the toastHistory store', async () => {
      const { toastHistory, addToast, clearToastHistory } = await import('./notifications');
      addToast('to be cleared', 'error');
      expect(get(toastHistory).length).toBeGreaterThan(0);
      clearToastHistory();
      expect(get(toastHistory)).toHaveLength(0);
    });
  });

  // ── soundVolume ─────────────────────────────────────────────────────

  describe('soundVolume', () => {
    it('defaults to 0.7 when localStorage has no value', async () => {
      localStorage.removeItem('jarvis-sound-volume');
      const { soundVolume } = await import('./notifications');
      expect(get(soundVolume)).toBe(0.7);
    });

    it('loads a previously saved volume from localStorage', async () => {
      localStorage.setItem('jarvis-sound-volume', '0.4');
      const { soundVolume } = await import('./notifications');
      expect(get(soundVolume)).toBe(0.4);
    });

    it('persists new volume to localStorage when the store changes', async () => {
      const { soundVolume } = await import('./notifications');
      soundVolume.set(0.2);
      expect(localStorage.getItem('jarvis-sound-volume')).toBe('0.2');
    });

    it('ignores out-of-range values in localStorage and falls back to 0.7', async () => {
      localStorage.setItem('jarvis-sound-volume', '2.5'); // > 1
      const { soundVolume } = await import('./notifications');
      expect(get(soundVolume)).toBe(0.7);
    });
  });
});
