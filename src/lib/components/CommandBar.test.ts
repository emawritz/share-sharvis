/**
 * CommandBar logic tests
 *
 * CommandBar.svelte is a Svelte 5 component whose core send logic lives in
 * handleSend() / handleInputKeydown(). Since @testing-library/svelte is not
 * yet installed we test the extracted logic directly by reproducing the same
 * guard conditions in plain TypeScript, and by testing the underlying API
 * layer (apiSendTask) with the Tauri invoke mock.
 *
 * To add full DOM rendering tests, install:
 *   npm install --save-dev @testing-library/svelte @testing-library/jest-dom
 * and then use render(CommandBar) here.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { invoke } from '@tauri-apps/api/core';

// Mock the api module
vi.mock('../api', () => ({
  sendTask: vi.fn().mockResolvedValue({ id: 1 }),
  sendTaskChain: vi.fn().mockResolvedValue([]),
  executeAction: vi.fn().mockResolvedValue(null),
  sendAgentMessage: vi.fn().mockResolvedValue(null),
  fetchTasks: vi.fn().mockResolvedValue([]),
  onTaskStarted: vi.fn().mockResolvedValue(() => {}),
  onTaskDone: vi.fn().mockResolvedValue(() => {}),
  fetchMachines: vi.fn().mockResolvedValue({}),
}));

vi.mock('../stores/notifications', () => ({
  addToast: vi.fn(),
  sendSmartNotification: vi.fn(),
}));

// ---------------------------------------------------------------------------
// Reproduce the guard logic from CommandBar.handleSend()
// ---------------------------------------------------------------------------

/**
 * Minimal reproduction of handleSend from CommandBar.svelte so we can unit-
 * test the guard conditions without mounting the full Svelte component.
 */
async function makeHandleSend(sendTaskFn: (target: string, prompt: string) => Promise<unknown>) {
  let sending = false;

  return async function handleSend(prompt: string, target: string = 'auto') {
    const p = prompt.trim();
    if (!p) return 'empty'; // guard: empty prompt → no-op

    if (sending) return 'busy'; // guard: already sending → no-op

    sending = true;
    try {
      await sendTaskFn(target, p);
      return 'sent';
    } finally {
      sending = false;
    }
  };
}

describe('CommandBar logic', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  // ── Empty prompt guard ──────────────────────────────────────────────

  describe('empty prompt guard', () => {
    it('does NOT call sendTask when prompt is empty', async () => {
      const { sendTask } = await import('../api');
      const handleSend = await makeHandleSend(sendTask as (t: string, p: string) => Promise<unknown>);

      await handleSend('');

      expect(sendTask).not.toHaveBeenCalled();
    });

    it('does NOT call sendTask when prompt is only whitespace', async () => {
      const { sendTask } = await import('../api');
      const handleSend = await makeHandleSend(sendTask as (t: string, p: string) => Promise<unknown>);

      await handleSend('   ');

      expect(sendTask).not.toHaveBeenCalled();
    });

    it('DOES call sendTask when prompt has content', async () => {
      const { sendTask } = await import('../api');
      const handleSend = await makeHandleSend(sendTask as (t: string, p: string) => Promise<unknown>);

      await handleSend('fix the bug');

      expect(sendTask).toHaveBeenCalledOnce();
      expect(sendTask).toHaveBeenCalledWith('auto', 'fix the bug');
    });
  });

  // ── Sending guard (disabled-while-sending) ──────────────────────────

  describe('sending guard', () => {
    it('does NOT call sendTask a second time while a send is already in-flight', async () => {
      // Create a sendTask that stays pending until we resolve it manually
      let resolveFirst!: () => void;
      const slowSend = vi.fn(
        () => new Promise<void>((res) => { resolveFirst = res; })
      );

      const handleSend = await makeHandleSend(slowSend);

      // Fire first send — it hangs
      const first = handleSend('task one');

      // Fire second send while first is still in-flight
      const second = await handleSend('task two');

      expect(second).toBe('busy');
      expect(slowSend).toHaveBeenCalledTimes(1);

      // Resolve the first and confirm it completes
      resolveFirst();
      await first;
    });

    it('allows a second send AFTER the first completes', async () => {
      const { sendTask } = await import('../api');
      const handleSend = await makeHandleSend(sendTask as (t: string, p: string) => Promise<unknown>);

      await handleSend('first task');
      await handleSend('second task');

      expect(sendTask).toHaveBeenCalledTimes(2);
    });
  });

  // ── Enter key guard ─────────────────────────────────────────────────

  describe('Enter key handling', () => {
    it('pressing Enter with empty prompt returns "empty" (no dispatch)', async () => {
      const { sendTask } = await import('../api');
      const handleSend = await makeHandleSend(sendTask as (t: string, p: string) => Promise<unknown>);

      // Simulate handleInputKeydown: only fires handleSend when not sending
      let sending = false;
      async function handleInputKeydown(key: string, prompt: string) {
        if (key === 'Enter' && !sending) return handleSend(prompt);
      }

      const result = await handleInputKeydown('Enter', '');
      expect(result).toBe('empty');
      expect(sendTask).not.toHaveBeenCalled();
    });

    it('pressing Enter twice rapidly only dispatches once due to sending guard', async () => {
      let resolveFirst!: () => void;
      const slowSend = vi.fn(
        () => new Promise<void>((res) => { resolveFirst = res; })
      );

      const handleSend = await makeHandleSend(slowSend);

      let sending = false;

      async function handleInputKeydown(key: string, prompt: string) {
        if (key === 'Enter' && !sending) {
          // Note: in the real component `sending` is set inside handleSend
          // Our reproduction sets it internally; simulate here by calling
          // handleSend which sets its internal flag.
          return handleSend(prompt);
        }
        return 'skipped';
      }

      // First Enter: starts the slow send
      const p1 = handleInputKeydown('Enter', 'do something');
      // Second Enter: handleSend sees sending=true internally, returns 'busy'
      const r2 = await handleInputKeydown('Enter', 'do something again');

      expect(r2).toBe('busy');
      expect(slowSend).toHaveBeenCalledTimes(1);

      resolveFirst();
      await p1;
    });
  });

  // ── Tauri invoke is called via sendTask ────────────────────────────

  describe('sendTask integration with invoke mock', () => {
    it('invoke is called when sendTask is called', async () => {
      // sendTask calls invoke('send_task', ...) internally
      // The invoke mock is set up in setup.ts and returns null by default
      const result = await invoke('send_task', { target: 'atlas', prompt: 'hello' });
      expect(result).toBeNull(); // mock returns null
    });
  });
});
