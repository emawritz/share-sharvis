// JARVIS - Session store
import { writable, get } from 'svelte/store';
import type { SessionData, Activity, AgentInfo, Config } from '../types';
import { fetchSessionData, fetchAtlasActivity, fetchPixelActivity, onSessionUpdate, onActivityUpdate, onCommitsUpdate } from '../api';

const defaultSession: SessionData = {
  sessionId: '',
  rama: '',
  objetivo: '',
  active: false,
  atlasRunning: false,
  pixelRunning: false,
  totalRounds: '0',
  rounds: [],
  roundSummaries: [],
  commitsBack: [],
  commitsFront: []
};

export const session = writable<SessionData>(defaultSession);
export const atlasFeed = writable<Activity[]>([]);
export const pixelFeed = writable<Activity[]>([]);
export const config = writable<Config>({ sessionId: '', rama: '', objetivo: '' });
export const atlasAgentInfo = writable<AgentInfo>({ agentCount: 0, skills: [] });
export const pixelAgentInfo = writable<AgentInfo>({ agentCount: 0, skills: [] });
export const lastHeartbeat = writable<number>(Date.now());

// Fingerprints of cleared activities — persisted in localStorage so they survive reloads
const FP_KEY = 'jarvis-cleared-fps-';
const MAX_STORED_FPS = 2000;

function loadPersistedFps(which: 'atlas' | 'pixel'): Set<string> {
  try {
    const raw = localStorage.getItem(FP_KEY + which);
    if (raw) return new Set(JSON.parse(raw) as string[]);
  } catch {}
  return new Set();
}

function persistFps(which: 'atlas' | 'pixel', fps: Set<string>) {
  try {
    const arr = Array.from(fps);
    const trimmed = arr.length > MAX_STORED_FPS ? arr.slice(-MAX_STORED_FPS) : arr;
    localStorage.setItem(FP_KEY + which, JSON.stringify(trimmed));
  } catch {}
}

// Lazy-loaded so localStorage is available (browser only, not SSR)
let _fpAtlas: Set<string> | null = null;
let _fpPixel: Set<string> | null = null;
function getClearedFps(which: 'atlas' | 'pixel'): Set<string> {
  if (which === 'atlas') { _fpAtlas ??= loadPersistedFps('atlas'); return _fpAtlas; }
  _fpPixel ??= loadPersistedFps('pixel'); return _fpPixel;
}

export function clearFeed(which: 'atlas' | 'pixel') {
  const store = which === 'atlas' ? atlasFeed : pixelFeed;
  const fps = getClearedFps(which);
  const current = get(store);
  current.forEach(a => fps.add(activityKey(a)));
  persistFps(which, fps);
  store.set([]);
}

/** Only update session store if key fields actually changed */
function updateSessionIfChanged(newData: SessionData) {
  const current = get(session);
  if (current &&
      current.sessionId === newData.sessionId &&
      current.rama === newData.rama &&
      current.objetivo === newData.objetivo &&
      current.active === newData.active &&
      current.atlasRunning === newData.atlasRunning &&
      current.pixelRunning === newData.pixelRunning &&
      current.totalRounds === newData.totalRounds &&
      current.rounds?.length === newData.rounds?.length &&
      current.roundSummaries?.length === newData.roundSummaries?.length &&
      current.commitsBack?.length === newData.commitsBack?.length &&
      current.commitsFront?.length === newData.commitsFront?.length) {
    return; // skip update — no meaningful change
  }
  session.set(newData);
}

/** Only update config store if values actually changed */
function updateConfigIfChanged(newConfig: Config) {
  const current = get(config);
  if (current &&
      current.sessionId === newConfig.sessionId &&
      current.rama === newConfig.rama &&
      current.objetivo === newConfig.objetivo) {
    return;
  }
  config.set(newConfig);
}

/** Only update agent info if values actually changed */
function updateAgentInfoIfChanged(store: typeof atlasAgentInfo, newInfo: AgentInfo) {
  const current = get(store);
  if (current &&
      current.agentCount === newInfo.agentCount &&
      current.skills?.length === newInfo.skills?.length) {
    return;
  }
  store.set(newInfo);
}

let initialized = false;
let pollInterval: ReturnType<typeof setInterval> | null = null;
let eventsActive = false;
let unlistenFns: (() => void)[] = [];

const MAX_FEED_ITEMS = 500;

/** Fingerprint an activity for dedup */
export function activityKey(a: Activity): string {
  if (a.type === 'tool') return `t:${a.name}:${a.detail || ''}`;
  if (a.type === 'prompt') return `p:${(a.content || '').substring(0, 100)}`;
  return `x:${(a.content || '').substring(0, 100)}`;
}

