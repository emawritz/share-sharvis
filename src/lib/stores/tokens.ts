import { writable, derived, get } from 'svelte/store';
import { getTokenStats as apiGetTokenStats, getBudgetLimit, setBudgetLimit as apiSetBudgetLimit } from '../api';
import { sendSmartNotification } from './notifications';
import type { TokenStats as BackendTokenStats } from '../types';

interface TokenStats {
  inputTokens: number;
  outputTokens: number;
  totalTokens: number;
  totalCostUsd: number;
  sessionsToday: number;
  costByModel: Record<string, number>;
}

// Combined stats from all machines
export const tokenStats = writable<TokenStats>({
  inputTokens: 0,
  outputTokens: 0,
  totalTokens: 0,
  totalCostUsd: 0,
  sessionsToday: 0,
  costByModel: {},
});

// Real cost from backend (replaces the approximation)
export const estimatedCost = derived(tokenStats, ($stats) => $stats.totalCostUsd);

// Budget limit store (persisted in config.toml)
export const budgetLimit = writable<number | null>(null);

let budgetAlertFired = false;
let refreshInterval: ReturnType<typeof setInterval> | null = null;

/** Load budget limit from backend config */
export async function loadBudgetLimit(): Promise<void> {
  try {
    const limit = await getBudgetLimit();
    budgetLimit.set(limit);
  } catch (e) {
    console.warn('tokens: failed to load budget limit', e);
  }
}

/** Persist budget limit to backend config */
export async function saveBudgetLimit(limit: number | null): Promise<void> {
  budgetLimit.set(limit);
  budgetAlertFired = false;
  try {
    await apiSetBudgetLimit(limit);
  } catch (e) {
    console.warn('tokens: failed to save budget limit', e);
  }
}

export async function refreshTokenStats() {
  try {
    const stats: BackendTokenStats = await apiGetTokenStats();

    tokenStats.set({
      inputTokens: stats.tokensIn,
      outputTokens: stats.tokensOut,
      totalTokens: stats.tokensIn + stats.tokensOut,
      totalCostUsd: stats.totalCostUsd,
      sessionsToday: stats.sessionsToday,
      costByModel: stats.costByModel,
    });

    // Budget alert check
    const cost = stats.totalCostUsd;
    const limit = get(budgetLimit);
    if (limit && limit > 0 && cost > limit && !budgetAlertFired) {
      budgetAlertFired = true;
      sendSmartNotification(
        'taskError',
        'Presupuesto alcanzado',
        `Gasto actual: $${cost.toFixed(2)} / limite: $${limit.toFixed(2)}`,
        'error'
      );
    }
    // Reset alert fired if cost drops back below limit
    if (limit && cost <= limit) {
      budgetAlertFired = false;
    }
  } catch (e) {
    console.error('tokens:', e);
  }
}

export function startTokenTracking() {
  loadBudgetLimit();
  refreshTokenStats();
  if (refreshInterval) clearInterval(refreshInterval);
  refreshInterval = setInterval(refreshTokenStats, 60000);
}

export function stopTokenTracking() {
  if (refreshInterval) {
    clearInterval(refreshInterval);
    refreshInterval = null;
  }
}
