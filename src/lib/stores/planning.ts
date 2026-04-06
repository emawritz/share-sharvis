import { writable, get } from 'svelte/store';
import type { PlanningState } from '../types';
import { getPlanningState, onPlanningUpdate } from '../api';
import { sendSmartNotification } from './notifications';

export const planningState = writable<PlanningState | null>(null);
export const planningModalOpen = writable<boolean>(false);

let initialized = false;
let previousPhase: string | null = null;
let unlistenPlanning: (() => void) | null = null;

export async function initPlanningStore() {
  if (initialized) return;
  initialized = true;

  try {
    const state = await getPlanningState();
    planningState.set(state);
    previousPhase = state?.phase ?? null;
    if (state && !['done', 'cancelled', 'done-with-errors'].includes(state.phase)) {
      planningModalOpen.set(true);
    }
  } catch (e) {
    console.error('planning:', e);
  }

  try {
    unlistenPlanning = await onPlanningUpdate((data) => {
      planningState.set(data);
      // Notify when planning transitions to review or done
      if (data && (data.phase === 'review' || data.phase === 'done') && previousPhase !== data.phase) {
        const objetivo = data.objetivo ? data.objetivo.substring(0, 100) : '';
        sendSmartNotification(
          'planningDone',
          'Planning listo',
          objetivo,
          'success'
        );
      }
      previousPhase = data?.phase ?? null;
    });
  } catch (e) {
    console.error('planning:', e);
  }
}

export function destroyPlanningStore() {
  if (unlistenPlanning) { unlistenPlanning(); unlistenPlanning = null; }
  initialized = false;
  previousPhase = null;
  planningState.set(null);
  planningModalOpen.set(false);
}
