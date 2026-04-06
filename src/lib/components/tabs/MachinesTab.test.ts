import { describe, it, expect } from 'vitest';

// Pure functions extracted from MachinesTab.svelte for isolated testing.
// No Svelte mounting — all logic is tested directly.

// machineColor: deterministic palette selection based on id hash.
function machineColor(id: string): string {
  const palette = ['#7eb8ff', '#7effa0', '#ffb74d', '#c084fc', '#f48fb1', '#4fc3f7', '#ff8a65', '#aed581'];
  let hash = 0;
  for (let i = 0; i < id.length; i++) hash = ((hash << 5) - hash + id.charCodeAt(i)) | 0;
  return palette[Math.abs(hash) % palette.length];
}

// pctClass: returns CSS class name based on numeric percentage value.
function pctClass(val: string | undefined): string {
  const n = parseInt(val || '');
  if (isNaN(n)) return 'off';
  if (n >= 90) return 'crit';
  if (n >= 70) return 'warn';
  return 'ok';
}

// isOnline: derives online state from machine health.
function isOnline(health: { online?: boolean } | undefined): boolean {
  return health?.online === true;
}

// hostDisplay: returns the host string as-is (template shows m.ip || m.host || '').
function hostDisplay(ip: string | undefined, host: string | undefined): string {
  return ip || host || '';
}

// statsVisible: stats panel is only shown when machine is online or stats.online is true.
function statsVisible(
  machineOnline: boolean,
  stats: { online?: boolean } | null | undefined
): boolean {
  return !!((stats?.online || machineOnline) ? stats : null);
}

// gpuDisplay: parse the raw GPU string to show only the percentage part.
function gpuDisplay(raw: string): string {
  const m = (raw + '').match(/^(\d+)%/);
  return m ? m[1] + '%' : raw;
}

// machineEnabled: derives enabled flag from machine config (default true).
function machineEnabled(enabled: boolean | undefined): boolean {
  return enabled !== false;
}

// isMonitorOffline: machine is tracked as offline by the monitor (separate from health).
function isMonitorOffline(offlineSet: Set<string>, id: string): boolean {
  return offlineSet.has(id);
}

// cardClasses: produces the CSS class list for a machine card.
function cardClasses(online: boolean, monitorOffline: boolean): string[] {
  const classes = ['machine-card'];
  if (online) classes.push('online');
  else classes.push('offline');
  if (monitorOffline) classes.push('monitor-offline');
  return classes;
}

// dotActive: the indicator dot is active only when online and NOT monitor-offline.
function dotActive(online: boolean, monitorOffline: boolean): boolean {
  return online && !monitorOffline;
}

// roleLabel: returns role, then os as fallback, then empty string.
function roleLabel(role: string | undefined, os: string | undefined): string {
  return role || os || '';
}

// ---------------------------------------------------------------------------
// machineColor
// ---------------------------------------------------------------------------