/**
 * Merge new activities into existing feed, appending only truly new items.
 *
 * Dedup strategy: Build a Set of fingerprints from existing activities for O(1)
 * lookup, then append only incoming items whose fingerprint isn't already present.
 * This avoids the previous O(n*m) nested comparison when finding overlap.
 */
export function mergeActivities(existing: Activity[], incoming: Activity[]): Activity[] {
  if (existing.length === 0) return incoming.slice(-MAX_FEED_ITEMS);
  if (incoming.length === 0) return existing;

  // Set of fingerprints from existing activities for O(1) dedup lookups
  const existingFingerprints = new Set(existing.map(activityKey));

  const newItems = incoming.filter(a => !existingFingerprints.has(activityKey(a)));
  if (newItems.length === 0) return existing;

  const merged = [...existing, ...newItems];
  // Cap to prevent unbounded growth
  return merged.length > MAX_FEED_ITEMS ? merged.slice(-MAX_FEED_ITEMS) : merged;
}

function updateFeed(store: typeof atlasFeed, incoming: Activity[], which: 'atlas' | 'pixel') {
  const suppressed = getClearedFps(which);
  // Filter out anything the user cleared
  const fresh = suppressed.size > 0
    ? incoming.filter(a => !suppressed.has(activityKey(a)))
    : incoming;
  store.update(existing => {
    const merged = mergeActivities(existing, fresh);
    if (merged.length === existing.length) return existing;
    return merged;
  });
}

export async function initSessionStore() {
  if (initialized) return;
  initialized = true;

  // Initial fetch
  try {
    const data = await fetchSessionData();
    updateSessionIfChanged(data);
    updateConfigIfChanged({
      sessionId: data.sessionId,
      rama: data.rama,
      objetivo: data.objetivo
    });
  } catch (e) {
    console.warn('Initial session fetch failed:', e);
  }

  try {
    const [atlas, pixel] = await Promise.all([
      fetchAtlasActivity(),
      fetchPixelActivity()
    ]);
    updateFeed(atlasFeed, atlas, 'atlas');
    updateFeed(pixelFeed, pixel, 'pixel');
  } catch (e) {
    console.warn('Initial activity fetch failed:', e);
  }

  // Listen for Tauri events
  let hasEvents = false;
  try {
    // Await all three before pushing — avoids partial registration if destroy fires mid-init
    const ul1 = await onSessionUpdate((data) => {
      updateSessionIfChanged(data);
      updateConfigIfChanged({
        sessionId: data.sessionId,
        rama: data.rama,
        objetivo: data.objetivo
      });
      lastHeartbeat.set(Date.now());
    });
    const ul2 = await onActivityUpdate((data) => {
      if (data.atlas) updateFeed(atlasFeed, data.atlas, 'atlas');
      if (data.pixel) updateFeed(pixelFeed, data.pixel, 'pixel');
      if (data.atlasAgentInfo) updateAgentInfoIfChanged(atlasAgentInfo, data.atlasAgentInfo);
      if (data.pixelAgentInfo) updateAgentInfoIfChanged(pixelAgentInfo, data.pixelAgentInfo);
    });
    const ul3 = await onCommitsUpdate((data) => {
      session.update((s) => ({
        ...s,
        commitsBack: data.back || s.commitsBack,
        commitsFront: data.front || s.commitsFront
      }));
    });
    unlistenFns.push(ul1, ul2, ul3);

    hasEvents = true;
    eventsActive = true;
  } catch (e) {
    console.warn('Tauri events not available, falling back to polling:', e);
  }

  if (!hasEvents) {
    // Fallback: poll every 3s
    pollInterval = setInterval(async () => {
      if (eventsActive) {
        // Events took over, stop polling
        if (pollInterval) { clearInterval(pollInterval); pollInterval = null; }
        return;
      }
      try {
        const data = await fetchSessionData();
        updateSessionIfChanged(data);
        updateConfigIfChanged({
          sessionId: data.sessionId,
          rama: data.rama,
          objetivo: data.objetivo
        });
        lastHeartbeat.set(Date.now());

        const [atlas, pixel] = await Promise.all([
          fetchAtlasActivity(),
          fetchPixelActivity()
        ]);
        if (!initialized) return; // destroyed while awaiting
        updateFeed(atlasFeed, atlas, 'atlas');
        updateFeed(pixelFeed, pixel, 'pixel');
      } catch (e) {
        console.error('session:', e);
      }
    }, 3000);
  }
}

export function destroySessionStore() {
  if (pollInterval) {
    clearInterval(pollInterval);
    pollInterval = null;
  }
  unlistenFns.forEach(fn => fn());
  unlistenFns = [];
  eventsActive = false;
  initialized = false;
}
