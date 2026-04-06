<script lang="ts">
  import { tasks } from '../stores/tasks';
  import { session } from '../stores/session';
  import { machines } from '../stores/machines';
  import { getAgentMessages, getRules, getRuleHistory, getTaskHistory } from '../api';
  import type { AgentMessage, AutoRule, RuleFireEvent, TaskHistoryEntry } from '../types';
  import { appVisible } from '../stores/visibility';
  import { tr } from '$lib/i18n';

  let machineIds = $derived(Object.keys($machines));
  let agentStatus = $state<Record<string, {running: boolean, task: string, elapsed: number}>>({});

  // Initialize agent status for each machine
  $effect(() => {
    for (const mid of machineIds) {
      if (!agentStatus[mid]) {
        agentStatus[mid] = { running: false, task: '', elapsed: 0 };
      }
    }
  });

  let activeRules = $state(0);
  let firedToday = $state(0);
  let alerts = $state<{type: string, message: string, time: string}[]>([]);
  // $state so that elapsed-time updates drive re-renders
  let now = $state(Date.now());

  // Derives running tasks per machine
  let runningTasks = $derived(
    $tasks.filter(t => t.status === 'running')
  );

  // Only tick the clock when at least one agent is running
  $effect(() => {
    if (runningTasks.length === 0) return;
    const interval = setInterval(() => { now = Date.now(); }, 1000);
    return () => clearInterval(interval);
  });

  $effect(() => {
    const updated: Record<string, {running: boolean, task: string, elapsed: number}> = {};
    for (const mid of machineIds) {
      const running = runningTasks.find(t => t.target === mid);
      updated[mid] = {
        running: !!running,
        task: running?.prompt.slice(0, 50) || '',
        elapsed: running ? Math.floor((now - (running.startedAt ?? now)) / 1000) : 0,
      };
    }
    agentStatus = updated;
  });

  // 2a: destroyed is local to the effect run to avoid reset-on-remount bug
  $effect(() => {
    let destroyed = false;
    const interval = setInterval(async () => {
      if (destroyed) return;
      await loadData();
    }, 15000);
    loadData();
    return () => { destroyed = true; clearInterval(interval); };
  });

  // 2b: recentCommits as $derived.by instead of $effect writing state
  let recentCommits = $derived.by(() => {
    const s = $session;
    const commits: string[] = [];
    if (s.commitsBack?.length) commits.push(...s.commitsBack.slice(0, 2));
    if (s.commitsFront?.length) commits.push(...s.commitsFront.slice(0, 2));
    return commits.slice(0, 3);
  });

  async function loadData() {
    if (!$appVisible) return;
    try {
      const rules = await getRules();
      activeRules = rules.filter(r => r.enabled).length;
      const history = await getRuleHistory();
      const today = new Date().toISOString().slice(0, 10);
      firedToday = history.filter(h => h.timestamp.startsWith(today)).length;
    } catch { /* ignore */ }

    try {
      const msgs = await getAgentMessages(undefined, false, 'conflict');
      const failedHistory = await getTaskHistory(undefined, 'fail');
      const newAlerts: typeof alerts = [];
      for (const m of msgs.slice(-5)) {
        newAlerts.push({ type: 'conflict', message: m.content, time: m.timestamp });
      }
      for (const h of failedHistory.slice(-5)) {
        newAlerts.push({ type: 'fail', message: `${h.target}: ${h.prompt.slice(0, 40)}`, time: h.timestamp });
      }
      alerts = newAlerts.sort((a, b) => b.time.localeCompare(a.time)).slice(0, 10);
    } catch { /* ignore */ }
  }

  function formatElapsed(secs: number): string {
    const h = Math.floor(secs / 3600);
    const m = Math.floor((secs % 3600) / 60);
    const s = secs % 60;
    const pad = (n: number) => String(n).padStart(2, '0');
    return `${pad(h)}:${pad(m)}:${pad(s)}`;
  }
</script>

