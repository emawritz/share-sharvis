// JARVIS - Vitest global setup
import { vi } from 'vitest';

// Mock Tauri APIs — tests run in jsdom, not inside a Tauri webview
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue(null),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
  emit: vi.fn().mockResolvedValue(undefined),
}));

vi.mock('@tauri-apps/plugin-notification', () => ({
  isPermissionGranted: vi.fn().mockResolvedValue(false),
  requestPermission: vi.fn().mockResolvedValue('denied'),
  sendNotification: vi.fn(),
}));

// Minimal AudioContext stub so notification store doesn't throw
class MockAudioContext {
  state = 'running';
  currentTime = 0;
  destination = {};
  createOscillator() {
    return {
      connect: vi.fn(),
      start: vi.fn(),
      stop: vi.fn(),
      frequency: { value: 0 },
    };
  }
  createGain() {
    return {
      connect: vi.fn(),
      gain: {
        value: 0,
        exponentialRampToValueAtTime: vi.fn(),
      },
    };
  }
  resume() { return Promise.resolve(); }
}
(globalThis as unknown as Record<string, unknown>).AudioContext = MockAudioContext;
