import { describe, it, expect } from 'vitest';

// Pure functions extracted from SettingsTab.svelte and its sub-components
// (ConnectionsSubtab.svelte, MachinesSubtab.svelte) for isolated testing.
// No Svelte mounting — all logic is tested as plain TypeScript functions.

// ---------------------------------------------------------------------------
// statusIcon: maps a connection check status to a Unicode symbol.
// From ConnectionsSubtab.svelte → statusIcon()
// ---------------------------------------------------------------------------
function statusIcon(status: string): string {
  if (status === 'ok') return '\u2713';
  if (status === 'warning') return '\u26A0';
  return '\u2717';
}

// ---------------------------------------------------------------------------
// statusColor: maps a connection check status to a CSS color string.
// From ConnectionsSubtab.svelte → statusColor()
// ---------------------------------------------------------------------------
function statusColor(status: string): string {
  if (status === 'ok') return 'var(--green)';
  if (status === 'warning') return '#ffb74d';
  return '#ef5350';
}

// ---------------------------------------------------------------------------
// isConnected: helper derived from the connection status model.
// Returns true only when status === 'ok'.
// ---------------------------------------------------------------------------
function isConnected(status: string): boolean {
  return status === 'ok';
}

// ---------------------------------------------------------------------------
// isChecking: returns true when a machine's result is not yet in the map.
// Mirrors: `if (!mc)` in the template — machine not in connectionResults yet.
// ---------------------------------------------------------------------------
function isChecking(connectionResults: Record<string, unknown>, machineId: string): boolean {
  return !(machineId in connectionResults);
}

// ---------------------------------------------------------------------------
// machineIdFromName: mirrors saveMachine() auto-ID logic:
//   id = name.toLowerCase().replace(/[^a-z0-9]/g, '')
// Used when editingMachine.id is empty at save time.
// ---------------------------------------------------------------------------
function machineIdFromName(name: string): string {
  return name.toLowerCase().replace(/[^a-z0-9]/g, '');
}

// ---------------------------------------------------------------------------
// isLocalHost: mirrors the guard in testSSH():
//   if (!host || host === 'local') return
// Returns true when SSH test should be skipped (local or empty host).
// ---------------------------------------------------------------------------
function isLocalHost(host: string | undefined): boolean {
  return !host || host === 'local';
}

// ---------------------------------------------------------------------------
// parseTags: mirrors the oninput handler for the tags field:
//   value.split(',').map(t => t.trim()).filter(Boolean)
// ---------------------------------------------------------------------------
function parseTags(value: string): string[] {
  return value.split(',').map((t) => t.trim()).filter(Boolean);
}

// ---------------------------------------------------------------------------
// isMachineValid: a machine form is valid when it has a non-empty name and host.
// Derived from saveMachine() which does not block on missing name/host but
// the UI fields are required in practice.
// ---------------------------------------------------------------------------
function isMachineValid(machine: { name: string; host: string }): boolean {
  return machine.name.trim().length > 0 && machine.host.trim().length > 0;
}

// ---------------------------------------------------------------------------
// isRepoValid: a repo entry is valid when it has both a name and a path.
// GitHub slug is optional (local-only repos have no github field).
// ---------------------------------------------------------------------------
function isRepoValid(repo: { name: string; path: string; github?: string }): boolean {
  return repo.name.trim().length > 0 && repo.path.trim().length > 0;
}

// ---------------------------------------------------------------------------
// isSSHHost: validates that a host string looks like a valid SSH alias or IP.
// Allows alphanumeric, dots, dashes, underscores (no spaces, no protocol).
// ---------------------------------------------------------------------------
function isSSHHost(host: string): boolean {
  if (!host || host === 'local') return true; // local is always valid
  return /^[a-zA-Z0-9._-]+$/.test(host);
}

// ---------------------------------------------------------------------------
// isUnixPath: validates a Unix-style absolute or home-relative path.
// Accepts paths starting with / or ~/
// ---------------------------------------------------------------------------
function isUnixPath(path: string): boolean {
  if (!path) return false;
  return path.startsWith('/') || path.startsWith('~/');
}

// ---------------------------------------------------------------------------
// formatMachineRepoCount: mirrors `{m.repos.length} repos` in the template.
// ---------------------------------------------------------------------------
function formatMachineRepoCount(count: number): string {
  return `${count} repos`;
}

// ---------------------------------------------------------------------------
// isOsValid: OS must be one of the three allowed values.
// ---------------------------------------------------------------------------
function isOsValid(os: string): boolean {
  return ['macos', 'linux', 'windows'].includes(os);
}

// ---------------------------------------------------------------------------
// deduplicateMachineIds: ensures no two machines share the same id.
// ---------------------------------------------------------------------------
interface MinimalMachine { id: string; name: string }

function hasDuplicateMachineIds(machines: MinimalMachine[]): boolean {
  const ids = machines.map((m) => m.id);
  return new Set(ids).size !== ids.length;
}

