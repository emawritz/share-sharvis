<script lang="ts">
  import { fetchPipelines, runPipeline, stopPipeline } from '../../api';
  import { addToast } from '../../stores/notifications';
  import { handleError } from '../../utils';
  import type { BuiltinInfo, PipelineState, PipelineStepState } from '../../types';
  import { t, tr } from '$lib/i18n';
  import { onMount } from 'svelte';

  let builtins = $state<BuiltinInfo[]>([]);
  let running = $state<PipelineState[]>([]);
  let refreshTimer: ReturnType<typeof setInterval> | null = null;

  async function refresh() {
    try {
      const data = await fetchPipelines();
      builtins = data.builtins || [];
      running = data.pipelines || [];
    } catch (e) {
      addToast(t('pipelines.errorLoading') + ': ' + handleError(e), 'error');
    }
  }

  async function handleRun(name: string) {
    try {
      const pipelineId = await runPipeline(name);
      addToast(t('pipelines.started', { name, id: pipelineId.substring(0, 8) }), 'success');
    } catch (e) {
      addToast('Error: ' + handleError(e), 'error');
    }
  }

  async function handleStop(pipeline: PipelineState) {
    const confirmed = window.confirm(t('pipelines.stopConfirm', { name: pipeline.name }));
    if (!confirmed) return;
    try {
      await stopPipeline(pipeline.id);
      addToast(t('pipelines.stopSuccess', { name: pipeline.name }), 'success');
      await refresh();
    } catch (e) {
      addToast(t('pipelines.stopError') + ': ' + handleError(e), 'error');
    }
  }

  function getRunningState(name: string): PipelineState | undefined {
    return running.find((r) => r.name === name && (r.status === 'running' || r.status === 'cancelling'));
  }

  function stepStatusClass(status: string): string {
    switch (status) {
      case 'completed': return 'step-done';
      case 'running': return 'step-running';
      case 'failed': return 'step-failed';
      case 'cancelled': return 'step-cancelled';
      case 'skipped': return 'step-skipped';
      default: return 'step-pending';
    }
  }

  function stepIcon(status: string): string {
    switch (status) {
      case 'completed': return '✓';
      case 'running': return '▶';
      case 'failed': return '✗';
      case 'cancelled': return '○';
      case 'skipped': return '–';
      default: return '○';
    }
  }

  function calcProgress(state: PipelineState): { done: number; total: number; pct: number } {
    const total = state.steps.length;
    const done = state.steps.filter((s) => s.status === 'completed' || s.status === 'skipped').length;
    return { done, total, pct: total > 0 ? Math.round((done / total) * 100) : 0 };
  }

  function formatDate(iso?: string): string {
    if (!iso) return '';
    try {
      const d = new Date(iso);
      return d.toLocaleString(undefined, { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' });
    } catch {
      return iso;
    }
  }

  function calcDuration(state: PipelineState): string {
    if (!state.startedAt) return '';
    const end = state.finishedAt ? new Date(state.finishedAt) : new Date();
    const start = new Date(state.startedAt);
    const secs = Math.round((end.getTime() - start.getTime()) / 1000);
    if (secs < 60) return `${secs}s`;
    return `${Math.floor(secs / 60)}m${secs % 60}s`;
  }

  function overallStatusClass(status: string): string {
    switch (status) {
      case 'completed': return 'status-completed';
      case 'failed': return 'status-failed';
      case 'running': return 'status-running';
      case 'cancelling': return 'status-running';
      case 'stopped': case 'cancelled': return 'status-stopped';
      default: return 'status-pending';
    }
  }

  // Group finished runs per pipeline name (last 3)
  function recentRuns(name: string): PipelineState[] {
    return running
      .filter((r) => r.name === name && r.finishedAt)
      .sort((a, b) => (b.finishedAt! > a.finishedAt! ? 1 : -1))
      .slice(0, 3);
  }

  onMount(() => {
    refresh();
    refreshTimer = setInterval(refresh, 3000);
    return () => {
      if (refreshTimer) { clearInterval(refreshTimer); refreshTimer = null; }
    };
  });
</script>

<div class="pipelines-list">
  <!-- Action types reference -->
  <div class="reference-card">
    <div class="reference-title">Step Action Types</div>
    <div class="reference-grid">
      <div class="action-chip">task <span class="action-desc">default — send prompt to agent</span></div>
      <div class="action-chip">open_pr <span class="action-desc">create GitHub PR (repo=, title=, body= in prompt)</span></div>
      <div class="action-chip">merge_when_green <span class="action-desc">wait for CI checks then merge (target = repo slug)</span></div>
    </div>
    <div class="condition-hint">
      <span class="hint-label">Conditions:</span>
      Simple: <code>FAIL</code>, <code>!FAIL</code>, <code>contains:X</code>, <code>!contains:X</code> —
      Compound: <code>{"{"}"and": ["FAIL", "contains:error"]{"}"}</code> · <code>{"{"}"or": ["FAIL", "contains:warn"]{"}"}</code> · <code>{"{"}"not": "FAIL"{"}"}</code>
    </div>
  </div>

  {#each builtins as pipeline}
    {@const runState = getRunningState(pipeline.name)}
    {@const prog = runState ? calcProgress(runState) : null}
    {@const history = recentRuns(pipeline.name)}

    <div class="pipeline-card" class:is-running={!!runState}>
      <!-- Header row -->
      <div class="pipeline-header">
        <div class="pipeline-info">
          <div class="pipeline-name">{pipeline.name}</div>
          {#if pipeline.description}
            <div class="pipeline-desc">{pipeline.description}</div>
          {/if}
          {#if !runState}
            <div class="pipeline-steps-count">{pipeline.steps} {$tr('pipelines.steps')}</div>
          {/if}
        </div>
        <div class="pipeline-actions">
          {#if runState}
            <span class="pipeline-status {overallStatusClass(runState.status)}">
              {runState.status === 'cancelling' ? 'stopping…' : $tr('pipelines.running')}
            </span>
            {#if runState.status === 'running'}
              <button class="pipeline-stop-btn" type="button" onclick={() => handleStop(runState!)}>
                {$tr('pipelines.stop')}
              </button>
            {/if}
          {:else}
            <button class="pipeline-run-btn" type="button" onclick={() => handleRun(pipeline.name)}>
              {$tr('pipelines.run')}
            </button>
          {/if}
        </div>
      </div>

      <!-- Running: step train + progress bar -->
      {#if runState}
        <!-- Progress bar -->
        {#if prog}
          <div class="progress-section">
            <div class="progress-label">
              <span>{$tr('pipelines.progress')}</span>
              <span class="progress-fraction">{prog.done}/{prog.total} pasos · {prog.pct}%</span>
            </div>
            <div class="progress-track">
              <div class="progress-fill" style="width: {prog.pct}%"></div>
            </div>
          </div>
        {/if}

        <!-- Step train -->
        <div class="step-train">
          {#each runState.steps as step, idx}
            {#if idx > 0}
              <div class="step-connector" class:connector-done={runState.steps[idx - 1].status === 'completed' || runState.steps[idx - 1].status === 'skipped'}></div>
            {/if}
            <div class="step-node {stepStatusClass(step.status)}" title="{step.name}">
              <div class="step-icon" class:pulse={step.status === 'running'}>{stepIcon(step.status)}</div>
              <div class="step-label">{step.name}</div>
            </div>
          {/each}
        </div>
      {/if}

      <!-- Recent run history (finished runs) -->
      {#if history.length > 0}
        <div class="run-history">
          {#each history as run}
            <div class="run-row">
              <span class="run-icon {overallStatusClass(run.status)}">
                {#if run.status === 'completed'}✓{:else if run.status === 'failed'}✗{:else}●{/if}
              </span>
              <span class="run-date">{formatDate(run.startedAt)}</span>
              <span class="run-duration">{calcDuration(run)}</span>
              <span class="run-status-text {overallStatusClass(run.status)}">{run.status}</span>
            </div>
          {/each}
        </div>
      {/if}
    </div>
  {/each}

  {#if builtins.length === 0}
    <div class="empty-state">
      <span class="empty-icon">&#x25A1;</span>
      <div class="empty-title">{$tr('pipelines.none')}</div>
      <div class="empty-hint">Add pipelines to ~/.config/jarvis/config.toml</div>
    </div>
  {/if}
</div>

<style>
  .pipelines-list {
    padding: 8px 14px;
    overflow-y: auto;
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  /* Pipeline card */
  .pipeline-card {
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 10px 12px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    transition: border-color 0.15s ease;
  }
  .pipeline-card:hover { border-color: var(--border-bright); }
  .pipeline-card.is-running { border-color: #00bbff44; }

  /* Header row */
  .pipeline-header {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .pipeline-info { flex: 1; min-width: 0; }
  .pipeline-name {
    font-family: var(--font-display);
    font-size: 11px;
    font-weight: 700;
    color: var(--text-0);
    letter-spacing: 0.5px;
  }
  .pipeline-desc {
    font-size: 10px;
    color: var(--text-2);
    margin-top: 2px;
  }
  .pipeline-steps-count {
    font-size: 9px;
    color: var(--text-3);
    margin-top: 2px;
  }
  .pipeline-actions {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
  }

  /* Buttons */
  .pipeline-run-btn {
    background: linear-gradient(180deg, #0088cc 0%, #006699 100%);
    color: #fff;
    border: 1px solid #0099dd;
    padding: 5px 14px;
    border-radius: var(--radius);
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 700;
    cursor: pointer;
    letter-spacing: 1px;
    text-transform: uppercase;
    transition: background 0.15s ease;
  }
  .pipeline-run-btn:hover { background: linear-gradient(180deg, #0099dd 0%, #0077aa 100%); }

  .pipeline-stop-btn {
    background: linear-gradient(180deg, #cc3300 0%, #992200 100%);
    color: #fff;
    border: 1px solid #dd4411;
    padding: 5px 14px;
    border-radius: var(--radius);
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 700;
    cursor: pointer;
    letter-spacing: 1px;
    text-transform: uppercase;
    transition: background 0.15s ease;
  }
  .pipeline-stop-btn:hover { background: linear-gradient(180deg, #dd4411 0%, #aa2200 100%); }

  /* Status badge */
  .pipeline-status {
    font-family: var(--font-display);
    font-size: 9px;
    padding: 2px 8px;
    border-radius: 3px;
    letter-spacing: 0.5px;
    text-transform: uppercase;
  }
  .status-running {
    background: var(--amber-dim, #332900);
    color: var(--amber, #ffb800);
    border: 1px solid #ffb80033;
    animation: blink 1.5s infinite;
  }
  .status-completed {
    background: #003311;
    color: #00cc66;
    border: 1px solid #00cc6633;
  }
  .status-failed {
    background: #330011;
    color: #ff4466;
    border: 1px solid #ff446633;
  }
  .status-stopped {
    background: #1a1a2e;
    color: #8888bb;
    border: 1px solid #8888bb33;
  }
  .status-pending {
    background: var(--bg-3, #222);
    color: var(--text-3);
    border: 1px solid var(--border);
  }

  /* Progress bar */
  .progress-section {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .progress-label {
    display: flex;
    justify-content: space-between;
    font-size: 9px;
    color: var(--text-3);
    font-family: var(--font-display);
    letter-spacing: 0.3px;
  }
  .progress-fraction { color: var(--text-2); }
  .progress-track {
    height: 4px;
    background: var(--bg-3, #1a1a1a);
    border-radius: 2px;
    overflow: hidden;
  }
  .progress-fill {
    height: 100%;
    background: linear-gradient(90deg, #0088cc, #00ccff);
    border-radius: 2px;
    transition: width 0.4s ease;
  }

  /* Step train */
  .step-train {
    display: flex;
    align-items: flex-start;
    flex-wrap: wrap;
    gap: 0;
    padding: 4px 0;
  }
  .step-connector {
    width: 16px;
    height: 2px;
    background: var(--border);
    margin-top: 12px;
    flex-shrink: 0;
    transition: background 0.3s ease;
  }
  .step-connector.connector-done { background: #00cc6688; }

  .step-node {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 3px;
    cursor: default;
    flex-shrink: 0;
  }
  .step-icon {
    width: 22px;
    height: 22px;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 9px;
    font-weight: 700;
    border: 2px solid transparent;
    transition: all 0.2s ease;
  }
  .step-label {
    font-size: 8px;
    color: var(--text-3);
    max-width: 56px;
    text-align: center;
    line-height: 1.2;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* Step state colors */
  .step-done .step-icon {
    background: #004422;
    border-color: #00cc66;
    color: #00cc66;
  }
  .step-running .step-icon {
    background: #002244;
    border-color: #00bbff;
    color: #00bbff;
  }
  .step-failed .step-icon {
    background: #330011;
    border-color: #ff4466;
    color: #ff4466;
  }
  .step-cancelled .step-icon,
  .step-skipped .step-icon {
    background: var(--bg-3, #1a1a1a);
    border-color: var(--border);
    color: var(--text-3);
  }
  .step-pending .step-icon {
    background: var(--bg-3, #1a1a1a);
    border-color: var(--border);
    color: var(--text-3);
  }

  /* Pulse animation for running step */
  .pulse {
    animation: pulse-ring 1.2s ease-out infinite;
  }
  @keyframes pulse-ring {
    0%   { box-shadow: 0 0 0 0 #00bbff55; }
    70%  { box-shadow: 0 0 0 6px #00bbff00; }
    100% { box-shadow: 0 0 0 0 #00bbff00; }
  }

  /* Run history */
  .run-history {
    border-top: 1px solid var(--border);
    padding-top: 6px;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .run-row {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 9px;
    color: var(--text-3);
  }
  .run-icon {
    font-size: 9px;
    font-weight: 700;
    width: 14px;
    text-align: center;
  }
  .run-icon.status-completed { color: #00cc66; }
  .run-icon.status-failed { color: #ff4466; }
  .run-icon.status-stopped { color: #8888bb; }
  .run-date { flex: 1; color: var(--text-2); }
  .run-duration { color: var(--text-3); min-width: 32px; text-align: right; }
  .run-status-text {
    font-family: var(--font-display);
    font-size: 8px;
    letter-spacing: 0.5px;
    text-transform: uppercase;
    min-width: 52px;
    text-align: right;
  }
  .run-status-text.status-completed { color: #00cc66; }
  .run-status-text.status-failed { color: #ff4466; }
  .run-status-text.status-stopped { color: #8888bb; }

  /* Empty state */
  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 40px 20px;
    color: var(--text-3);
    text-align: center;
  }
  .empty-icon { font-size: 24px; opacity: 0.4; }
  .empty-title { font-size: 12px; font-weight: 600; color: var(--text-2); }
  .empty-hint { font-size: 10px; color: var(--text-3); }

  /* Reference card */
  .reference-card {
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 8px 10px;
    font-size: 9px;
    color: var(--text-2);
  }
  .reference-title {
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 700;
    color: var(--text-1);
    letter-spacing: 0.5px;
    text-transform: uppercase;
    margin-bottom: 5px;
  }
  .reference-grid {
    display: flex;
    flex-direction: column;
    gap: 3px;
    margin-bottom: 6px;
  }
  .action-chip {
    font-family: var(--font-mono, monospace);
    font-size: 9px;
    color: var(--accent, #0088cc);
  }
  .action-desc {
    font-family: inherit;
    color: var(--text-3);
    margin-left: 4px;
  }
  .condition-hint {
    border-top: 1px solid var(--border);
    padding-top: 5px;
    font-size: 9px;
    color: var(--text-3);
    line-height: 1.6;
  }
  .hint-label {
    font-weight: 700;
    color: var(--text-2);
    margin-right: 3px;
  }
  .condition-hint code {
    font-family: var(--font-mono, monospace);
    font-size: 8px;
    background: var(--bg-3, #2a2a2a);
    padding: 1px 3px;
    border-radius: 2px;
    color: var(--text-1);
  }

  @keyframes blink {
    0%, 100% { opacity: 1; }
    50%       { opacity: 0.5; }
  }
</style>
