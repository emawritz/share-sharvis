import { describe, it, expect, vi, beforeEach } from 'vitest';
import { get } from 'svelte/store';
import type { MachineInfo } from '../types';

// Mock the api module before any store imports
vi.mock('../api', () => ({
  fetchMachines: vi.fn().mockResolvedValue({}),
}));

import {
  machines,
  offlineMachines,
  markMachineOffline,
  markMachineOnline,
  refreshMachinesStore,
} from './machines';
import { fetchMachines } from '../api';

const mockFetchMachines = fetchMachines as ReturnType<typeof vi.fn>;

function makeMachine(overrides: Partial<MachineInfo> = {}): MachineInfo {
  return {
    id: 'atlas',
    name: 'ATLAS',
    host: 'local',
    os: 'macos',
    role: 'orchestrator',
    enabled: true,
    tags: [],
    ...overrides,
  };
}

describe('machines store', () => {
  beforeEach(() => {
    machines.set({});
    offlineMachines.set(new Set());
    mockFetchMachines.mockResolvedValue({});
    vi.clearAllMocks();
    mockFetchMachines.mockResolvedValue({});
  });

  // ── Initial state ────────────────────────────────────────────────────

  it('starts with an empty machines record', () => {
    expect(get(machines)).toEqual({});
  });

  it('starts with an empty offlineMachines set', () => {
    expect(get(offlineMachines).size).toBe(0);
  });

  // ── markMachineOffline / markMachineOnline ───────────────────────────

  it('markMachineOffline adds the machine id to offlineMachines', () => {
    markMachineOffline('atlas');
    expect(get(offlineMachines).has('atlas')).toBe(true);
  });

  it('markMachineOnline removes the machine id from offlineMachines', () => {
    markMachineOffline('atlas');
    markMachineOffline('pixel');
    markMachineOnline('atlas');
    const set = get(offlineMachines);
    expect(set.has('atlas')).toBe(false);
    expect(set.has('pixel')).toBe(true);
  });

  it('markMachineOnline on an id that was not offline is a no-op', () => {
    markMachineOnline('nonexistent');
    expect(get(offlineMachines).size).toBe(0);
  });

  it('multiple machines can be marked offline independently', () => {
    markMachineOffline('atlas');
    markMachineOffline('pixel');
    markMachineOffline('cloud');
    const set = get(offlineMachines);
    expect(set.size).toBe(3);
    expect(set.has('atlas')).toBe(true);
    expect(set.has('pixel')).toBe(true);
    expect(set.has('cloud')).toBe(true);
  });

  // ── refreshMachinesStore ─────────────────────────────────────────────

  it('refreshMachinesStore sets machines from the API response', async () => {
    const data: Record<string, MachineInfo> = {
      atlas: makeMachine({ id: 'atlas', name: 'ATLAS' }),
    };
    mockFetchMachines.mockResolvedValueOnce(data);

    await refreshMachinesStore();

    expect(get(machines)).toEqual(data);
  });

  it('refreshMachinesStore updates the store when machines change', async () => {
    const first: Record<string, MachineInfo> = {
      atlas: makeMachine({ id: 'atlas', name: 'ATLAS' }),
    };
    mockFetchMachines.mockResolvedValueOnce(first);
    await refreshMachinesStore();
    expect(get(machines)['atlas'].name).toBe('ATLAS');

    const second: Record<string, MachineInfo> = {
      atlas: makeMachine({ id: 'atlas', name: 'ATLAS-UPDATED' }),
    };
    mockFetchMachines.mockResolvedValueOnce(second);
    await refreshMachinesStore();

    expect(get(machines)['atlas'].name).toBe('ATLAS-UPDATED');
  });

  it('refreshMachinesStore skips store update when machines are identical', async () => {
    const data: Record<string, MachineInfo> = {
      atlas: makeMachine({ id: 'atlas' }),
    };
    mockFetchMachines.mockResolvedValue(data);

    await refreshMachinesStore();
    const refAfterFirst = get(machines);

    await refreshMachinesStore();
    const refAfterSecond = get(machines);

    // Same reference — no re-render triggered
    expect(refAfterFirst).toBe(refAfterSecond);
  });

  it('refreshMachinesStore handles a new machine being added', async () => {
    const first: Record<string, MachineInfo> = {
      atlas: makeMachine({ id: 'atlas' }),
    };
    mockFetchMachines.mockResolvedValueOnce(first);
    await refreshMachinesStore();
    expect(Object.keys(get(machines))).toHaveLength(1);

    const second: Record<string, MachineInfo> = {
      atlas: makeMachine({ id: 'atlas' }),
      pixel: makeMachine({ id: 'pixel', name: 'PIXEL', host: 'pixel', os: 'linux' }),
    };
    mockFetchMachines.mockResolvedValueOnce(second);
    await refreshMachinesStore();

    expect(Object.keys(get(machines))).toHaveLength(2);
    expect(get(machines)['pixel'].name).toBe('PIXEL');
  });

  it('refreshMachinesStore handles a machine being removed', async () => {
    const two: Record<string, MachineInfo> = {
      atlas: makeMachine({ id: 'atlas' }),
      pixel: makeMachine({ id: 'pixel', name: 'PIXEL', host: 'pixel', os: 'linux' }),
    };
    mockFetchMachines.mockResolvedValueOnce(two);
    await refreshMachinesStore();
    expect(Object.keys(get(machines))).toHaveLength(2);

    const one: Record<string, MachineInfo> = {
      atlas: makeMachine({ id: 'atlas' }),
    };
    mockFetchMachines.mockResolvedValueOnce(one);
    await refreshMachinesStore();

    expect(Object.keys(get(machines))).toHaveLength(1);
    expect(get(machines)['pixel']).toBeUndefined();
  });

  it('refreshMachinesStore does not throw when the API call fails', async () => {
    mockFetchMachines.mockRejectedValueOnce(new Error('network error'));

    // Should not throw; machines store stays at its previous value
    await expect(refreshMachinesStore()).resolves.toBeUndefined();
    expect(get(machines)).toEqual({});
  });

  it('refreshMachinesStore detects a change in machine enabled field', async () => {
    const enabled: Record<string, MachineInfo> = {
      atlas: makeMachine({ id: 'atlas', enabled: true }),
    };
    mockFetchMachines.mockResolvedValueOnce(enabled);
    await refreshMachinesStore();
    expect(get(machines)['atlas'].enabled).toBe(true);

    const disabled: Record<string, MachineInfo> = {
      atlas: makeMachine({ id: 'atlas', enabled: false }),
    };
    mockFetchMachines.mockResolvedValueOnce(disabled);
    await refreshMachinesStore();

    expect(get(machines)['atlas'].enabled).toBe(false);
  });
});
