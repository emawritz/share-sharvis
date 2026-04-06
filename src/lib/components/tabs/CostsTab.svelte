<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { fetchTimeline, getJarvisConfig, getTokenStats, getDailyStats, getTopTools } from '../../api';
  import { handleError } from '../../utils';
  import { addToast } from '../../stores/notifications';
  import { t, tr } from '$lib/i18n';
  import type { TimelineResponse, TokenStats, DailyStat, ToolStat } from '../../types';
  import Skeleton from '$lib/components/Skeleton.svelte';
  import { exportAsCSV, exportAsJSON } from '$lib/utils/export';

  // Token pricing (USD per million tokens)
  const PRICE_INPUT_PER_MTOK = 3;
  const PRICE_OUTPUT_PER_MTOK = 15;

  interface MachineTokens {
    id: string;
    name: string;
    input: number;
    output: number;
    total: number;
    cost: number;
    timeline: TimelineResponse | null;
  }

  let machines = $state<MachineTokens[]>([]);
  let loading = $state(false);
  let sessionStart = $state(Date.now());
  let now = $state(Date.now());
  let nowTimer: ReturnType<typeof setInterval> | null = null;
  let dailyBudget = $state(parseFloat(localStorage.getItem('jarvis-daily-budget') || '5'));
  let editingBudget = $state(false);
  let budgetInput = $state(localStorage.getItem('jarvis-daily-budget') || '5');
  let budgetAlertFired = $state(false);

  let totalCost = $derived(machines.reduce((sum, m) => sum + m.cost, 0));
  let totalInput = $derived(machines.reduce((sum, m) => sum + m.input, 0));
  let totalOutput = $derived(machines.reduce((sum, m) => sum + m.output, 0));
  let totalTokens = $derived(machines.reduce((sum, m) => sum + m.total, 0));
  let maxCost = $derived(Math.max(...machines.map(m => m.cost), 0.01));

  let totalToolCalls = $derived(machines.reduce((sum, m) => {
    if (!m.timeline?.summary?.toolCalls) return sum;
    const tc = m.timeline.summary.toolCalls;
    return sum + Object.values(tc).reduce((a, b) => a + b, 0);
  }, 0));

  let totalErrors = $derived(machines.reduce((sum, m) => {
    return sum + (m.timeline?.summary?.errorCount || 0);
  }, 0));

  let totalFiles = $derived(machines.reduce((sum, m) => {
    return sum + (m.timeline?.summary?.filesTouched?.length || 0);
  }, 0));

  let totalDuration = $derived(machines.reduce((sum, m) => {
    return sum + (m.timeline?.summary?.duration || 0);
  }, 0));

  // Budget progress
  let budgetPct = $derived(dailyBudget > 0 ? Math.min((totalCost / dailyBudget) * 100, 100) : 0);
  let budgetColor = $derived(
    budgetPct < 50 ? 'var(--green)' :
    budgetPct < 80 ? 'var(--amber)' :
    'var(--red)'
  );

  // Burn rate: cost per hour based on session elapsed time
  let sessionElapsedHours = $derived(Math.max((now - sessionStart) / 3_600_000, 0.01));
  let costPerHour = $derived(totalCost / sessionElapsedHours);
  let projectedDailyCost = $derived(costPerHour * 24);
  let tokensPerMinute = $derived(totalTokens / Math.max((now - sessionStart) / 60_000, 1));

  // Session duration display
  let sessionDurationMs = $derived(now - sessionStart);

  // Budget alert at 80%
  $effect(() => {
    if (budgetPct >= 80 && !budgetAlertFired && totalCost > 0) {
      budgetAlertFired = true;
      addToast(t('costs.budgetAlert', { pct: budgetPct.toFixed(0) }) + ` (${fmtCost(totalCost)} / ${fmtCost(dailyBudget)})`, 'error');
    }
  });

  function fmtTokens(n: number): string {
    if (n >= 1_000_000) return (n / 1_000_000).toFixed(1) + 'M';
    if (n >= 1_000) return (n / 1_000).toFixed(1) + 'K';
    return String(n);
  }

  function fmtCost(n: number): string {
    return '$' + n.toFixed(2);
  }

  function fmtDuration(ms: number): string {
    if (ms <= 0) return '-';
    const secs = Math.round(ms / 1000);
    if (secs < 60) return secs + 's';
    const mins = Math.floor(secs / 60);
    const rem = secs % 60;
    if (mins < 60) return mins + 'm ' + rem + 's';
    const hrs = Math.floor(mins / 60);
    return hrs + 'h ' + (mins % 60) + 'm';
  }

  function machineColor(id: string): string {
    const palette = ['#7eb8ff', '#7effa0', '#ffb74d', '#c084fc', '#f48fb1', '#4fc3f7', '#ff8a65', '#aed581'];
    let hash = 0;
    for (let i = 0; i < id.length; i++) hash = ((hash << 5) - hash + id.charCodeAt(i)) | 0;
    return palette[Math.abs(hash) % palette.length];
  }

  function inputPct(input: number, output: number): number {
    const t = input + output;
    if (t === 0) return 50;
    return Math.round((input / t) * 100);
  }

  function saveBudget() {
    const val = parseFloat(budgetInput);
    if (!isNaN(val) && val > 0) {
      dailyBudget = val;
      localStorage.setItem('jarvis-daily-budget', String(val));
      budgetAlertFired = false;
      addToast(t('costs.budgetUpdated', { amount: fmtCost(val) }), 'success');
    }
    editingBudget = false;
  }

  function handleBudgetKey(e: KeyboardEvent) {
    if (e.key === 'Enter') saveBudget();
    if (e.key === 'Escape') editingBudget = false;
  }

  async function refresh() {
    loading = true;
    try {
      const [cfg, ts] = await Promise.all([getJarvisConfig(), getTokenStats().catch(() => null)]);
      tokenStats = ts;
      const enabled = cfg.machines.filter(m => m.enabled);
      const results = await Promise.allSettled(
        enabled.map(async (m) => {
          const tl = await fetchTimeline(m.id);
          const input = tl.summary.totalInputTokens;
          const output = tl.summary.totalOutputTokens;
          const cost = (input / 1_000_000) * PRICE_INPUT_PER_MTOK + (output / 1_000_000) * PRICE_OUTPUT_PER_MTOK;
          return { id: m.id, name: m.name, input, output, total: tl.summary.totalTokens, cost, timeline: tl };
        })
      );
      machines = results
        .filter(r => r.status === 'fulfilled')
        .map(r => (r as PromiseFulfilledResult<MachineTokens>).value);
      // Update sparkline with today's cost
      saveSparklineToday(totalCost);
      sparklineData = getSparklineData();
    } catch (e) {
      addToast('Error: ' + handleError(e), 'error');
    }
    loading = false;
  }

  let showExportMenu = $state(false);
  let tokenStats = $state<TokenStats | null>(null);

  // --- Model breakdown from getTokenStats ---
  interface ModelBreakdown {
    model: string;
    cost: number;
    pct: number;
  }
  let modelBreakdown = $derived<ModelBreakdown[]>((() => {
    if (!tokenStats) return [];
    const entries = Object.entries(tokenStats.costByModel);
    if (entries.length === 0) return [];
    const total = entries.reduce((s, [, v]) => s + v, 0);
    if (total === 0) return [];
    return entries
      .map(([model, cost]) => ({ model, cost, pct: Math.round((cost / total) * 100) }))
      .sort((a, b) => b.cost - a.cost);
  })());
  let maxModelCost = $derived(modelBreakdown.length > 0 ? Math.max(...modelBreakdown.map(m => m.cost)) : 0.01);

  // --- Daily sparkline: persist daily totals in localStorage ---
  const SPARKLINE_KEY = 'jarvis-daily-sparkline';
  interface DayEntry { date: string; cost: number; }

  function getSparklineData(): DayEntry[] {
    try {
      return JSON.parse(localStorage.getItem(SPARKLINE_KEY) || '[]');
    } catch { return []; }
  }

  function saveSparklineToday(cost: number) {
    const today = new Date().toISOString().slice(0, 10);
    let data = getSparklineData();
    // Remove old entries (keep last 7 days)
    const cutoff = new Date();
    cutoff.setDate(cutoff.getDate() - 6);
    const cutoffStr = cutoff.toISOString().slice(0, 10);
    data = data.filter(d => d.date >= cutoffStr);
    const existing = data.find(d => d.date === today);
    if (existing) { existing.cost = cost; } else { data.push({ date: today, cost }); }
    data.sort((a, b) => a.date.localeCompare(b.date));
    localStorage.setItem(SPARKLINE_KEY, JSON.stringify(data));
  }

  let sparklineData = $state<DayEntry[]>(getSparklineData());
  let sparklineMax = $derived(Math.max(...sparklineData.map(d => d.cost), 0.001));

  // --- Daily Stats from backend (bar chart) ---
  let dailyStatsDays = $state(30);
  let dailyStats = $state<DailyStat[]>([]);
  let dailyStatsLoading = $state(false);
  let dailyStatsMax = $derived(Math.max(...dailyStats.map(d => d.costUsd), 0.001));

  async function fetchDailyStats() {
    dailyStatsLoading = true;
    try {
      dailyStats = await getDailyStats(dailyStatsDays);
    } catch (e) {
      // silently ignore — backend may not have data
    }
    dailyStatsLoading = false;
  }

  $effect(() => {
    // re-fetch when days selector changes
    void fetchDailyStats();
  });

  // --- Top Tools from backend ---
  let topTools = $state<ToolStat[]>([]);
  let topToolsMax = $derived(Math.max(...topTools.map(t => t.calls), 1));

  async function fetchTopTools() {
    try {
      topTools = await getTopTools(10);
    } catch {
      // silently ignore
    }
  }

  // --- Budget alert inline ---
  let showBudgetBanner = $derived(budgetPct >= 80 && totalCost > 0);

  // --- Cost per task: cost / total tool calls ---
  let costPerTask = $derived(
    totalToolCalls > 0 ? totalCost / totalToolCalls : null
  );

  // --- Reset counter ---
  function resetDailyCounter() {
    const today = new Date().toISOString().slice(0, 10);
    let data = getSparklineData().filter(d => d.date !== today);
    localStorage.setItem(SPARKLINE_KEY, JSON.stringify(data));
    sparklineData = getSparklineData();
    budgetAlertFired = false;
    addToast('Contador diario reseteado', 'success');
  }

  function exportCostsCSV() {
    const rows = machines.map(m => ({
      machine: m.name,
      input_tokens: m.input,
      output_tokens: m.output,
      total_tokens: m.total,
      cost_usd: m.cost.toFixed(4),
    }));
    exportAsCSV(rows, 'jarvis-costs');
    showExportMenu = false;
  }

  function exportCostsJSON() {
    const data = {
      exported_at: new Date().toISOString(),
      totals: { input: totalInput, output: totalOutput, tokens: totalTokens, cost: totalCost },
      machines: machines.map(m => ({
        id: m.id, name: m.name, input: m.input, output: m.output, total: m.total, cost: m.cost,
      })),
    };
    exportAsJSON(data, 'jarvis-costs');
    showExportMenu = false;
  }

  onMount(() => {
    refresh();
    fetchTopTools();
    nowTimer = setInterval(() => { now = Date.now(); }, 60_000);
  });
  onDestroy(() => { if (nowTimer) clearInterval(nowTimer); });
