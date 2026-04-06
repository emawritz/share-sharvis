import { describe, it, expect } from 'vitest';

// Pure functions extracted from PipelinesTab.svelte for isolated testing.
// No Svelte mounting — all logic is tested as plain TypeScript functions.

// ----- Types (mirrored from src/lib/types.ts) -----

interface PipelineStepState {
  name: string;
  target: string;
  status: string;
  output?: string;
  startedAt?: string;
  finishedAt?: string;
  retries: number;
}

interface PipelineState {
  id: string;
  name: string;
  description: string;
  status: string;
  currentStep: number;
  startedAt?: string;
  finishedAt?: string;
  steps: PipelineStepState[];
}

interface BuiltinInfo {
  name: string;
  description: string;
  steps: number;
}

// ----- Pure logic functions -----

// pipelineStatusBadgeClass: maps pipeline status → CSS class used in the UI.
function pipelineStatusBadgeClass(status: string): string {
  switch (status) {
    case 'running': return 'status-running';
    case 'done':    return 'status-done';
    case 'failed':  return 'status-failed';
    case 'pending': return 'status-pending';
    default:        return 'status-pending';
  }
}

// isRunning: returns true when a pipeline with the given name is currently running.
// Mirrors the component's `isRunning(name)` helper.
function isRunning(running: PipelineState[], name: string): boolean {
  return running.some((r) => r.name === name && r.status === 'running');
}

// currentStepIndex: returns the 0-based index of the active step (currentStep is 1-based in the Rust type).
function currentStepIndex(pipeline: PipelineState): number {
  return Math.max(0, pipeline.currentStep - 1);
}

// completedSteps: returns all steps whose status is 'done'.
function completedSteps(pipeline: PipelineState): PipelineStepState[] {
  return pipeline.steps.filter((s) => s.status === 'done');
}

// progressPercent: 0–100 integer representing completion ratio.
function progressPercent(pipeline: PipelineState): number {
  if (pipeline.steps.length === 0) return 0;
  return Math.round((pipeline.currentStep / pipeline.steps.length) * 100);
}

// pipelineDurationMs: returns elapsed milliseconds between startedAt and finishedAt
// (or now when still running). Returns 0 if startedAt is absent.
function pipelineDurationMs(pipeline: PipelineState, now: number): number {
  if (!pipeline.startedAt) return 0;
  const start = new Date(pipeline.startedAt).getTime();
  const end = pipeline.finishedAt ? new Date(pipeline.finishedAt).getTime() : now;
  return Math.max(0, end - start);
}

// formatPipelineDuration: human-readable "Xs" / "XmYs" string.
function formatPipelineDuration(ms: number): string {
  const secs = Math.round(ms / 1000);
  if (secs === 0) return '-';
  if (secs < 60) return `${secs}s`;
  const m = Math.floor(secs / 60);
  const s = secs % 60;
  return `${m}m${s}s`;
}

// pipelineRunId: UI-friendly ID (first 8 chars of the UUID).
function pipelineRunId(id: string): string {
  return id.substring(0, 8);
}

// hasDescription: guard used to conditionally render the description line.
function hasDescription(info: BuiltinInfo): boolean {
  return !!info.description && info.description.length > 0;
}

// stepStatusBadgeClass: per-step status → badge class.
function stepStatusBadgeClass(status: string): string {
  switch (status) {
    case 'running': return 'step-running';
    case 'done':    return 'step-done';
    case 'failed':  return 'step-failed';
    case 'skipped': return 'step-skipped';
    default:        return 'step-pending';
  }
}

// builtinStepLabel: pluralise "step" / "steps" for display.
function builtinStepLabel(count: number): string {
  return count === 1 ? `${count} step` : `${count} steps`;
}

// ---------------------------------------------------------------------------
// pipelineStatusBadgeClass
// ---------------------------------------------------------------------------

describe('pipelineStatusBadgeClass', () => {
  it('maps "running" to status-running', () => {
    expect(pipelineStatusBadgeClass('running')).toBe('status-running');
  });

  it('maps "done" to status-done', () => {
    expect(pipelineStatusBadgeClass('done')).toBe('status-done');
  });

  it('maps "failed" to status-failed', () => {
    expect(pipelineStatusBadgeClass('failed')).toBe('status-failed');
  });

  it('maps "pending" to status-pending', () => {
    expect(pipelineStatusBadgeClass('pending')).toBe('status-pending');
  });

  it('falls back to status-pending for unknown status', () => {
    expect(pipelineStatusBadgeClass('cancelled')).toBe('status-pending');
    expect(pipelineStatusBadgeClass('')).toBe('status-pending');
  });
});

// ---------------------------------------------------------------------------
// isRunning
// ---------------------------------------------------------------------------

