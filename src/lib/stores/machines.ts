// JARVIS - Machines store
import { writable, get } from 'svelte/store';
import type { MachineInfo } from '../types';
import { fetchMachines } from '../api';

export const machines = writable<Record<string, MachineInfo>>({});

/** Set of machine IDs that are currently confirmed offline by the monitor */
export const offlineMachines = writable<Set<string>>(new Set());

// ── Machine status history ────────────────────────────────

export interface StatusChange {
  machineId: string;
  status: 'online' | 'offline';
  timestamp: number;
}

const MAX_STATUS_HISTORY = 20;

/** Rolling log of the last 20 online/offline transitions per machine */
export const machineStatusHistory = writable<StatusChange[]>([]);

function recordStatusChange(machineId: string, status: 'online' | 'offline'): void {
  machineStatusHistory.update((hist) => {
    const entry: StatusChange = { machineId, status, timestamp: Date.now() };
    const updated = [...hist, entry];
    return updated.length > MAX_STATUS_HISTORY ? updated.slice(-MAX_STATUS_HISTORY) : updated;
  });
}

export function markMachineOffline(id: string) {
  offlineMachines.update(s => { s.add(id); return s; });
  recordStatusChange(id, 'offline');
}

export function markMachineOnline(id: string) {
  offlineMachines.update(s => { s.delete(id); return s; });
  recordStatusChange(id, 'online');
}

/** Shallow compare two machine records to detect actual changes */
function machinesChanged(current: Record<string, MachineInfo>, next: Record<string, MachineInfo>): boolean {
  const currentKeys = Object.keys(current);
  const nextKeys = Object.keys(next);
  if (currentKeys.length !== nextKeys.length) return true;
  for (const key of nextKeys) {
    if (!current[key]) return true;
    const c = current[key];
    const n = next[key];
    if (c.name !== n.name || c.host !== n.host || c.os !== n.os ||
        c.enabled !== n.enabled ||
        c.role !== n.role || c.gpu !== n.gpu) {
      return true;
    }
  }
  return false;
}

export async function refreshMachinesStore() {
  try {
    const data = await fetchMachines();
    const current = get(machines);
    if (machinesChanged(current, data)) {
      machines.set(data);
    }
  } catch (e) {
    console.warn('Failed to fetch machines:', e);
  }
}