// ===========================================================================
// TESTS
// ===========================================================================

// ---------------------------------------------------------------------------
// statusIcon
// ---------------------------------------------------------------------------
describe('statusIcon', () => {
  it('returns checkmark for ok status', () => {
    expect(statusIcon('ok')).toBe('\u2713');
  });

  it('returns warning triangle for warning status', () => {
    expect(statusIcon('warning')).toBe('\u26A0');
  });

  it('returns X mark for error status', () => {
    expect(statusIcon('error')).toBe('\u2717');
  });

  it('returns X mark for unknown status', () => {
    expect(statusIcon('unknown')).toBe('\u2717');
  });

  it('returns X mark for empty string', () => {
    expect(statusIcon('')).toBe('\u2717');
  });
});

// ---------------------------------------------------------------------------
// statusColor
// ---------------------------------------------------------------------------
describe('statusColor', () => {
  it('returns green CSS var for ok', () => {
    expect(statusColor('ok')).toBe('var(--green)');
  });

  it('returns amber hex for warning', () => {
    expect(statusColor('warning')).toBe('#ffb74d');
  });

  it('returns red hex for error', () => {
    expect(statusColor('error')).toBe('#ef5350');
  });

  it('returns red hex for unknown status', () => {
    expect(statusColor('')).toBe('#ef5350');
    expect(statusColor('failed')).toBe('#ef5350');
  });
});