<div class="mc-panel">
  <div class="mc-header">{$tr('mc.title')}</div>

  <div class="mc-card">
    <div class="mc-card-title">{$tr('mc.agents')}</div>
    {#each machineIds as agent}
      {@const a = agentStatus[agent] ?? { running: false, task: '', elapsed: 0 }}
      <div class="mc-agent">
        <span class="mc-dot" class:mc-dot-active={a.running} class:mc-dot-idle={!a.running}></span>
        <span class="mc-agent-name">{($machines[agent]?.name ?? agent).toUpperCase()}</span>
        {#if a.running}
          <span class="mc-agent-task">{a.task || $tr('mc.active')}</span>
          <span class="mc-agent-time">{formatElapsed(a.elapsed)}</span>
        {:else}
          <span class="mc-agent-idle">{$tr('common.idle')}</span>
        {/if}
      </div>
    {/each}
  </div>

  <div class="mc-card">
    <div class="mc-card-title">{$tr('mc.recentCommits')}</div>
    {#if recentCommits.length > 0}
      {#each recentCommits as commit}
        <div class="mc-commit">{commit.slice(0, 60)}</div>
      {/each}
    {:else}
      <div class="mc-empty">{$tr('mc.noRecentCommits')}</div>
    {/if}
  </div>

  <div class="mc-card">
    <div class="mc-card-title">{$tr('mc.rules')}</div>
    <div class="mc-stats-row">
      <span class="mc-stat"><span class="mc-stat-val">{activeRules}</span> {$tr('mc.rulesActive')}</span>
      <span class="mc-stat"><span class="mc-stat-val mc-stat-fire">{firedToday}</span> {$tr('mc.rulesToday')}</span>
    </div>
  </div>

  <div class="mc-card mc-card-alerts">
    <div class="mc-card-title">{$tr('mc.alerts')}</div>
    {#if alerts.length > 0}
      {#each alerts as alert}
        <div class="mc-alert" class:mc-alert-conflict={alert.type === 'conflict'} class:mc-alert-fail={alert.type === 'fail'}>
          <span class="mc-alert-icon">{alert.type === 'conflict' ? '!' : 'x'}</span>
          <span class="mc-alert-msg">{alert.message}</span>
        </div>
      {/each}
    {:else}
      <div class="mc-empty">{$tr('mc.noAlerts')}</div>
    {/if}
  </div>
</div>

<style>
  .mc-panel {
    width: 280px;
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    gap: 0;
    background: var(--bg-0);
    border-left: 1px solid var(--border);
    overflow-y: auto;
    overflow-x: hidden;
  }
  .mc-panel::-webkit-scrollbar { width: 3px; }
  .mc-panel::-webkit-scrollbar-thumb { background: var(--border-bright); border-radius: 2px; }
  .mc-header {
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 2px;
    color: var(--cyan);
    padding: 8px 12px 4px;
    text-transform: uppercase;
    border-bottom: 1px solid var(--border);
  }
  .mc-card {
    padding: 8px 12px;
    border-bottom: 1px solid var(--border);
  }
  .mc-card-title {
    font-family: var(--font-display);
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 1.5px;
    text-transform: uppercase;
    color: var(--text-3);
    margin-bottom: 6px;
  }
  .mc-agent {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 10px;
    padding: 3px 0;
  }
  .mc-dot { width: 6px; height: 6px; border-radius: 50%; flex-shrink: 0; }
  .mc-dot-active { background: var(--green); box-shadow: 0 0 4px var(--green); }
  .mc-dot-idle { background: var(--text-3); }
  .mc-agent-name {
    font-weight: 700;
    font-family: var(--font-display);
    font-size: 9px;
    color: var(--text-1);
    min-width: 40px;
  }
  .mc-agent-task {
    color: var(--text-2);
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .mc-agent-time {
    color: var(--cyan);
    font-family: var(--font-mono);
    font-size: 9px;
    flex-shrink: 0;
  }
  .mc-agent-idle { color: var(--text-3); font-style: italic; }
  .mc-commit {
    font-family: var(--font-mono);
    font-size: 9px;
    color: var(--text-2);
    padding: 2px 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .mc-stats-row { display: flex; gap: 12px; }
  .mc-stat { font-size: 10px; color: var(--text-2); }
  .mc-stat-val { font-weight: 700; color: var(--amber); }
  .mc-stat-fire { color: var(--cyan); }
  .mc-card-alerts { flex: 1; min-height: 0; overflow-y: auto; }
  .mc-alert {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 9px;
    padding: 3px 0;
    color: var(--text-1);
  }
  .mc-alert-icon {
    width: 14px;
    height: 14px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 3px;
    font-weight: 700;
    font-size: 8px;
    flex-shrink: 0;
  }
  .mc-alert-conflict .mc-alert-icon { background: var(--amber); color: var(--bg-0); }
  .mc-alert-fail .mc-alert-icon { background: var(--red); color: var(--bg-0); }
  .mc-alert-msg {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .mc-empty {
    font-size: 9px;
    color: var(--text-3);
    font-style: italic;
  }
</style>
