// JARVIS - Notifications store
import { writable, derived, get } from 'svelte/store';
import type { Toast, ToastType } from '../types';
import { appVisible } from './visibility';

// ── Notification Preferences ────────────────────────────

export interface NotificationPrefs {
  taskComplete: boolean;
  taskError: boolean;
  planningDone: boolean;
  conflictAlert: boolean;
  soundEnabled: boolean;
}

const PREFS_KEY = 'jarvis-notif-prefs';
const VOLUME_KEY = 'jarvis-sound-volume';

const defaultPrefs: NotificationPrefs = {
  taskComplete: true,
  taskError: true,
  planningDone: true,
  conflictAlert: true,
  soundEnabled: true,
};

export function getNotifPrefs(): NotificationPrefs {
  try {
    const raw = localStorage.getItem(PREFS_KEY);
    if (raw) return { ...defaultPrefs, ...JSON.parse(raw) };
  } catch { /* ignore */ }
  return { ...defaultPrefs };
}

export function saveNotifPrefs(prefs: NotificationPrefs): void {
  localStorage.setItem(PREFS_KEY, JSON.stringify(prefs));
}

// ── Sound Volume ─────────────────────────────────────────

function loadSoundVolume(): number {
  try {
    const raw = localStorage.getItem(VOLUME_KEY);
    if (raw !== null) {
      const val = parseFloat(raw);
      if (!isNaN(val) && val >= 0 && val <= 1) return val;
    }
  } catch { /* ignore */ }
  return 0.7;
}

export const soundVolume = writable<number>(loadSoundVolume());

// Intentional: persists for the full app lifetime. No cleanup needed in a Tauri app.
soundVolume.subscribe((val) => {
  try { localStorage.setItem(VOLUME_KEY, String(val)); } catch {}
});

// ── Notification History ──────────────────────────────────

export interface ToastHistoryEntry {
  id: number;
  message: string;
  type: ToastType;
  timestamp: number;
}

const MAX_HISTORY = 100;
const HISTORY_STORAGE_KEY = 'jarvis-notification-history';
const UNREAD_STORAGE_KEY = 'jarvis-notification-unread-baseline';

function loadNotifHistory(): ToastHistoryEntry[] {
  try {
    const raw = localStorage.getItem(HISTORY_STORAGE_KEY);
    if (raw) return JSON.parse(raw) as ToastHistoryEntry[];
  } catch { /* ignore */ }
  return [];
}

function saveNotifHistory(entries: ToastHistoryEntry[]): void {
  try { localStorage.setItem(HISTORY_STORAGE_KEY, JSON.stringify(entries)); } catch {}
}

function loadUnreadBaseline(): number {
  try {
    const raw = localStorage.getItem(UNREAD_STORAGE_KEY);
    if (raw !== null) return parseInt(raw, 10) || 0;
  } catch { /* ignore */ }
  return 0;
}

function saveUnreadBaseline(n: number): void {
  try { localStorage.setItem(UNREAD_STORAGE_KEY, String(n)); } catch {}
}

/** Full persistent notification history (last 100) */
export const notificationHistory = writable<ToastHistoryEntry[]>(loadNotifHistory());

// Intentional: persists for full app lifetime.
notificationHistory.subscribe((entries) => saveNotifHistory(entries));

/** Alias kept for any existing consumers */
export const toastHistory = notificationHistory;

/** Total number of notifications ever recorded — used to compute unread count */
const _unreadBaseline = writable<number>(loadUnreadBaseline());
_unreadBaseline.subscribe(saveUnreadBaseline);

/** Count of notifications added since last markAllRead() call */
export const unreadCount = derived(
  [notificationHistory, _unreadBaseline],
  ([$history, $baseline]) => Math.max(0, $history.length - $baseline)
);

export function markAllRead(): void {
  _unreadBaseline.set(get(notificationHistory).length);
}

export function clearNotificationHistory(): void {
  notificationHistory.set([]);
  _unreadBaseline.set(0);
}