// ---------------------------------------------------------------------------
// isConnected
// ---------------------------------------------------------------------------
describe('isConnected', () => {
  it('returns true for ok', () => {
    expect(isConnected('ok')).toBe(true);
  });

  it('returns false for warning', () => {
    expect(isConnected('warning')).toBe(false);
  });

  it('returns false for error', () => {
    expect(isConnected('error')).toBe(false);
  });

  it('returns false for empty string', () => {
    expect(isConnected('')).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// isChecking
// ---------------------------------------------------------------------------
describe('isChecking', () => {
  it('returns true when machine id not yet in results', () => {
    expect(isChecking({}, 'atlas')).toBe(true);
  });

  it('returns false when machine id is present in results', () => {
    expect(isChecking({ atlas: {} }, 'atlas')).toBe(false);
  });

  it('returns true for one missing, false for one present', () => {
    const results = { atlas: {} };
    expect(isChecking(results, 'atlas')).toBe(false);
    expect(isChecking(results, 'pixel')).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// machineIdFromName
// ---------------------------------------------------------------------------
describe('machineIdFromName', () => {
  it('lowercases the name', () => {
    expect(machineIdFromName('PIXEL')).toBe('pixel');
  });

  it('strips spaces', () => {
    expect(machineIdFromName('my machine')).toBe('mymachine');
  });

  it('strips special characters', () => {
    expect(machineIdFromName('Atlas-01!')).toBe('atlas01');
  });

  it('keeps alphanumeric characters', () => {
    expect(machineIdFromName('node01')).toBe('node01');
  });

  it('returns empty string for all-special input', () => {
    expect(machineIdFromName('---!!!')).toBe('');
  });
});

// ---------------------------------------------------------------------------
// isLocalHost
// ---------------------------------------------------------------------------
describe('isLocalHost', () => {
  it('returns true for "local"', () => {
    expect(isLocalHost('local')).toBe(true);
  });

  it('returns true for empty string', () => {
    expect(isLocalHost('')).toBe(true);
  });

  it('returns true for undefined', () => {
    expect(isLocalHost(undefined)).toBe(true);
  });

  it('returns false for a real SSH alias', () => {
    expect(isLocalHost('pixel')).toBe(false);
  });

  it('returns false for an IP address', () => {
    expect(isLocalHost('100.64.0.1')).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// parseTags
// ---------------------------------------------------------------------------
describe('parseTags', () => {
  it('splits comma-separated tags', () => {
    expect(parseTags('backend, gpu, remote')).toEqual(['backend', 'gpu', 'remote']);
  });

  it('trims whitespace from each tag', () => {
    expect(parseTags('  a , b , c  ')).toEqual(['a', 'b', 'c']);
  });

  it('filters out empty segments', () => {
    expect(parseTags('a,,b,')).toEqual(['a', 'b']);
  });

  it('returns empty array for blank string', () => {
    expect(parseTags('')).toEqual([]);
  });

  it('handles a single tag with no comma', () => {
    expect(parseTags('backend')).toEqual(['backend']);
  });
});

// ---------------------------------------------------------------------------
// isMachineValid
// ---------------------------------------------------------------------------
describe('isMachineValid', () => {
  it('returns true when both name and host are provided', () => {
    expect(isMachineValid({ name: 'PIXEL', host: 'pixel' })).toBe(true);
  });

  it('returns true for local host', () => {
    expect(isMachineValid({ name: 'ATLAS', host: 'local' })).toBe(true);
  });

  it('returns false when name is empty', () => {
    expect(isMachineValid({ name: '', host: 'pixel' })).toBe(false);
  });

  it('returns false when host is empty', () => {
    expect(isMachineValid({ name: 'PIXEL', host: '' })).toBe(false);
  });

  it('returns false when both are empty', () => {
    expect(isMachineValid({ name: '', host: '' })).toBe(false);
  });

  it('returns false when name is only whitespace', () => {
    expect(isMachineValid({ name: '   ', host: 'pixel' })).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// isRepoValid
// ---------------------------------------------------------------------------
describe('isRepoValid', () => {
  it('returns true when name and path are present', () => {
    expect(isRepoValid({ name: 'jarvis', path: '~/jarvis' })).toBe(true);
  });

  it('returns true without a github slug', () => {
    expect(isRepoValid({ name: 'local-repo', path: '/home/user/repo', github: '' })).toBe(true);
  });

  it('returns false when name is empty', () => {
    expect(isRepoValid({ name: '', path: '~/jarvis' })).toBe(false);
  });

  it('returns false when path is empty', () => {
    expect(isRepoValid({ name: 'jarvis', path: '' })).toBe(false);
  });

  it('returns false when both are empty', () => {
    expect(isRepoValid({ name: '', path: '' })).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// isSSHHost
// ---------------------------------------------------------------------------
describe('isSSHHost', () => {
  it('accepts a simple SSH alias', () => {
    expect(isSSHHost('pixel')).toBe(true);
  });

  it('accepts an IP address', () => {
    expect(isSSHHost('100.64.0.1')).toBe(true);
  });

  it('accepts "local" as a special valid value', () => {
    expect(isSSHHost('local')).toBe(true);
  });

  it('rejects hosts with spaces', () => {
    expect(isSSHHost('my host')).toBe(false);
  });

  it('rejects hosts with protocol prefix', () => {
    expect(isSSHHost('ssh://pixel')).toBe(false);
  });

  it('returns true for empty string (treated as local)', () => {
    expect(isSSHHost('')).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// isUnixPath
// ---------------------------------------------------------------------------
describe('isUnixPath', () => {
  it('accepts absolute paths', () => {
    expect(isUnixPath('/Users/jane/jarvis')).toBe(true);
    expect(isUnixPath('/home/user/project')).toBe(true);
  });

  it('accepts home-relative paths', () => {
    expect(isUnixPath('~/jarvis')).toBe(true);
    expect(isUnixPath('~/projects/front')).toBe(true);
  });

  it('rejects relative paths', () => {
    expect(isUnixPath('jarvis')).toBe(false);
    expect(isUnixPath('./jarvis')).toBe(false);
  });

  it('rejects empty string', () => {
    expect(isUnixPath('')).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// formatMachineRepoCount
// ---------------------------------------------------------------------------
describe('formatMachineRepoCount', () => {
  it('formats zero repos', () => {
    expect(formatMachineRepoCount(0)).toBe('0 repos');
  });

  it('formats one repo', () => {
    expect(formatMachineRepoCount(1)).toBe('1 repos');
  });

  it('formats multiple repos', () => {
    expect(formatMachineRepoCount(5)).toBe('5 repos');
  });
});

// ---------------------------------------------------------------------------
// isOsValid
// ---------------------------------------------------------------------------
describe('isOsValid', () => {
  it('accepts macos', () => {
    expect(isOsValid('macos')).toBe(true);
  });

  it('accepts linux', () => {
    expect(isOsValid('linux')).toBe(true);
  });

  it('accepts windows', () => {
    expect(isOsValid('windows')).toBe(true);
  });

  it('rejects unknown OS strings', () => {
    expect(isOsValid('freebsd')).toBe(false);
    expect(isOsValid('')).toBe(false);
    expect(isOsValid('MacOS')).toBe(false); // case-sensitive
  });
});

// ---------------------------------------------------------------------------
// hasDuplicateMachineIds
// ---------------------------------------------------------------------------
describe('hasDuplicateMachineIds', () => {
  it('returns false for unique ids', () => {
    const machines = [{ id: 'atlas', name: 'ATLAS' }, { id: 'pixel', name: 'PIXEL' }];
    expect(hasDuplicateMachineIds(machines)).toBe(false);
  });

  it('returns true when two machines share the same id', () => {
    const machines = [{ id: 'atlas', name: 'ATLAS' }, { id: 'atlas', name: 'ATLAS2' }];
    expect(hasDuplicateMachineIds(machines)).toBe(true);
  });

  it('returns false for a single machine', () => {
    expect(hasDuplicateMachineIds([{ id: 'atlas', name: 'ATLAS' }])).toBe(false);
  });

  it('returns false for empty list', () => {
    expect(hasDuplicateMachineIds([])).toBe(false);
  });
});
