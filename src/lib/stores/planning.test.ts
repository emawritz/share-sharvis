import { describe, it, expect, vi, beforeEach } from 'vitest';
import { get } from 'svelte/store';
import type { PlanningState, PlanStep, PlanningMessage } from '../types';

// Mock api and notifications before any store imports
vi.mock('../api', () => ({
  getPlanningState: vi.fn().mockResolvedValue(null),
  onPlanningUpdate: vi.fn().mockResolvedValue(() => {}),
}));

vi.mock('./notifications', () => ({
  sendSmartNotification: vi.fn(),
  addToast: vi.fn(),
}));

import {
  planningState,
  planningModalOpen,
  initPlanningStore,
  destroyPlanningStore,
} from './planning';
import { getPlanningState, onPlanningUpdate } from '../api';
import { sendSmartNotification } from './notifications';

const mockGetPlanningState = getPlanningState as ReturnType<typeof vi.fn>;
const mockOnPlanningUpdate = onPlanningUpdate as ReturnType<typeof vi.fn>;
const mockSendSmartNotification = sendSmartNotification as ReturnType<typeof vi.fn>;

function makePlanningState(overrides: Partial<PlanningState> = {}): PlanningState {
  return {
    id: 'plan-1',
    objetivo: 'Build feature X',
    phase: 'thinking',
    messages: [],
    planSteps: [],
    currentRound: 0,
    currentSpeaker: 'atlas',
    startedAt: '2026-01-01T00:00:00Z',
    elapsedSecs: 0,
    currentActivity: [],
    ...overrides,
  };
}

function makePlanStep(overrides: Partial<PlanStep> = {}): PlanStep {
  return {
    index: 0,
    target: 'atlas',
    description: 'Do something',
    status: 'pending',
    ...overrides,
  };
}