/** @deprecated use clearNotificationHistory */
export function clearToastHistory(): void {
  clearNotificationHistory();
}

// ── Toasts ──────────────────────────────────────────────

let nextId = 0;
export const toasts = writable<Toast[]>([]);

const MAX_TOASTS = 5;

function pushToHistory(id: number, message: string, type: ToastType) {
  notificationHistory.update((hist) => {
    const entry: ToastHistoryEntry = { id, message, type, timestamp: Date.now() };
    const updated = [...hist, entry];
    return updated.length > MAX_HISTORY ? updated.slice(-MAX_HISTORY) : updated;
  });
}

export function addToast(message: string, type: ToastType = 'info') {
  const id = nextId++;
  toasts.update((list) => {
    const updated = [...list, { id, message, type }];
    return updated.length > MAX_TOASTS ? updated.slice(-MAX_TOASTS) : updated;
  });
  pushToHistory(id, message, type);
  setTimeout(() => removeToast(id), 4000);
  const prefs = getNotifPrefs();
  if (prefs.soundEnabled) playBeep();
}

export function removeToast(id: number) {
  toasts.update((list) => list.filter((t) => t.id !== id));
}

// ── Smart Notification ──────────────────────────────────

/**
 * Sends a notification respecting user preferences and app visibility.
 * Always shows an in-app toast. Only sends a native OS notification
 * when the app window is not visible/focused.
 */
export function sendSmartNotification(
  type: keyof Omit<NotificationPrefs, 'soundEnabled'>,
  title: string,
  body: string,
  toastType: ToastType = 'info'
): void {
  const prefs = getNotifPrefs();

  // Check if this notification type is enabled
  if (!prefs[type]) return;

  // Always show in-app toast
  const id = nextId++;
  const message = `${title}: ${body}`;
  toasts.update((list) => {
    const updated = [...list, { id, message, type: toastType }];
    return updated.length > MAX_TOASTS ? updated.slice(-MAX_TOASTS) : updated;
  });
  pushToHistory(id, message, toastType);
  setTimeout(() => removeToast(id), 4000);

  // Play sound if enabled
  if (prefs.soundEnabled) {
    playBeep();
  }

  // Only send native notification if app is NOT visible
  if (!get(appVisible)) {
    sendNativeNotification(body, title);
  }
}

// ── Internal helpers ────────────────────────────────────

// Reuse a single AudioContext to avoid hitting browser limits (max ~6 instances)
let _audioCtx: AudioContext | null = null;
function getAudioCtx(): AudioContext {
  if (!_audioCtx || _audioCtx.state === 'closed') {
    const AC = window.AudioContext || (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext;
    _audioCtx = new AC();
  }
  if (_audioCtx.state === 'suspended') {
    _audioCtx.resume().catch(() => {});
  }
  return _audioCtx;
}

function playBeep() {
  try {
    const volume = get(soundVolume);
    const ctx = getAudioCtx();
    const osc = ctx.createOscillator();
    const gain = ctx.createGain();
    osc.connect(gain);
    gain.connect(ctx.destination);
    osc.frequency.value = 880;
    const peak = 0.1 * volume;
    gain.gain.value = peak;
    osc.start();
    gain.gain.exponentialRampToValueAtTime(0.001, ctx.currentTime + 0.3);
    osc.stop(ctx.currentTime + 0.3);
  } catch (e) {
    console.warn('notifications: audio playback failed', e);
  }
}

async function sendNativeNotification(body: string, title: string = 'JARVIS') {
  try {
    const { sendNotification, isPermissionGranted, requestPermission } = await import('@tauri-apps/plugin-notification');
    let permitted = await isPermissionGranted();
    if (!permitted) {
      const perm = await requestPermission();
      permitted = perm === 'granted';
    }
    if (permitted) {
      sendNotification({ title, body });
    }
  } catch (e) {
    console.warn('notifications: native notification failed', e);
  }
}