</script>

<div class="costs-panel">
  <div class="costs-header">
    <div class="section-label">{$tr('costs.title')}</div>
    <div class="header-actions">
      <div class="export-wrapper">
        <button class="export-btn" onclick={() => showExportMenu = !showExportMenu} disabled={machines.length === 0}>
          {$tr('costs.export')}
        </button>
        {#if showExportMenu}
          <div class="export-dropdown">
            <button class="export-option" onclick={exportCostsCSV}>{$tr('costs.exportCSV')}</button>
            <button class="export-option" onclick={exportCostsJSON}>{$tr('costs.exportJSON')}</button>
          </div>
        {/if}
      </div>
      <button class="refresh-btn" onclick={refresh} disabled={loading}>
        {loading ? $tr('common.loading') : $tr('common.refresh')}
      </button>
    </div>
  </div>

  {#if loading && machines.length === 0}
    <Skeleton width="100%" height="80px" variant="card" />
    <Skeleton width="100%" height="60px" variant="card" />
    <div class="summary-grid">
      {#each [0, 1, 2, 3] as _}
        <div class="summary-card">
          <Skeleton width="50%" height="22px" />
          <Skeleton width="70%" height="10px" />
        </div>
      {/each}
    </div>
  {:else if machines.length === 0}
    <div class="empty-state">{$tr('costs.title')}</div>
  {:else}
    <!-- Budget Progress -->
    <div class="budget-card">
      <div class="budget-header">
        <div class="budget-title">
          <span class="budget-label">{$tr('costs.dailyBudget')}</span>
          {#if editingBudget}
            <div class="budget-editor">
              <span class="budget-dollar">$</span>
              <input
                class="jarvis-input budget-input"
                type="number"
                min="0.5"
                step="0.5"
                bind:value={budgetInput}
                onkeydown={handleBudgetKey}
                onblur={saveBudget}
              />
              <button class="jarvis-btn jarvis-btn-primary budget-save" onclick={saveBudget}>OK</button>
            </div>
          {:else}
            <button class="budget-edit-btn" onclick={() => { editingBudget = true; budgetInput = String(dailyBudget); }}>
              {fmtCost(dailyBudget)}
            </button>
          {/if}
        </div>
        <div class="budget-spend">
          <span class="budget-current" style="color:{budgetColor}">{fmtCost(totalCost)}</span>
          <span class="budget-sep">/</span>
          <span class="budget-max">{fmtCost(dailyBudget)}</span>
        </div>
      </div>
      <div class="budget-bar-track">
        <div
          class="budget-bar-fill"
          style="width:{budgetPct}%; background:{budgetColor}"
        ></div>
      </div>
      <div class="budget-footer">
        <span class="budget-pct" style="color:{budgetColor}">{budgetPct.toFixed(1)}% {$tr('costs.used')}</span>
        <span class="budget-remaining">{$tr('costs.remaining')} {fmtCost(Math.max(dailyBudget - totalCost, 0))}</span>
      </div>
    </div>

    <!-- Budget Alert Banner -->
    {#if showBudgetBanner}
      <div class="budget-alert-banner">
        <span class="budget-alert-icon">⚠</span>
        <span class="budget-alert-text">
          Gastaste el <strong>{budgetPct.toFixed(0)}%</strong> del presupuesto diario
          ({fmtCost(totalCost)} de {fmtCost(dailyBudget)})
        </span>
        <button class="reset-counter-btn" onclick={resetDailyCounter}>Resetear contador</button>
      </div>
    {/if}

    <!-- Burn Rate -->
    <div class="burn-rate-card">
      <div class="burn-rate-row">
        <div class="burn-stat">
          <span class="burn-val">{fmtCost(costPerHour)}</span>
          <span class="burn-label">{$tr('costs.perHour')}</span>
        </div>
        <div class="burn-stat">
          <span class="burn-val projection">{fmtCost(projectedDailyCost)}</span>
          <span class="burn-label">{$tr('costs.perDayProjected')}</span>
        </div>
        <div class="burn-stat">
          <span class="burn-val tpm">{tokensPerMinute.toFixed(0)}</span>
          <span class="burn-label">{$tr('costs.tokensPerMin')}</span>
        </div>
      </div>
      <div class="burn-projection-text">
        A este ritmo: ~{fmtCost(projectedDailyCost)}/dia
        {#if projectedDailyCost > dailyBudget}
          <span class="burn-warning"> -- {$tr('costs.exceedsBudget')}</span>
        {/if}
      </div>
    </div>

    <!-- Session Summary Card -->
    <div class="section-label">{$tr('costs.sessions')}</div>
    <div class="summary-grid">
      <div class="summary-card">
        <div class="summary-val">{fmtTokens(totalTokens)}</div>
        <div class="summary-label">{$tr('timeline.tokens')}</div>
      </div>
      <div class="summary-card highlight">
        <div class="summary-val cost">{fmtCost(totalCost)}</div>
        <div class="summary-label">{$tr('costs.totalCost')}</div>
      </div>
      <div class="summary-card">
        <div class="summary-val input">{fmtTokens(totalInput)}</div>
        <div class="summary-label">{$tr('costs.inputTokens')}</div>
      </div>
      <div class="summary-card">
        <div class="summary-val output">{fmtTokens(totalOutput)}</div>
        <div class="summary-label">{$tr('costs.outputTokens')}</div>
      </div>
    </div>

    <!-- Daily Sparkline -->
    {#if sparklineData.length > 0}
      <div class="section-label">Gasto últimos 7 días</div>
      <div class="sparkline-card">
        <div class="sparkline-bars">
          {#each sparklineData as day}
            {@const heightPct = Math.max(4, Math.round((day.cost / sparklineMax) * 100))}
            {@const isToday = day.date === new Date().toISOString().slice(0, 10)}
            <div class="sparkline-col">
              <div class="sparkline-bar-wrap">
                <div
                  class="sparkline-bar"
                  class:sparkline-today={isToday}
                  style="height:{heightPct}%"
                  title="{day.date}: {fmtCost(day.cost)}"
                ></div>
              </div>
              <span class="sparkline-label">{day.date.slice(5)}</span>
            </div>
          {/each}
        </div>
        <div class="sparkline-meta">
          <span class="sparkline-max">Max: {fmtCost(sparklineMax)}</span>
          {#if sparklineData.length >= 2}
            {@const avg = sparklineData.reduce((s, d) => s + d.cost, 0) / sparklineData.length}
            <span class="sparkline-avg">Promedio: {fmtCost(avg)}/día</span>
          {/if}
        </div>
      </div>
    {/if}

    <!-- Model Breakdown -->
    {#if modelBreakdown.length > 0}
      <div class="section-label">Costo por modelo</div>
      <div class="model-bars">
        {#each modelBreakdown as m}
          <div class="model-bar-row">
            <span class="model-bar-label" title={m.model}>{m.model.length > 22 ? m.model.slice(-22) : m.model}</span>
            <div class="model-bar-track">
              <div
                class="model-bar-fill"
                style="width:{Math.max(2, (m.cost / maxModelCost) * 100)}%"
              ></div>
            </div>
            <span class="model-bar-value">{fmtCost(m.cost)}</span>
            <span class="model-bar-pct">{m.pct}%</span>
          </div>
        {/each}
      </div>
    {/if}

    <!-- Daily Costs Bar Chart (from backend) -->
    <div class="section-label daily-range-header">
      <span>Costo diario</span>
      <div class="range-tabs">
        {#each [7, 30, 90] as d}
          <button
            class="range-tab"
            class:range-tab-active={dailyStatsDays === d}
            onclick={() => { dailyStatsDays = d; }}
          >{d}d</button>
        {/each}
      </div>
    </div>
    {#if dailyStatsLoading}
      <div class="daily-loading">Cargando...</div>
    {:else if dailyStats.length > 0}
      {@const nonZero = dailyStats.filter(d => d.costUsd > 0)}
      {@const totalDaily = dailyStats.reduce((s, d) => s + d.costUsd, 0)}
      {@const totalDailyTokens = dailyStats.reduce((s, d) => s + d.tokens, 0)}
      <!-- Summary cards from daily stats -->
      <div class="daily-summary-cards">
        <div class="daily-summary-card">
          <div class="daily-summary-val">{fmtCost(totalDaily)}</div>
          <div class="daily-summary-label">Total {dailyStatsDays}d</div>
        </div>
        <div class="daily-summary-card">
          <div class="daily-summary-val">{fmtTokens(totalDailyTokens)}</div>
          <div class="daily-summary-label">Tokens {dailyStatsDays}d</div>
        </div>
        <div class="daily-summary-card">
          <div class="daily-summary-val">{nonZero.length > 0 ? fmtCost(totalDaily / nonZero.length) : '$0.00'}</div>
          <div class="daily-summary-label">Promedio/día activo</div>
        </div>
      </div>
      <div class="daily-chart-card">
        <div class="daily-chart-bars">
          {#each dailyStats as day}
            {@const hPct = day.costUsd > 0 ? Math.max(3, (day.costUsd / dailyStatsMax) * 100) : 0}
            {@const isToday = day.date === new Date().toISOString().slice(0, 10)}
            <div class="daily-chart-col" title="{day.date}: {fmtCost(day.costUsd)} / {fmtTokens(day.tokens)} tokens / {day.events} events">
              <div class="daily-chart-bar-wrap">
                <div
                  class="daily-chart-bar"
                  class:daily-chart-bar-today={isToday}
                  style="height:{hPct}%"
                ></div>
              </div>
              {#if dailyStatsDays <= 14}
                <span class="daily-chart-label">{day.date.slice(5)}</span>
              {/if}
            </div>
          {/each}
        </div>
        <div class="daily-chart-meta">
          <span class="daily-chart-max">Max: {fmtCost(dailyStatsMax)}</span>
          <span class="daily-chart-days">{dailyStatsDays} días</span>
        </div>
      </div>
    {/if}

    <!-- Top Tools Section -->
    {#if topTools.length > 0}
      <div class="section-label">Top Tools</div>
      <div class="top-tools-bars">
        {#each topTools as tool}
          <div class="top-tool-row">
            <span class="top-tool-name" title={tool.toolName}>{tool.toolName.length > 18 ? tool.toolName.slice(0, 18) + '…' : tool.toolName}</span>
            <div class="top-tool-track">
              <div
                class="top-tool-fill"
                style="width:{Math.max(2, (tool.calls / topToolsMax) * 100)}%"
              ></div>
            </div>
            <span class="top-tool-count">{tool.calls.toLocaleString()}</span>
          </div>
        {/each}
      </div>
    {/if}

    <!-- Cost per task + Reset -->
    <div class="meta-row">
      {#if costPerTask !== null}
        <div class="meta-stat">
          <span class="meta-val">{fmtCost(costPerTask)}</span>
          <span class="meta-label">Costo promedio por tool call</span>
        </div>
      {/if}
      {#if !showBudgetBanner}
        <button class="reset-counter-btn-subtle" onclick={resetDailyCounter}>Resetear contador</button>
      {/if}
    </div>

    <!-- Per-Machine Breakdown -->
    <div class="section-label">{$tr('costs.perMachine')}</div>
    <div class="machine-grid">
      {#each machines as m}
        <div class="machine-cost-card">
          <div class="mcc-header">
            <span class="mcc-name" style="color:{machineColor(m.id)}">{m.name}</span>
            <span class="mcc-cost">{fmtCost(m.cost)}</span>
          </div>
          <div class="mcc-stats">
            <div class="mcc-stat">
              <span class="mcc-stat-label">Input</span>
              <span class="mcc-stat-val input">{fmtTokens(m.input)}</span>
            </div>
            <div class="mcc-stat">
              <span class="mcc-stat-label">Output</span>
              <span class="mcc-stat-val output">{fmtTokens(m.output)}</span>
            </div>
            <div class="mcc-stat">
              <span class="mcc-stat-label">Total</span>
              <span class="mcc-stat-val">{fmtTokens(m.total)}</span>
            </div>
          </div>
          <!-- Cost share bar -->
          <div class="mcc-share-row">
            <div class="mcc-share-track">
              <div
                class="mcc-share-fill"
                style="width:{Math.max(2, (m.cost / maxCost) * 100)}%; background:{machineColor(m.id)}"
              ></div>
            </div>
            <span class="mcc-share-pct">{totalCost > 0 ? ((m.cost / totalCost) * 100).toFixed(0) : 0}%</span>
          </div>
          <!-- Mini token split bar -->
          <div class="token-split-mini">
            <div class="split-input" style="width:{inputPct(m.input, m.output)}%"></div>
            <div class="split-output" style="width:{100 - inputPct(m.input, m.output)}%"></div>
          </div>
        </div>
      {/each}
    </div>

    <!-- Cost Breakdown Bar Chart -->
    <div class="section-label">{$tr('costs.totalCost')}</div>
    <div class="cost-bars">
      {#each machines as m}
        <div class="cost-bar-row">
          <span class="cost-bar-label" style="color:{machineColor(m.id)}">{m.name}</span>
          <div class="cost-bar-track">
            <div
              class="cost-bar-fill"
              style="width:{Math.max(2, (m.cost / maxCost) * 100)}%; background:{machineColor(m.id)}"
            ></div>
          </div>
          <span class="cost-bar-value">{fmtCost(m.cost)}</span>
        </div>
      {/each}
    </div>

    <!-- Token Distribution -->
    <div class="section-label">{$tr('costs.inputTokens')} / {$tr('costs.outputTokens')}</div>
    <div class="token-split-section">
      <div class="token-split-bar">
        <div class="split-input" style="width:{inputPct(totalInput, totalOutput)}%">
          <span class="split-label">{inputPct(totalInput, totalOutput)}% Input</span>
        </div>
        <div class="split-output" style="width:{100 - inputPct(totalInput, totalOutput)}%">
          <span class="split-label">{100 - inputPct(totalInput, totalOutput)}% Output</span>
        </div>
      </div>
      <div class="split-legend">
        <span class="split-legend-item"><span class="dot input"></span> Input ({fmtTokens(totalInput)}) - ${PRICE_INPUT_PER_MTOK}/M</span>
        <span class="split-legend-item"><span class="dot output"></span> Output ({fmtTokens(totalOutput)}) - ${PRICE_OUTPUT_PER_MTOK}/M</span>
      </div>
    </div>

    <!-- Session Stats -->
    <div class="section-label">{$tr('costs.sessions')}</div>
    <div class="session-stats-grid">
      <div class="session-stat">
        <div class="session-stat-val">{fmtDuration(totalDuration)}</div>
        <div class="session-stat-label">{$tr('timeline.duration')}</div>
      </div>
      <div class="session-stat">
        <div class="session-stat-val">{totalToolCalls.toLocaleString()}</div>
        <div class="session-stat-label">{$tr('costs.toolCalls')}</div>
      </div>
      <div class="session-stat">
        <div class="session-stat-val" class:has-errors={totalErrors > 0}>{totalErrors}</div>
        <div class="session-stat-label">{$tr('timeline.errors')}</div>
      </div>
      <div class="session-stat">
        <div class="session-stat-val">{totalFiles}</div>
        <div class="session-stat-label">{$tr('timeline.files')}</div>
      </div>
    </div>
  {/if}
</div>

<style>
  .costs-panel {
    padding: 8px 14px;
    overflow-y: auto;
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .costs-header {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .section-label {
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 700;
    color: var(--text-2);
    text-transform: uppercase;
    letter-spacing: 2px;
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .section-label::before {
    content: '';
    width: 3px; height: 3px;
    border-radius: 50%;
    background: var(--text-2);
  }

  .header-actions {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-left: auto;
  }
  .export-wrapper {
    position: relative;
  }
  .export-btn {
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 600;
    letter-spacing: 0.5px;
    text-transform: uppercase;
    color: var(--text-2);
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 4px 10px;
    cursor: pointer;
    transition: background 0.15s ease, border-color 0.15s ease, color 0.15s ease;
  }
  .export-btn:hover { background: var(--bg-3); border-color: var(--border-bright); color: var(--text-1); }
  .export-btn:disabled { opacity: 0.5; cursor: default; }
  .export-dropdown {
    position: absolute;
    top: calc(100% + 4px);
    right: 0;
    background: var(--bg-1);
    border: 1px solid var(--border-bright);
    border-radius: 4px;
    display: flex;
    flex-direction: column;
    z-index: 10;
    min-width: 70px;
    box-shadow: 0 4px 12px rgba(0,0,0,0.3);
  }
  .export-option {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--text-1);
    background: none;
    border: none;
    padding: 6px 12px;
    cursor: pointer;
    text-align: left;
    transition: background 0.1s ease;
  }
  .export-option:hover { background: var(--bg-3); }
  .export-option:first-child { border-radius: 4px 4px 0 0; }
  .export-option:last-child { border-radius: 0 0 4px 4px; }

  .refresh-btn {
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 600;
    letter-spacing: 0.5px;
    text-transform: uppercase;
    color: var(--cyan);
    background: #00d4ff11;
    border: 1px solid #00d4ff33;
    border-radius: 4px;
    padding: 4px 10px;
    cursor: pointer;
    transition: background 0.15s ease, border-color 0.15s ease;
  }
  .refresh-btn:hover { background: #00d4ff22; border-color: #00d4ff55; }
  .refresh-btn:disabled { opacity: 0.5; cursor: default; }

  /* Budget Card */
  .budget-card {
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .budget-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .budget-title {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .budget-label {
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 700;
    color: var(--text-2);
    text-transform: uppercase;
    letter-spacing: 1.5px;
  }
  .budget-edit-btn {
    background: none;
    border: 1px dashed var(--border-bright);
    border-radius: 4px;
    color: var(--text-1);
    font-family: var(--font-mono);
    font-size: 11px;
    padding: 2px 8px;
    cursor: pointer;
    transition: border-color 0.15s, color 0.15s;
  }
  .budget-edit-btn:hover {
    border-color: var(--cyan);
    color: var(--cyan);
  }
  .budget-editor {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .budget-dollar {
    color: var(--text-2);
    font-size: 11px;
    font-weight: 600;
  }
  .budget-input {
    width: 60px;
    padding: 2px 6px;
    font-size: 11px;
  }
  .budget-save {
    padding: 2px 8px;
    font-size: 9px;
  }
  .budget-spend {
    display: flex;
    align-items: baseline;
    gap: 4px;
  }
  .budget-current {
    font-family: var(--font-display);
    font-size: 18px;
    font-weight: 700;
  }
  .budget-sep {
    color: var(--text-3);
    font-size: 12px;
  }
  .budget-max {
    font-family: var(--font-display);
    font-size: 12px;
    color: var(--text-2);
  }
  .budget-bar-track {
    height: 8px;
    background: var(--bg-1);
    border-radius: 4px;
    overflow: hidden;
  }
  .budget-bar-fill {
    height: 100%;
    border-radius: 4px;
    transition: width 0.4s ease, background 0.3s ease;
  }
  .budget-footer {
    display: flex;
    justify-content: space-between;
    font-size: 9px;
  }
  .budget-pct {
    font-family: var(--font-display);
    font-weight: 700;
    letter-spacing: 0.5px;
  }
  .budget-remaining {
    color: var(--text-2);
    font-family: var(--font-mono);
  }

  /* Burn Rate Card */
  .burn-rate-card {
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 10px 14px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .burn-rate-row {
    display: flex;
    gap: 16px;
  }
  .burn-stat {
    display: flex;
    align-items: baseline;
    gap: 4px;
  }
  .burn-val {
    font-family: var(--font-display);
    font-size: 16px;
    font-weight: 700;
    color: var(--cyan);
  }
  .burn-val.projection {
    color: var(--amber);
  }
  .burn-val.tpm {
    color: var(--text-1);
    font-size: 14px;
  }
  .burn-label {
    font-size: 9px;
    color: var(--text-2);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }
  .burn-projection-text {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--text-2);
    font-style: italic;
  }
  .burn-warning {
    color: var(--red);
    font-weight: 600;
    font-style: normal;
  }

  /* Summary Cards */
  .summary-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(100px, 1fr));
    gap: 6px;
  }
  .summary-card {
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 10px 12px;
    text-align: center;
  }
  .summary-card.highlight {
    border-color: #00ff8833;
    background: linear-gradient(180deg, var(--bg-2) 0%, #00ff8808 100%);
  }
  .summary-val {
    font-family: var(--font-display);
    font-size: 20px;
    font-weight: 700;
    color: var(--cyan);
  }
  .summary-val.cost { color: var(--green); }
  .summary-val.input { color: #22d3ee; }
  .summary-val.output { color: var(--amber); }
  .summary-label {
    font-size: 9px;
    color: var(--text-2);
    text-transform: uppercase;
    letter-spacing: 1px;
    margin-top: 2px;
  }

  /* Per-Machine Cards */
  .machine-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
    gap: 8px;
  }
  .machine-cost-card {
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    transition: border-color 0.2s ease;
  }
  .machine-cost-card:hover { border-color: var(--border-bright); }
  .mcc-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .mcc-name {
    font-family: var(--font-display);
    font-size: 12px;
    font-weight: 700;
    letter-spacing: 2px;
    text-transform: uppercase;
  }
  .mcc-cost {
    font-family: var(--font-display);
    font-size: 14px;
    font-weight: 700;
    color: var(--green);
  }
  .mcc-stats {
    display: flex;
    gap: 8px;
  }
  .mcc-stat {
    display: flex;
    flex-direction: column;
    gap: 1px;
    flex: 1;
  }
  .mcc-stat-label {
    font-size: 8px;
    color: var(--text-3);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    font-weight: 600;
  }
  .mcc-stat-val {
    font-family: var(--font-display);
    font-size: 12px;
    font-weight: 700;
    color: var(--text-1);
    font-variant-numeric: tabular-nums;
  }
  .mcc-stat-val.input { color: #22d3ee; }
  .mcc-stat-val.output { color: var(--amber); }

  /* Cost share row in machine card */
  .mcc-share-row {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .mcc-share-track {
    flex: 1;
    height: 4px;
    background: var(--bg-1);
    border-radius: 2px;
    overflow: hidden;
  }
  .mcc-share-fill {
    height: 100%;
    border-radius: 2px;
    opacity: 0.7;
    transition: width 0.3s ease;
  }
  .mcc-share-pct {
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 700;
    color: var(--text-2);
    min-width: 24px;
    text-align: right;
  }

  /* Mini token split in machine cards */
  .token-split-mini {
    display: flex;
    height: 3px;
    border-radius: 2px;
    overflow: hidden;
  }

  /* Cost Breakdown Bars */
  .cost-bars {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .cost-bar-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .cost-bar-label {
    font-family: var(--font-display);
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 1px;
    text-transform: uppercase;
    width: 60px;
    flex-shrink: 0;
    text-align: right;
  }
  .cost-bar-track {
    flex: 1;
    height: 16px;
    background: var(--bg-1);
    border-radius: 3px;
    overflow: hidden;
  }
  .cost-bar-fill {
    height: 100%;
    border-radius: 3px;
    opacity: 0.75;
    transition: width 0.3s ease;
  }
  .cost-bar-value {
    font-family: var(--font-display);
    font-size: 10px;
    font-weight: 600;
    color: var(--text-1);
    width: 50px;
    flex-shrink: 0;
    font-variant-numeric: tabular-nums;
  }

  /* Token Distribution Split Bar */
  .token-split-section {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .token-split-bar {
    display: flex;
    height: 24px;
    border-radius: 4px;
    overflow: hidden;
  }
  .split-input {
    background: #22d3ee;
    display: flex;
    align-items: center;
    justify-content: center;
    min-width: 2px;
    transition: width 0.3s ease;
  }
  .split-output {
    background: var(--amber);
    display: flex;
    align-items: center;
    justify-content: center;
    min-width: 2px;
    transition: width 0.3s ease;
  }
  .split-label {
    font-family: var(--font-display);
    font-size: 8px;
    font-weight: 700;
    color: var(--bg-0);
    letter-spacing: 0.5px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    padding: 0 4px;
  }
  .split-legend {
    display: flex;
    gap: 16px;
    justify-content: center;
  }
  .split-legend-item {
    font-size: 9px;
    color: var(--text-2);
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .dot {
    width: 6px; height: 6px;
    border-radius: 50%;
  }
  .dot.input { background: #22d3ee; }
  .dot.output { background: var(--amber); }

  /* Session Stats */
  .session-stats-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(100px, 1fr));
    gap: 6px;
  }
  .session-stat {
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 8px 10px;
    text-align: center;
  }
  .session-stat-val {
    font-family: var(--font-display);
    font-size: 16px;
    font-weight: 700;
    color: var(--text-1);
    font-variant-numeric: tabular-nums;
  }
  .session-stat-val.has-errors { color: var(--red); }
  .session-stat-label {
    font-size: 9px;
    color: var(--text-2);
    text-transform: uppercase;
    letter-spacing: 1px;
    margin-top: 2px;
  }

  .empty-state { color: var(--text-3); font-size: 11px; font-style: italic; }

  /* Budget Alert Banner */
  .budget-alert-banner {
    display: flex;
    align-items: center;
    gap: 8px;
    background: #ff444422;
    border: 1px solid #ff444466;
    border-radius: 6px;
    padding: 8px 12px;
    font-size: 11px;
    color: var(--text-1);
  }
  .budget-alert-icon {
    font-size: 14px;
    flex-shrink: 0;
  }
  .budget-alert-text {
    flex: 1;
    color: #ffaaaa;
    line-height: 1.4;
  }
  .budget-alert-text strong {
    color: var(--red);
  }
  .reset-counter-btn {
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--red);
    background: #ff444422;
    border: 1px solid #ff444466;
    border-radius: 4px;
    padding: 4px 10px;
    cursor: pointer;
    flex-shrink: 0;
    transition: background 0.15s ease, border-color 0.15s ease;
  }
  .reset-counter-btn:hover {
    background: #ff444444;
    border-color: var(--red);
  }

  /* Sparkline */
  .sparkline-card {
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 10px 14px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .sparkline-bars {
    display: flex;
    gap: 6px;
    align-items: flex-end;
    height: 60px;
  }
  .sparkline-col {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    flex: 1;
  }
  .sparkline-bar-wrap {
    flex: 1;
    width: 100%;
    display: flex;
    align-items: flex-end;
  }
  .sparkline-bar {
    width: 100%;
    background: var(--cyan);
    opacity: 0.5;
    border-radius: 2px 2px 0 0;
    min-height: 3px;
    transition: height 0.3s ease;
  }
  .sparkline-bar.sparkline-today {
    opacity: 1;
    background: var(--cyan);
    box-shadow: 0 0 6px #00d4ff55;
  }
  .sparkline-label {
    font-family: var(--font-mono);
    font-size: 8px;
    color: var(--text-3);
    white-space: nowrap;
  }
  .sparkline-meta {
    display: flex;
    justify-content: space-between;
    font-family: var(--font-mono);
    font-size: 9px;
    color: var(--text-3);
  }
  .sparkline-avg {
    color: var(--cyan);
    opacity: 0.7;
  }

  /* Model Breakdown Bars */
  .model-bars {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }
  .model-bar-row {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .model-bar-label {
    font-family: var(--font-mono);
    font-size: 9px;
    color: var(--text-2);
    width: 130px;
    flex-shrink: 0;
    text-align: right;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .model-bar-track {
    flex: 1;
    height: 6px;
    background: var(--bg-1);
    border-radius: 3px;
    overflow: hidden;
  }
  .model-bar-fill {
    height: 100%;
    border-radius: 3px;
    background: var(--cyan);
    opacity: 0.75;
    transition: width 0.3s ease;
  }
  .model-bar-value {
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 600;
    color: var(--text-1);
    width: 40px;
    flex-shrink: 0;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .model-bar-pct {
    font-family: var(--font-display);
    font-size: 9px;
    color: var(--text-3);
    width: 30px;
    flex-shrink: 0;
    text-align: right;
  }

  /* Meta row: cost per task + reset */
  .meta-row {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .meta-stat {
    display: flex;
    align-items: baseline;
    gap: 6px;
    flex: 1;
  }
  .meta-val {
    font-family: var(--font-display);
    font-size: 14px;
    font-weight: 700;
    color: var(--amber);
  }
  .meta-label {
    font-size: 9px;
    color: var(--text-2);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }
  .reset-counter-btn-subtle {
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--text-3);
    background: none;
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 3px 8px;
    cursor: pointer;
    flex-shrink: 0;
    transition: color 0.15s ease, border-color 0.15s ease;
  }
  .reset-counter-btn-subtle:hover {
    color: var(--text-1);
    border-color: var(--border-bright);
  }

  /* Daily range header */
  .daily-range-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
  }
  .range-tabs {
    display: flex;
    gap: 2px;
    margin-left: auto;
  }
  .range-tab {
    font-family: var(--font-display);
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.5px;
    text-transform: uppercase;
    color: var(--text-3);
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 3px;
    padding: 2px 6px;
    cursor: pointer;
    transition: background 0.12s, color 0.12s, border-color 0.12s;
  }
  .range-tab:hover { color: var(--text-1); border-color: var(--border-bright); }
  .range-tab-active {
    color: var(--cyan);
    background: #00d4ff15;
    border-color: #00d4ff44;
  }
  .daily-loading {
    font-size: 10px;
    color: var(--text-3);
    font-style: italic;
  }

  /* Daily summary cards */
  .daily-summary-cards {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 6px;
  }
  .daily-summary-card {
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 8px 10px;
    text-align: center;
  }
  .daily-summary-val {
    font-family: var(--font-display);
    font-size: 16px;
    font-weight: 700;
    color: var(--cyan);
  }
  .daily-summary-label {
    font-size: 8px;
    color: var(--text-2);
    text-transform: uppercase;
    letter-spacing: 1px;
    margin-top: 2px;
  }

  /* Daily chart */
  .daily-chart-card {
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 10px 14px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .daily-chart-bars {
    display: flex;
    gap: 2px;
    align-items: flex-end;
    height: 70px;
  }
  .daily-chart-col {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
    flex: 1;
    cursor: default;
  }
  .daily-chart-bar-wrap {
    flex: 1;
    width: 100%;
    display: flex;
    align-items: flex-end;
  }
  .daily-chart-bar {
    width: 100%;
    background: var(--cyan);
    opacity: 0.45;
    border-radius: 2px 2px 0 0;
    min-height: 2px;
    transition: height 0.3s ease, opacity 0.15s ease;
  }
  .daily-chart-bar:hover { opacity: 0.8; }
  .daily-chart-bar-today {
    opacity: 0.9;
    box-shadow: 0 0 6px #00d4ff55;
  }
  .daily-chart-label {
    font-family: var(--font-mono);
    font-size: 7px;
    color: var(--text-3);
    white-space: nowrap;
  }
  .daily-chart-meta {
    display: flex;
    justify-content: space-between;
    font-family: var(--font-mono);
    font-size: 9px;
    color: var(--text-3);
  }
  .daily-chart-days { color: var(--text-3); }

  /* Top Tools bars */
  .top-tools-bars {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }
  .top-tool-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .top-tool-name {
    font-family: var(--font-mono);
    font-size: 9px;
    color: var(--text-2);
    width: 110px;
    flex-shrink: 0;
    text-align: right;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .top-tool-track {
    flex: 1;
    height: 8px;
    background: var(--bg-1);
    border-radius: 3px;
    overflow: hidden;
  }
  .top-tool-fill {
    height: 100%;
    border-radius: 3px;
    background: var(--amber);
    opacity: 0.75;
    transition: width 0.3s ease;
  }
  .top-tool-count {
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 600;
    color: var(--text-1);
    width: 50px;
    flex-shrink: 0;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
</style>