describe('planning store', () => {
  beforeEach(() => {
    destroyPlanningStore();
    // Reset all mocks after destroy (which also resets module-level state)
    vi.clearAllMocks();
    mockGetPlanningState.mockResolvedValue(null);
    mockOnPlanningUpdate.mockResolvedValue(() => {});
  });

  // ── Initial state ────────────────────────────────────────────────────

  it('planningState starts as null', () => {
    expect(get(planningState)).toBeNull();
  });

  it('planningModalOpen starts as false', () => {
    expect(get(planningModalOpen)).toBe(false);
  });

  // ── initPlanningStore — no active planning ───────────────────────────

  it('initPlanningStore sets planningState to null when API returns null', async () => {
    mockGetPlanningState.mockResolvedValueOnce(null);
    await initPlanningStore();
    expect(get(planningState)).toBeNull();
  });

  it('initPlanningStore does not open modal when state is null', async () => {
    mockGetPlanningState.mockResolvedValueOnce(null);
    await initPlanningStore();
    expect(get(planningModalOpen)).toBe(false);
  });

  // ── initPlanningStore — active planning ──────────────────────────────

  it('initPlanningStore sets planningState from API response', async () => {
    const state = makePlanningState({ phase: 'thinking' });
    mockGetPlanningState.mockResolvedValueOnce(state);
    await initPlanningStore();
    expect(get(planningState)).toEqual(state);
  });

  it('initPlanningStore opens modal for an active (non-terminal) phase', async () => {
    const state = makePlanningState({ phase: 'thinking' });
    mockGetPlanningState.mockResolvedValueOnce(state);
    await initPlanningStore();
    expect(get(planningModalOpen)).toBe(true);
  });

  it('initPlanningStore opens modal for "review" phase', async () => {
    const state = makePlanningState({ phase: 'review' });
    mockGetPlanningState.mockResolvedValueOnce(state);
    await initPlanningStore();
    expect(get(planningModalOpen)).toBe(true);
  });

  it('initPlanningStore does NOT open modal for "done" phase', async () => {
    const state = makePlanningState({ phase: 'done' });
    mockGetPlanningState.mockResolvedValueOnce(state);
    await initPlanningStore();
    expect(get(planningModalOpen)).toBe(false);
  });

  it('initPlanningStore does NOT open modal for "cancelled" phase', async () => {
    const state = makePlanningState({ phase: 'cancelled' });
    mockGetPlanningState.mockResolvedValueOnce(state);
    await initPlanningStore();
    expect(get(planningModalOpen)).toBe(false);
  });

  it('initPlanningStore does NOT open modal for "done-with-errors" phase', async () => {
    const state = makePlanningState({ phase: 'done-with-errors' });
    mockGetPlanningState.mockResolvedValueOnce(state);
    await initPlanningStore();
    expect(get(planningModalOpen)).toBe(false);
  });

  // ── initPlanningStore — idempotency ──────────────────────────────────

  it('initPlanningStore is idempotent — second call does not re-fetch', async () => {
    mockGetPlanningState.mockResolvedValue(null);
    await initPlanningStore();
    await initPlanningStore();
    expect(mockGetPlanningState).toHaveBeenCalledTimes(1);
  });

  // ── onPlanningUpdate callback ─────────────────────────────────────────

  it('onPlanningUpdate callback updates planningState', async () => {
    let capturedCallback: ((data: PlanningState) => void) | null = null;
    mockOnPlanningUpdate.mockImplementation((cb) => {
      capturedCallback = cb;
      return Promise.resolve(() => {});
    });
    mockGetPlanningState.mockResolvedValueOnce(null);

    await initPlanningStore();

    const update = makePlanningState({ phase: 'thinking', objetivo: 'Updated objective' });
    capturedCallback!(update);

    expect(get(planningState)).toEqual(update);
  });

  it('onPlanningUpdate callback sends notification when transitioning to "done"', async () => {
    let capturedCallback: ((data: PlanningState) => void) | null = null;
    mockOnPlanningUpdate.mockImplementation((cb) => {
      capturedCallback = cb;
      return Promise.resolve(() => {});
    });

    // Start from "thinking" phase
    const initial = makePlanningState({ phase: 'thinking' });
    mockGetPlanningState.mockResolvedValueOnce(initial);
    await initPlanningStore();

    // Transition to "done"
    capturedCallback!(makePlanningState({ phase: 'done', objetivo: 'My objective' }));

    expect(mockSendSmartNotification).toHaveBeenCalledWith(
      'planningDone',
      'Planning listo',
      'My objective',
      'success'
    );
  });

  it('onPlanningUpdate callback sends notification when transitioning to "review"', async () => {
    let capturedCallback: ((data: PlanningState) => void) | null = null;
    mockOnPlanningUpdate.mockImplementation((cb) => {
      capturedCallback = cb;
      return Promise.resolve(() => {});
    });

    const initial = makePlanningState({ phase: 'thinking' });
    mockGetPlanningState.mockResolvedValueOnce(initial);
    await initPlanningStore();

    capturedCallback!(makePlanningState({ phase: 'review', objetivo: 'Review time' }));

    expect(mockSendSmartNotification).toHaveBeenCalledWith(
      'planningDone',
      'Planning listo',
      'Review time',
      'success'
    );
  });

  it('onPlanningUpdate does NOT send duplicate notifications for the same phase', async () => {
    let capturedCallback: ((data: PlanningState) => void) | null = null;
    mockOnPlanningUpdate.mockImplementation((cb) => {
      capturedCallback = cb;
      return Promise.resolve(() => {});
    });

    const initial = makePlanningState({ phase: 'thinking' });
    mockGetPlanningState.mockResolvedValueOnce(initial);
    await initPlanningStore();

    // Transition to done twice (same phase, no re-notify)
    capturedCallback!(makePlanningState({ phase: 'done' }));
    capturedCallback!(makePlanningState({ phase: 'done' }));

    expect(mockSendSmartNotification).toHaveBeenCalledTimes(1);
  });

  it('onPlanningUpdate truncates long objetivo strings in the notification', async () => {
    let capturedCallback: ((data: PlanningState) => void) | null = null;
    mockOnPlanningUpdate.mockImplementation((cb) => {
      capturedCallback = cb;
      return Promise.resolve(() => {});
    });

    const initial = makePlanningState({ phase: 'thinking' });
    mockGetPlanningState.mockResolvedValueOnce(initial);
    await initPlanningStore();

    const longObjetivo = 'A'.repeat(200);
    capturedCallback!(makePlanningState({ phase: 'done', objetivo: longObjetivo }));

    const callArgs = mockSendSmartNotification.mock.calls[0];
    // Third arg is the objetivo — should be truncated to 100 chars
    expect(callArgs[2]).toBe('A'.repeat(100));
  });

  // ── destroyPlanningStore ─────────────────────────────────────────────

  it('destroyPlanningStore resets planningState to null', async () => {
    const state = makePlanningState({ phase: 'thinking' });
    mockGetPlanningState.mockResolvedValueOnce(state);
    await initPlanningStore();
    expect(get(planningState)).not.toBeNull();

    destroyPlanningStore();
    expect(get(planningState)).toBeNull();
  });

  it('destroyPlanningStore resets planningModalOpen to false', async () => {
    const state = makePlanningState({ phase: 'thinking' });
    mockGetPlanningState.mockResolvedValueOnce(state);
    await initPlanningStore();
    expect(get(planningModalOpen)).toBe(true);

    destroyPlanningStore();
    expect(get(planningModalOpen)).toBe(false);
  });

  it('destroyPlanningStore calls the unlisten function', async () => {
    const unlistenMock = vi.fn();
    mockOnPlanningUpdate.mockResolvedValueOnce(unlistenMock);
    mockGetPlanningState.mockResolvedValueOnce(null);

    await initPlanningStore();
    destroyPlanningStore();

    expect(unlistenMock).toHaveBeenCalledTimes(1);
  });

  it('destroyPlanningStore allows re-initialization', async () => {
    mockGetPlanningState.mockResolvedValue(null);
    await initPlanningStore();
    destroyPlanningStore();

    const state = makePlanningState({ phase: 'review' });
    mockGetPlanningState.mockResolvedValueOnce(state);
    await initPlanningStore();

    expect(get(planningState)).toEqual(state);
    expect(get(planningModalOpen)).toBe(true);
  });

  // ── Plan step data ────────────────────────────────────────────────────

  it('planningState reflects plan steps from the API', async () => {
    const steps: PlanStep[] = [
      makePlanStep({ index: 0, description: 'Step one', status: 'done' }),
      makePlanStep({ index: 1, description: 'Step two', status: 'running' }),
      makePlanStep({ index: 2, description: 'Step three', status: 'pending' }),
    ];
    const state = makePlanningState({ phase: 'executing', planSteps: steps });
    mockGetPlanningState.mockResolvedValueOnce(state);

    await initPlanningStore();

    const stored = get(planningState);
    expect(stored?.planSteps).toHaveLength(3);
    expect(stored?.planSteps[0].status).toBe('done');
    expect(stored?.planSteps[1].status).toBe('running');
    expect(stored?.planSteps[2].status).toBe('pending');
  });

  it('planningState reflects messages from the API', async () => {
    const messages: PlanningMessage[] = [
      { sender: 'atlas', content: 'Hello', round: 1, timestamp: '2026-01-01T00:00:01Z' },
      { sender: 'pixel', content: 'World', round: 1, timestamp: '2026-01-01T00:00:02Z' },
    ];
    const state = makePlanningState({ phase: 'thinking', messages });
    mockGetPlanningState.mockResolvedValueOnce(state);

    await initPlanningStore();

    const stored = get(planningState);
    expect(stored?.messages).toHaveLength(2);
    expect(stored?.messages[0].sender).toBe('atlas');
    expect(stored?.messages[1].sender).toBe('pixel');
  });
});