describe('isRunning', () => {
  const pipelines: PipelineState[] = [
    { id: 'a', name: 'deploy', description: '', status: 'running', currentStep: 1, steps: [] },
    { id: 'b', name: 'lint',   description: '', status: 'done',    currentStep: 2, steps: [] },
  ];

  it('returns true when a pipeline with that name is running', () => {
    expect(isRunning(pipelines, 'deploy')).toBe(true);
  });

  it('returns false when the named pipeline is done (not running)', () => {
    expect(isRunning(pipelines, 'lint')).toBe(false);
  });

  it('returns false when name does not exist', () => {
    expect(isRunning(pipelines, 'missing')).toBe(false);
  });

  it('returns false for an empty running list', () => {
    expect(isRunning([], 'deploy')).toBe(false);
  });

  it('returns true only for the matching name', () => {
    // Both pipelines share status=running; only the named one should match
    const two: PipelineState[] = [
      { id: 'x', name: 'alpha', description: '', status: 'running', currentStep: 1, steps: [] },
      { id: 'y', name: 'beta',  description: '', status: 'running', currentStep: 1, steps: [] },
    ];
    expect(isRunning(two, 'alpha')).toBe(true);
    expect(isRunning(two, 'beta')).toBe(true);
    expect(isRunning(two, 'gamma')).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// currentStepIndex
// ---------------------------------------------------------------------------

describe('currentStepIndex', () => {
  const makeP = (currentStep: number): PipelineState => ({
    id: '', name: '', description: '', status: 'running', currentStep, steps: [],
  });

  it('converts 1-based currentStep to 0-based index', () => {
    expect(currentStepIndex(makeP(1))).toBe(0);
    expect(currentStepIndex(makeP(2))).toBe(1);
    expect(currentStepIndex(makeP(5))).toBe(4);
  });

  it('clamps to 0 when currentStep is 0', () => {
    expect(currentStepIndex(makeP(0))).toBe(0);
  });
});

// ---------------------------------------------------------------------------
// completedSteps
// ---------------------------------------------------------------------------

describe('completedSteps', () => {
  const makeStep = (status: string): PipelineStepState => ({
    name: 'step', target: 'atlas', status, retries: 0,
  });

  it('returns only steps with status "done"', () => {
    const pipeline: PipelineState = {
      id: '', name: '', description: '', status: 'running', currentStep: 2,
      steps: [makeStep('done'), makeStep('running'), makeStep('done'), makeStep('pending')],
    };
    expect(completedSteps(pipeline)).toHaveLength(2);
    expect(completedSteps(pipeline).every(s => s.status === 'done')).toBe(true);
  });

  it('returns empty array when no steps are done', () => {
    const pipeline: PipelineState = {
      id: '', name: '', description: '', status: 'running', currentStep: 1,
      steps: [makeStep('running'), makeStep('pending')],
    };
    expect(completedSteps(pipeline)).toHaveLength(0);
  });

  it('returns all steps when all are done', () => {
    const pipeline: PipelineState = {
      id: '', name: '', description: '', status: 'done', currentStep: 3,
      steps: [makeStep('done'), makeStep('done'), makeStep('done')],
    };
    expect(completedSteps(pipeline)).toHaveLength(3);
  });

  it('handles pipeline with no steps', () => {
    const pipeline: PipelineState = {
      id: '', name: '', description: '', status: 'pending', currentStep: 0, steps: [],
    };
    expect(completedSteps(pipeline)).toHaveLength(0);
  });
});

// ---------------------------------------------------------------------------
// progressPercent
// ---------------------------------------------------------------------------

describe('progressPercent', () => {
  const makeP = (currentStep: number, totalSteps: number): PipelineState => ({
    id: '', name: '', description: '', status: 'running', currentStep,
    steps: Array.from({ length: totalSteps }, (_, i) => ({
      name: `step${i}`, target: 'atlas', status: 'pending', retries: 0,
    })),
  });

  it('returns 0 when there are no steps', () => {
    const p: PipelineState = { id: '', name: '', description: '', status: 'pending', currentStep: 0, steps: [] };
    expect(progressPercent(p)).toBe(0);
  });

  it('returns 0 when currentStep is 0 out of N', () => {
    expect(progressPercent(makeP(0, 4))).toBe(0);
  });

  it('returns 50 for half-way through', () => {
    expect(progressPercent(makeP(2, 4))).toBe(50);
  });

  it('returns 100 when all steps complete', () => {
    expect(progressPercent(makeP(5, 5))).toBe(100);
  });

  it('rounds to nearest integer', () => {
    // 1/3 = 33.33... → 33
    expect(progressPercent(makeP(1, 3))).toBe(33);
  });
});

// ---------------------------------------------------------------------------
// pipelineDurationMs
// ---------------------------------------------------------------------------

describe('pipelineDurationMs', () => {
  it('returns 0 when startedAt is absent', () => {
    const p: PipelineState = { id: '', name: '', description: '', status: 'pending', currentStep: 0, steps: [] };
    expect(pipelineDurationMs(p, Date.now())).toBe(0);
  });

  it('measures finished duration between startedAt and finishedAt', () => {
    const p: PipelineState = {
      id: '', name: '', description: '', status: 'done', currentStep: 3, steps: [],
      startedAt: '2024-01-01T00:00:00.000Z',
      finishedAt: '2024-01-01T00:01:30.000Z',
    };
    expect(pipelineDurationMs(p, Date.now())).toBe(90_000);
  });

  it('uses `now` as end time when finishedAt is absent (still running)', () => {
    const start = new Date('2024-01-01T00:00:00.000Z').getTime();
    const now   = start + 45_000;
    const p: PipelineState = {
      id: '', name: '', description: '', status: 'running', currentStep: 1, steps: [],
      startedAt: '2024-01-01T00:00:00.000Z',
    };
    expect(pipelineDurationMs(p, now)).toBe(45_000);
  });
});

// ---------------------------------------------------------------------------
// formatPipelineDuration
// ---------------------------------------------------------------------------

describe('formatPipelineDuration', () => {
  it('returns "-" for 0 ms', () => {
    expect(formatPipelineDuration(0)).toBe('-');
  });

  it('returns seconds for values under 1 minute', () => {
    expect(formatPipelineDuration(30_000)).toBe('30s');
    expect(formatPipelineDuration(1_000)).toBe('1s');
    expect(formatPipelineDuration(59_000)).toBe('59s');
  });

  it('returns minutes+seconds for 60+ seconds', () => {
    expect(formatPipelineDuration(60_000)).toBe('1m0s');
    expect(formatPipelineDuration(90_000)).toBe('1m30s');
    expect(formatPipelineDuration(150_000)).toBe('2m30s');
  });

  it('handles large durations', () => {
    expect(formatPipelineDuration(3_600_000)).toBe('60m0s');
  });
});

// ---------------------------------------------------------------------------
// pipelineRunId
// ---------------------------------------------------------------------------

describe('pipelineRunId', () => {
  it('returns the first 8 characters of a UUID', () => {
    expect(pipelineRunId('abcd1234-5678-90ab-cdef-111122223333')).toBe('abcd1234');
  });

  it('returns the whole string when it is shorter than 8 chars', () => {
    expect(pipelineRunId('abc')).toBe('abc');
  });

  it('handles an empty string', () => {
    expect(pipelineRunId('')).toBe('');
  });
});

// ---------------------------------------------------------------------------
// hasDescription
// ---------------------------------------------------------------------------

describe('hasDescription', () => {
  it('returns true when description is non-empty', () => {
    expect(hasDescription({ name: 'p', description: 'Deploy to prod', steps: 3 })).toBe(true);
  });

  it('returns false when description is an empty string', () => {
    expect(hasDescription({ name: 'p', description: '', steps: 3 })).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// stepStatusBadgeClass
// ---------------------------------------------------------------------------

describe('stepStatusBadgeClass', () => {
  it('maps "running" to step-running', () => {
    expect(stepStatusBadgeClass('running')).toBe('step-running');
  });

  it('maps "done" to step-done', () => {
    expect(stepStatusBadgeClass('done')).toBe('step-done');
  });

  it('maps "failed" to step-failed', () => {
    expect(stepStatusBadgeClass('failed')).toBe('step-failed');
  });

  it('maps "skipped" to step-skipped', () => {
    expect(stepStatusBadgeClass('skipped')).toBe('step-skipped');
  });

  it('falls back to step-pending for unknown or empty status', () => {
    expect(stepStatusBadgeClass('pending')).toBe('step-pending');
    expect(stepStatusBadgeClass('')).toBe('step-pending');
    expect(stepStatusBadgeClass('waiting')).toBe('step-pending');
  });
});

// ---------------------------------------------------------------------------
// builtinStepLabel
// ---------------------------------------------------------------------------

describe('builtinStepLabel', () => {
  it('uses singular "step" for 1', () => {
    expect(builtinStepLabel(1)).toBe('1 step');
  });

  it('uses plural "steps" for 0', () => {
    expect(builtinStepLabel(0)).toBe('0 steps');
  });

  it('uses plural "steps" for 2+', () => {
    expect(builtinStepLabel(2)).toBe('2 steps');
    expect(builtinStepLabel(10)).toBe('10 steps');
  });
});