describe('machineColor', () => {
  const palette = ['#7eb8ff', '#7effa0', '#ffb74d', '#c084fc', '#f48fb1', '#4fc3f7', '#ff8a65', '#aed581'];

  it('returns a colour from the palette for "atlas"', () => {
    expect(palette).toContain(machineColor('atlas'));
  });

  it('returns a colour from the palette for "pixel"', () => {
    expect(palette).toContain(machineColor('pixel'));
  });

  it('is deterministic — same id always gives same colour', () => {
    expect(machineColor('atlas')).toBe(machineColor('atlas'));
    expect(machineColor('pixel')).toBe(machineColor('pixel'));
  });

  it('different ids can produce different colours', () => {
    const results = new Set(['atlas', 'pixel', 'node1', 'node2', 'gpu'].map(machineColor));
    // At least 2 distinct colours for 5 different ids
    expect(results.size).toBeGreaterThanOrEqual(2);
  });

  it('returns a hex colour string starting with #', () => {
    expect(machineColor('testmachine')).toMatch(/^#[0-9a-f]{6}$/);
  });

  it('handles single-character ids', () => {
    expect(palette).toContain(machineColor('a'));
  });

  it('handles empty string id', () => {
    expect(palette).toContain(machineColor(''));
  });
});

// ---------------------------------------------------------------------------
// pctClass
// ---------------------------------------------------------------------------

describe('pctClass', () => {
  it('returns "off" for undefined', () => {
    expect(pctClass(undefined)).toBe('off');
  });

  it('returns "off" for empty string', () => {
    expect(pctClass('')).toBe('off');
  });

  it('returns "off" for non-numeric string', () => {
    expect(pctClass('n/a')).toBe('off');
    expect(pctClass('-')).toBe('off');
    expect(pctClass('abc')).toBe('off');
  });

  it('returns "ok" for values below 70', () => {
    expect(pctClass('0')).toBe('ok');
    expect(pctClass('50')).toBe('ok');
    expect(pctClass('69')).toBe('ok');
  });

  it('returns "warn" for values 70–89', () => {
    expect(pctClass('70')).toBe('warn');
    expect(pctClass('80')).toBe('warn');
    expect(pctClass('89')).toBe('warn');
  });

  it('returns "crit" for values >= 90', () => {
    expect(pctClass('90')).toBe('crit');
    expect(pctClass('95')).toBe('crit');
    expect(pctClass('100')).toBe('crit');
  });

  it('parses strings like "87%" (parseInt stops at non-digit)', () => {
    // parseInt('87%') === 87 → 'warn'
    expect(pctClass('87%')).toBe('warn');
    expect(pctClass('92%')).toBe('crit');
    expect(pctClass('50%')).toBe('ok');
  });

  it('returns "ok" for "0"', () => {
    expect(pctClass('0')).toBe('ok');
  });
});

// ---------------------------------------------------------------------------
// isOnline
// ---------------------------------------------------------------------------

describe('isOnline', () => {
  it('returns true when health.online is true', () => {
    expect(isOnline({ online: true })).toBe(true);
  });

  it('returns false when health.online is false', () => {
    expect(isOnline({ online: false })).toBe(false);
  });

  it('returns false when health is undefined', () => {
    expect(isOnline(undefined)).toBe(false);
  });

  it('returns false when health.online is undefined', () => {
    expect(isOnline({})).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// hostDisplay
// ---------------------------------------------------------------------------

describe('hostDisplay', () => {
  it('returns ip when ip is set', () => {
    expect(hostDisplay('100.64.0.1', 'pixel')).toBe('100.64.0.1');
  });

  it('returns host when ip is absent', () => {
    expect(hostDisplay(undefined, 'pixel')).toBe('pixel');
  });

  it('returns "local" when host is "local"', () => {
    expect(hostDisplay(undefined, 'local')).toBe('local');
  });

  it('returns empty string when both are absent', () => {
    expect(hostDisplay(undefined, undefined)).toBe('');
  });

  it('prefers ip over host', () => {
    expect(hostDisplay('192.168.1.100', 'atlas')).toBe('192.168.1.100');
  });
});

// ---------------------------------------------------------------------------
// statsVisible
// ---------------------------------------------------------------------------

describe('statsVisible', () => {
  it('shows stats when machine is online and stats exist', () => {
    expect(statsVisible(true, { online: false, cpu: '50%' } as any)).toBe(true);
  });

  it('shows stats when stats.online is true even if machine is offline', () => {
    expect(statsVisible(false, { online: true, cpu: '30%' } as any)).toBe(true);
  });

  it('hides stats when machine is offline and stats.online is false', () => {
    expect(statsVisible(false, { online: false } as any)).toBe(false);
  });

  it('hides stats when stats is null', () => {
    expect(statsVisible(false, null)).toBe(false);
  });

  it('hides stats when stats is undefined', () => {
    expect(statsVisible(false, undefined)).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// gpuDisplay
// ---------------------------------------------------------------------------

describe('gpuDisplay', () => {
  it('extracts percentage from "45% NVIDIA RTX"', () => {
    expect(gpuDisplay('45% NVIDIA RTX')).toBe('45%');
  });

  it('extracts percentage from "87%"', () => {
    expect(gpuDisplay('87%')).toBe('87%');
  });

  it('returns raw string when no leading percentage', () => {
    expect(gpuDisplay('NVIDIA RTX 3070')).toBe('NVIDIA RTX 3070');
    expect(gpuDisplay('n/a')).toBe('n/a');
    expect(gpuDisplay('-')).toBe('-');
  });

  it('handles "0%"', () => {
    expect(gpuDisplay('0%')).toBe('0%');
  });
});

// ---------------------------------------------------------------------------
// machineEnabled
// ---------------------------------------------------------------------------

describe('machineEnabled', () => {
  it('returns true when enabled is true', () => {
    expect(machineEnabled(true)).toBe(true);
  });

  it('returns false when enabled is false', () => {
    expect(machineEnabled(false)).toBe(false);
  });

  it('defaults to true when enabled is undefined', () => {
    expect(machineEnabled(undefined)).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// isMonitorOffline
// ---------------------------------------------------------------------------

describe('isMonitorOffline', () => {
  it('returns true when id is in the offline set', () => {
    expect(isMonitorOffline(new Set(['pixel', 'node1']), 'pixel')).toBe(true);
  });

  it('returns false when id is not in the offline set', () => {
    expect(isMonitorOffline(new Set(['pixel']), 'atlas')).toBe(false);
  });

  it('returns false for empty set', () => {
    expect(isMonitorOffline(new Set(), 'atlas')).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// cardClasses
// ---------------------------------------------------------------------------

describe('cardClasses', () => {
  it('includes "online" class when machine is online', () => {
    expect(cardClasses(true, false)).toContain('online');
    expect(cardClasses(true, false)).not.toContain('offline');
  });

  it('includes "offline" class when machine is offline', () => {
    expect(cardClasses(false, false)).toContain('offline');
    expect(cardClasses(false, false)).not.toContain('online');
  });

  it('includes "monitor-offline" class when monitor-offline is true', () => {
    expect(cardClasses(true, true)).toContain('monitor-offline');
    expect(cardClasses(false, true)).toContain('monitor-offline');
  });

  it('does not include "monitor-offline" when not flagged', () => {
    expect(cardClasses(true, false)).not.toContain('monitor-offline');
  });

  it('always includes "machine-card" base class', () => {
    expect(cardClasses(true, false)).toContain('machine-card');
    expect(cardClasses(false, true)).toContain('machine-card');
  });
});

// ---------------------------------------------------------------------------
// dotActive
// ---------------------------------------------------------------------------

describe('dotActive', () => {
  it('is active when online and not monitor-offline', () => {
    expect(dotActive(true, false)).toBe(true);
  });

  it('is not active when offline', () => {
    expect(dotActive(false, false)).toBe(false);
  });

  it('is not active when online but monitor-offline', () => {
    expect(dotActive(true, true)).toBe(false);
  });

  it('is not active when offline and monitor-offline', () => {
    expect(dotActive(false, true)).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// roleLabel
// ---------------------------------------------------------------------------

describe('roleLabel', () => {
  it('returns role when set', () => {
    expect(roleLabel('orchestrator/backend', 'macos')).toBe('orchestrator/backend');
  });

  it('falls back to os when role is undefined', () => {
    expect(roleLabel(undefined, 'linux')).toBe('linux');
  });

  it('falls back to os when role is empty string', () => {
    expect(roleLabel('', 'macos')).toBe('macos');
  });

  it('returns empty string when both are absent', () => {
    expect(roleLabel(undefined, undefined)).toBe('');
  });
});
