<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { tasks } from '$lib/stores/tasks';
  import { machines, refreshMachinesStore } from '$lib/stores/machines';
  import { session, machineFeed } from '$lib/stores/session';
  import { addToast } from '$lib/stores/notifications';
  import type { Task, MachineInfo, Activity } from '$lib/types';

  let {
    onswitchTab = (_tab: string) => {}
  }: {
    onswitchTab?: (tab: string) => void;
  } = $props();

  let now = $state(Date.now());
  let nowTimer: ReturnType<typeof setInterval> | null = null;

  // Today's midnight in ms
  let todayStart = $derived.by(() => {
    const d = new Date(now);
    d.setHours(0, 0, 0, 0);
    return d.getTime();
  });

  let machineList = $derived(Object.values($machines));

  let activeTasks = $derived(
    $tasks.filter((t: Task) => t.status === 'running' || t.status === 'pending')
      .sort((a: Task, b: Task) => (b.startedAt ?? b.id) - (a.startedAt ?? a.id))
  );

  let recentCompleted = $derived(
    $tasks.filter((t: Task) => t.status === 'done' || t.status === 'error' || t.status === 'killed')
      .sort((a: Task, b: Task) => (b.finishedAt ?? b.id) - (a.finishedAt ?? a.id))
      .slice(0, 10)
  );

  let tasksToday = $derived(
    $tasks.filter((t: Task) => (t.startedAt ?? 0) >= todayStart)
  );

  let totalToday = $derived(tasksToday.length);

  let completedToday = $derived(
    tasksToday.filter((t: Task) => (t.status === 'done' || t.status === 'error') && t.startedAt && t.finishedAt)
  );

  let avgDurationSecs = $derived.by(() => {
    if (completedToday.length === 0) return 0;
    const total = completedToday.reduce((sum: number, t: Task) => {
      return sum + Math.round(((t.finishedAt ?? 0) - (t.startedAt ?? 0)) / 1000);
    }, 0);
    return Math.round(total / completedToday.length);
  });

  function formatDuration(secs: number): string {
    if (secs === 0) return '-';
    if (secs < 60) return `${secs}s`;
    return `${Math.floor(secs / 60)}m${secs % 60}s`;
  }

  let hourlyActivity = $derived.by(() => {
    const hours: { label: string; done: number; error: number }[] = [];
    const nowHour = new Date(now);
    nowHour.setMinutes(0, 0, 0);

    for (let i = 11; i >= 0; i--) {
      const hourStart = nowHour.getTime() - i * 3600000;
      const hourEnd = hourStart + 3600000;
      const label = new Date(hourStart).getHours().toString().padStart(2, '0') + 'h';
      const done = $tasks.filter((t: Task) =>
        (t.status === 'done') && (t.startedAt ?? 0) >= hourStart && (t.startedAt ?? 0) < hourEnd
      ).length;
      const error = $tasks.filter((t: Task) =>
        (t.status === 'error' || t.status === 'killed') && (t.startedAt ?? 0) >= hourStart && (t.startedAt ?? 0) < hourEnd
      ).length;
      hours.push({ label, done, error });
    }
    return hours;
  });

  let maxHourlyCount = $derived(Math.max(1, ...hourlyActivity.map((h: { label: string; done: number; error: number }) => h.done + h.error)));

  let errorsToday = $derived(
    tasksToday.filter((t: Task) => t.status === 'error' || t.status === 'killed').length
  );

  let errorRateToday = $derived(totalToday > 0 ? (errorsToday / totalToday) * 100 : 0);

  function statusBadgeClass(status: string): string {
    switch (status) {
      case 'running': return 'sb-running';
      case 'done': return 'sb-done';
      case 'error': case 'killed': return 'sb-error';
      case 'pending': return 'sb-pending';
      default: return 'sb-pending';
    }
  }

  function targetBadgeClass(target: string): string {
    if (target === 'both') return 'tb-both';
    return `tb-${target}`;
  }

  function elapsed(startedAt?: number): string {
    if (!startedAt) return '';
    const secs = Math.round((now - startedAt) / 1000);
    if (secs < 60) return `${secs}s`;
    const m = Math.floor(secs / 60);
    const s = secs % 60;
    return `${m}m${s}s`;
  }

  function duration(task: Task): string {
    if (!task.startedAt || !task.finishedAt) return '-';
    const secs = Math.round((task.finishedAt - task.startedAt) / 1000);
    if (secs < 60) return `${secs}s`;
    const m = Math.floor(secs / 60);
    const s = secs % 60;
    return `${m}m${s}s`;
  }

  function machineActiveTasks(machine: MachineInfo): Task[] {
    return $tasks.filter(
      (t: Task) =>
        (t.status === 'running' || t.status === 'pending') &&
        (t.target === machine.id || t.target === 'both')
    );
  }

  function truncate(s: string, n: number): string {
    if (!s) return '';
    return s.length > n ? s.slice(0, n) + '…' : s;
  }

  function onlineColor(machine: MachineInfo): string {
    if (!machine.enabled) return 'var(--text-3)';
    return machine.health?.online ? 'var(--green)' : 'var(--red)';
  }

  function onlineLabel(machine: MachineInfo): string {
    if (!machine.enabled) return 'disabled';
    return machine.health?.online ? 'online' : 'offline';
  }

  function lastSeen(machine: MachineInfo): string {
    if (machine.health?.latencyMs) return `${machine.health.latencyMs}ms`;
    return '-';
  }

  // Combined recent activity from all machine feeds (last 5)
  let recentActivity = $derived.by(() => {
    const combined: (Activity & { source: string })[] = [];
    for (const [machineId, feed] of Object.entries($machineFeed)) {
      for (const a of feed) {
        combined.push({ ...a, source: machineId });
      }
    }
    // Sort by ts descending (most recent first), fallback to stable order for items without ts
    combined.sort((a, b) => (b.ts ?? 0) - (a.ts ?? 0));
    return combined.slice(0, 5);
  });

  function activityIcon(type: string): string {
    if (type === 'tool') return '⚙';
    if (type === 'prompt') return '▶';
    return '◦';
  }

  function activityLabel(a: Activity): string {
    if (a.type === 'tool' && a.name) return a.name + (a.detail ? ': ' + a.detail.slice(0, 40) : '');
    if (a.content) return a.content.slice(0, 60);
    return a.type;
  }

  // Machine health: green if online, yellow if seen > 60s ago, red if offline / unknown
  function machineHealthColor(machine: MachineInfo): string {
    if (!machine.enabled) return 'var(--text-3)';
    if (machine.health?.online) {
      const latency = machine.health.latencyMs ?? 0;
      return latency > 200 ? 'var(--amber)' : 'var(--green)';
    }
    return 'var(--red)';
  }

  function machineHealthLabel(machine: MachineInfo): string {
    if (!machine.enabled) return 'disabled';
    if (machine.health?.online) {
      const latency = machine.health.latencyMs ?? 0;
      return latency > 200 ? `degraded (${latency}ms)` : `online (${latency}ms)`;
    }
    return 'offline';
  }

  // Whether JARVIS is actively processing (any agent running)
  let isProcessing = $derived($session.atlasRunning || $session.pixelRunning || activeTasks.length > 0);

  // --- Services Status ---
  interface ServiceStatus {
    name: string;
    port: number;
    host: string;
    status: 'up' | 'down' | 'checking';
    uptime?: string;
    checkedAt?: number;
  }

  let services: ServiceStatus[] = $state([
    { name: 'wa-bridge', port: 3142, host: 'localhost', status: 'checking' },
    { name: 'JARVIS HTTP', port: 3141, host: 'localhost', status: 'checking' },
    { name: 'Voice Agent', port: 3144, host: 'localhost', status: 'checking' },
    // Add remote services here, e.g.:
    // { name: 'GPU Dashboard', port: 7777, host: '192.168.1.200', status: 'checking' },
  ]);

  let serviceTimer: ReturnType<typeof setInterval> | null = null;

  async function checkService(index: number): Promise<void> {
    const svc = services[index];
    // Skip remote services (PIXEL) to avoid blocking — only check localhost
    if (svc.host !== 'localhost' && svc.host !== '127.0.0.1') {
      services[index] = { ...svc, status: 'down', checkedAt: Date.now() };
      return;
    }
    const url = svc.name === 'wa-bridge'
      ? `http://${svc.host}:${svc.port}/metrics`
      : svc.name === 'Voice Agent'
        ? `http://${svc.host}:${svc.port}/token`
        : `http://${svc.host}:${svc.port}/`;
    try {
      const controller = new AbortController();
      const timeout = setTimeout(() => controller.abort(), 2000);
      const resp = await fetch(url, { signal: controller.signal, mode: 'no-cors' });
      clearTimeout(timeout);
      services[index] = { ...svc, status: 'up', checkedAt: Date.now() };
    } catch {
      services[index] = { ...svc, status: 'down', checkedAt: Date.now() };
    }
  }

  async function checkAllServices() {
    // Run checks sequentially to avoid flooding the network
    for (let i = 0; i < services.length; i++) {
      await checkService(i);
    }
  }

  function serviceUptimeLabel(svc: ServiceStatus): string {
    if (!svc.checkedAt) return 'checking...';
    return svc.status === 'up' ? 'reachable' : 'unreachable';
  }

  function quickNewTask() {
    const input = document.getElementById('promptInput') as HTMLInputElement | null;
    if (input) {
      input.focus();
      input.scrollIntoView({ behavior: 'smooth', block: 'center' });
    } else {
      addToast('Usa el CommandBar para enviar una tarea', 'info');
    }
  }

  function quickCheckConnections() {
    addToast('Abre Ajustes > Conexiones para verificar', 'info');
  }

  onMount(() => {
    nowTimer = setInterval(() => { now = Date.now(); }, 5000);
    // Delay first check to not block initial render
    setTimeout(checkAllServices, 3000);
    serviceTimer = setInterval(checkAllServices, 30000);
  });

  onDestroy(() => {
    if (nowTimer) clearInterval(nowTimer);
    if (serviceTimer) clearInterval(serviceTimer);
  });
</script>

<div class="dashboard-panel">
  <!-- Header -->
  <div class="dash-header">
    <div class="section-label">Overview</div>
    <button class="refresh-btn" onclick={() => refreshMachinesStore()}>Refresh</button>
  </div>

  <!-- Session Info Card -->
  {#if $session.active && ($session.sessionId || $session.rama || $session.objetivo)}
    <div class="session-card active">
      <div class="session-indicator">
        <span class="session-dot active-dot-sess" class:processing-pulse={isProcessing}></span>
        <span class="session-label-title">SESION ACTIVA</span>
        {#if isProcessing}
          <span class="processing-tag">procesando</span>
        {/if}
      </div>
      {#if $session.rama}
        <div class="session-row">
          <span class="session-key">Rama</span>
          <span class="session-val">{$session.rama}</span>
        </div>
      {/if}
      {#if $session.objetivo}
        <div class="session-row">
          <span class="session-key">Objetivo</span>
          <span class="session-val obj">{$session.objetivo}</span>
        </div>
      {/if}
      {#if $session.sessionId}
        <div class="session-row">
          <span class="session-key">ID</span>
          <span class="session-val mono">{$session.sessionId.slice(0, 12)}…</span>
        </div>
      {/if}
    </div>
  {:else}
    <div class="session-card inactive">
      <span class="session-dot inactive-dot"></span>
      <span class="session-inactive-msg">Sin sesion activa — envia un prompt para empezar</span>
    </div>
  {/if}

  <!-- Quick Actions -->
  <div class="section-label">Acciones rapidas</div>
  <div class="quick-actions">
    <div class="qa-wrapper">
      <button class="qa-btn qa-primary" onclick={quickNewTask}>
        <span class="qa-icon">+</span>
        Nueva tarea
      </button>
      <span class="qa-hint">Enter</span>
    </div>
    <div class="qa-wrapper">
      <button class="qa-btn" onclick={quickCheckConnections}>
        <span class="qa-icon">~</span>
        Verificar conexiones
      </button>
    </div>
    <div class="qa-wrapper">
      <button class="qa-btn" onclick={() => onswitchTab('Timeline')}>
        <span class="qa-icon">&#8767;</span>
        Ver timeline
      </button>
      <span class="qa-hint">Ctrl+L</span>
    </div>
    <div class="qa-wrapper">
      <button class="qa-btn" onclick={() => onswitchTab('Commits')}>
        <span class="qa-icon">&#9673;</span>
        Ver commits
      </button>
      <span class="qa-hint">Ctrl+G</span>
    </div>
    <div class="qa-wrapper">
      <button class="qa-btn" onclick={() => onswitchTab('Tareas')}>
        <span class="qa-icon">&#9745;</span>
        Ver tareas
      </button>
      <span class="qa-hint">Ctrl+T</span>
    </div>
    <div class="qa-wrapper">
      <button class="qa-btn" onclick={() => onswitchTab('Cron')}>
        <span class="qa-icon">&#9200;</span>
        Ver crons
      </button>
    </div>
  </div>

  <!-- Quick Stats Bar -->
  <div class="stats-bar">
    <div class="stat-item">
      <span class="stat-val">{totalToday}</span>
      <span class="stat-label">Tasks today</span>
    </div>
    <div class="stat-sep"></div>
    <div class="stat-item">
      <span class="stat-val" class:stat-danger={errorRateToday > 20}>{errorRateToday.toFixed(0)}%</span>
      <span class="stat-label">Error rate</span>
    </div>
    <div class="stat-sep"></div>
    <div class="stat-item">
      <span class="stat-val">{activeTasks.length}</span>
      <span class="stat-label">Running now</span>
    </div>
    <div class="stat-sep"></div>
    <div class="stat-item">
      <span class="stat-val">{machineList.filter(m => m.health?.online).length}<span class="stat-denom">/{machineList.length}</span></span>
      <span class="stat-label">Machines up</span>
    </div>
    <div class="stat-sep"></div>
    <div class="stat-item">
      <span class="stat-val">{formatDuration(avgDurationSecs)}</span>
      <span class="stat-label">Avg duration</span>
    </div>
  </div>

  <!-- Machine Health Row -->
  <div class="section-label">Machine Health</div>
  {#if machineList.length === 0}
    <div class="empty-state">No machines configured</div>
  {:else}
    <div class="health-row">
      {#each machineList as machine}
        <div class="health-chip" title="{machine.name}: {machineHealthLabel(machine)}">
          <span
            class="health-dot"
            class:health-pulse={machine.health?.online && !machine.enabled}
            style="background:{machineHealthColor(machine)};box-shadow:0 0 5px {machineHealthColor(machine)}"
          ></span>
          <span class="health-name">{machine.name}</span>
          <span class="health-status" style="color:{machineHealthColor(machine)}">{machineHealthLabel(machine)}</span>
        </div>
      {/each}
    </div>
  {/if}

  <!-- Services Status -->
  <div class="section-label">Services</div>
  <div class="services-grid">
    {#each services as svc, i (svc.name)}
      <div class="service-chip" class:service-up={svc.status === 'up'} class:service-down={svc.status === 'down'}>
        <span
          class="service-dot"
          class:service-dot-up={svc.status === 'up'}
          class:service-dot-down={svc.status === 'down'}
          class:service-dot-checking={svc.status === 'checking'}
        ></span>
        <div class="service-info">
          <span class="service-name">{svc.name}</span>
          <span class="service-meta">:{svc.port} · {serviceUptimeLabel(svc)}</span>
        </div>
      </div>
    {/each}
  </div>

  <!-- Recent Activity -->
  <div class="section-label">Recent Activity</div>
  {#if recentActivity.length === 0}
    <div class="empty-state">No recent activity</div>
  {:else}
    <div class="activity-feed">
      {#each recentActivity as item, i (i)}
        <div class="activity-row activity-{item.type}">
          <span class="activity-source activity-source-{item.source}">{item.source.toUpperCase()}</span>
          <span class="activity-icon">{activityIcon(item.type)}</span>
          <span class="activity-label">{activityLabel(item)}</span>
        </div>
      {/each}
    </div>
  {/if}

  <!-- Hourly Activity Chart -->
  <div class="section-label">Activity (12h)</div>
  <div class="hourly-chart">
    {#each hourlyActivity as hour}
      <div class="hour-col">
        <div class="hour-bars">
          {#if hour.error > 0}
            <div class="hour-bar bar-error" style="height:{Math.max(2, Math.round((hour.error / maxHourlyCount) * 48))}px" title="{hour.error} errors"></div>
          {/if}
          {#if hour.done > 0}
            <div class="hour-bar bar-done" style="height:{Math.max(2, Math.round((hour.done / maxHourlyCount) * 48))}px" title="{hour.done} done"></div>
          {/if}
        </div>
        <div class="hour-label">{hour.label}</div>
      </div>
    {/each}
  </div>

  <!-- Machines Grid -->
  <div class="section-label">Machines</div>
  {#if machineList.length === 0}
    <div class="empty-state">No machines configured</div>
  {:else}
    <div class="machines-grid">
      {#each machineList as machine}
        {@const active = machineActiveTasks(machine)}
        <div class="machine-card" class:disabled={!machine.enabled}>
          <div class="machine-card-header">
            <span class="machine-dot" style="background:{onlineColor(machine)};box-shadow:{machine.health?.online ? '0 0 6px ' + onlineColor(machine) : 'none'}"></span>
            <span class="machine-name">{machine.name}</span>
            <span class="machine-status-badge" style="color:{onlineColor(machine)}">{onlineLabel(machine)}</span>
          </div>
          <div class="machine-card-meta">
            {#if machine.stats?.cpu}
              <span class="machine-meta-item">CPU <span class="meta-val">{machine.stats.cpu}</span></span>
              <span class="meta-sep">·</span>
            {/if}
            {#if machine.stats?.mem}
              <span class="machine-meta-item">MEM <span class="meta-val">{machine.stats.mem}</span></span>
              <span class="meta-sep">·</span>
            {/if}
            <span class="machine-meta-item">ping <span class="meta-val">{lastSeen(machine)}</span></span>
          </div>
          {#if active.length > 0}
            <div class="machine-active-task">
              <span class="active-dot"></span>
              <span class="active-prompt">{truncate(active[0].prompt, 60)}</span>
            </div>
          {:else}
            <div class="machine-idle">idle</div>
          {/if}
          {#if machine.gpu}
            <div class="machine-gpu">{machine.gpu}</div>
          {/if}
        </div>
      {/each}
    </div>
  {/if}

  <!-- Active Tasks -->
  <div class="section-label">Active Tasks</div>
  {#if activeTasks.length === 0}
    <div class="empty-state">No active tasks</div>
  {:else}
    <div class="task-list">
      {#each activeTasks as task (task.id)}
        <div class="task-row">
          <span class="task-badge {targetBadgeClass(task.target)}">{task.target}</span>
          <span class="task-badge {statusBadgeClass(task.status)}">{task.status}</span>
          <span class="task-prompt">{truncate(task.prompt, 80)}</span>
          {#if task.startedAt}
            <span class="task-elapsed">{elapsed(task.startedAt)}</span>
          {/if}
        </div>
      {/each}
    </div>
  {/if}

  <!-- Recent Completed -->
  <div class="section-label">Recently Completed</div>
  {#if recentCompleted.length === 0}
    <div class="empty-state">No completed tasks yet</div>
  {:else}
    <div class="task-list">
      {#each recentCompleted as task (task.id)}
        <div class="task-row">
          <span class="task-badge {targetBadgeClass(task.target)}">{task.target}</span>
          <span class="task-badge {statusBadgeClass(task.status)}">{task.status}</span>
          <span class="task-prompt">{truncate(task.prompt, 70)}</span>
          <span class="task-elapsed">{duration(task)}</span>
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .dashboard-panel {
    padding: 8px 14px;
    overflow-y: auto;
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .dash-header {
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

  .refresh-btn {
    margin-left: auto;
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

  /* Stats Bar */
  .stats-bar {
    display: flex;
    align-items: center;
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 10px 16px;
    gap: 0;
  }
  .stat-item {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
  }
  .stat-val {
    font-family: var(--font-display);
    font-size: 20px;
    font-weight: 700;
    color: var(--cyan);
    font-variant-numeric: tabular-nums;
  }
  .stat-denom {
    font-size: 12px;
    color: var(--text-2);
    font-weight: 400;
  }
  .stat-val.stat-danger { color: var(--red); }
  .stat-label {
    font-size: 9px;
    color: var(--text-2);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }
  .stat-sep {
    width: 1px;
    height: 32px;
    background: var(--border);
    flex-shrink: 0;
    margin: 0 8px;
  }

  /* Machines Grid */
  .machines-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
    gap: 8px;
  }
  .machine-card {
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 10px 12px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    transition: border-color 0.2s ease;
  }
  .machine-card:hover { border-color: var(--border-bright); }
  .machine-card.disabled { opacity: 0.5; }

  .machine-card-header {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .machine-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .machine-name {
    font-family: var(--font-display);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 1.5px;
    color: var(--text-1);
    flex: 1;
  }
  .machine-status-badge {
    font-family: var(--font-display);
    font-size: 7px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .machine-card-meta {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-wrap: wrap;
  }
  .machine-meta-item {
    font-size: 9px;
    color: var(--text-3);
    font-family: var(--font-mono);
  }
  .meta-val {
    color: var(--text-1);
    font-weight: 600;
  }
  .meta-sep {
    color: var(--text-3);
    font-size: 8px;
  }

  .machine-active-task {
    display: flex;
    align-items: center;
    gap: 5px;
    background: var(--amber-dim);
    border: 1px solid #ffb80022;
    border-radius: 4px;
    padding: 3px 6px;
  }
  .active-dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--amber);
    flex-shrink: 0;
    animation: blink 1.5s infinite;
  }
  .active-prompt {
    font-size: 9px;
    color: var(--text-1);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }

  .machine-idle {
    font-size: 9px;
    color: var(--text-3);
    font-style: italic;
    font-family: var(--font-mono);
  }
  .machine-gpu {
    font-size: 8px;
    color: var(--text-3);
    font-family: var(--font-mono);
  }

  /* Task List */
  .task-list {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .task-row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 10px;
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: 4px;
    transition: border-color 0.15s;
  }
  .task-row:hover { border-color: var(--border-bright); }

  .task-badge {
    font-family: var(--font-display);
    font-size: 8px;
    font-weight: 700;
    padding: 1px 6px;
    border-radius: 3px;
    text-transform: uppercase;
    letter-spacing: 0.8px;
    flex-shrink: 0;
  }
  .tb-atlas { background: #0a1a33; color: #7eb8ff; border: 1px solid #2196f322; }
  .tb-pixel { background: #0a2a1a; color: #7effa0; border: 1px solid #4caf5022; }
  .tb-both  { background: var(--cyan-dim); color: var(--cyan); border: 1px solid #00d4ff33; }

  .sb-running {
    background: var(--amber-dim);
    color: var(--amber);
    border: 1px solid #ffb80033;
    animation: blink 1.5s infinite;
  }
  .sb-done    { background: var(--green-dim); color: var(--green); border: 1px solid #00ff8833; }
  .sb-error   { background: #ff333510; color: var(--red); border: 1px solid #ff335522; }
  .sb-pending { background: #66666610; color: var(--text-3); border: 1px solid var(--border); }

  .task-prompt {
    flex: 1;
    font-size: 10px;
    color: var(--text-1);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  .task-elapsed {
    font-family: var(--font-mono);
    font-size: 9px;
    color: var(--text-3);
    flex-shrink: 0;
  }

  .empty-state {
    color: var(--text-3);
    font-size: 10px;
    font-style: italic;
    padding: 4px 2px;
  }

  @keyframes blink {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.4; }
  }

  /* Hourly Chart */
  .hourly-chart {
    display: flex;
    align-items: flex-end;
    gap: 3px;
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 10px 12px 6px;
    height: 72px;
  }

  .hour-col {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 3px;
    min-width: 0;
  }

  .hour-bars {
    display: flex;
    flex-direction: column-reverse;
    align-items: center;
    gap: 1px;
    height: 48px;
    justify-content: flex-start;
  }

  .hour-bar {
    width: 100%;
    min-height: 2px;
    border-radius: 2px 2px 0 0;
  }

  .bar-done  { background: var(--green); opacity: 0.7; }
  .bar-error { background: var(--red); opacity: 0.8; }

  .hour-label {
    font-family: var(--font-mono);
    font-size: 7px;
    color: var(--text-3);
    white-space: nowrap;
  }

  /* Session Card */
  .session-card {
    border-radius: 8px;
    padding: 10px 14px;
    display: flex;
    flex-direction: column;
    gap: 5px;
    border: 1px solid var(--border);
    background: var(--bg-2);
  }
  .session-card.active {
    border-color: #00d4ff33;
    background: #00d4ff08;
  }
  .session-card.inactive {
    flex-direction: row;
    align-items: center;
    gap: 8px;
    opacity: 0.7;
  }

  .session-indicator {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 2px;
  }
  .session-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .active-dot-sess {
    background: var(--cyan);
    box-shadow: 0 0 6px var(--cyan);
    animation: blink 2s infinite;
  }
  .inactive-dot {
    background: var(--text-3);
  }
  .session-label-title {
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 2px;
    color: var(--cyan);
    text-transform: uppercase;
  }
  .session-inactive-msg {
    font-size: 10px;
    color: var(--text-3);
    font-style: italic;
  }

  .session-row {
    display: flex;
    align-items: baseline;
    gap: 8px;
    min-width: 0;
  }
  .session-key {
    font-family: var(--font-display);
    font-size: 8px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 1px;
    color: var(--text-2);
    flex-shrink: 0;
    min-width: 52px;
  }
  .session-val {
    font-size: 11px;
    color: var(--text-1);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  .session-val.obj {
    color: var(--text-0);
    font-weight: 500;
  }
  .session-val.mono {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--text-2);
  }

  /* Quick Actions */
  .quick-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .qa-btn {
    display: flex;
    align-items: center;
    gap: 5px;
    font-family: var(--font-display);
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.3px;
    color: var(--text-1);
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 5px 12px;
    cursor: pointer;
    transition: background 0.15s, border-color 0.15s, color 0.15s;
    white-space: nowrap;
  }
  .qa-btn:hover {
    background: var(--bg-3, #1a2030);
    border-color: var(--border-bright);
    color: var(--text-0);
  }
  .qa-btn.qa-primary {
    color: var(--cyan);
    background: var(--cyan-dim);
    border-color: #00d4ff33;
  }
  .qa-btn.qa-primary:hover {
    background: #00d4ff22;
    border-color: #00d4ff55;
  }
  .qa-icon {
    font-size: 12px;
    line-height: 1;
    opacity: 0.8;
  }

  /* Quick Action wrappers with hint */
  .qa-wrapper {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 3px;
  }
  .qa-hint {
    font-family: var(--font-mono);
    font-size: 8px;
    color: var(--text-3);
    letter-spacing: 0.3px;
    white-space: nowrap;
  }

  /* Processing pulse — stronger animation when actively running */
  .active-dot-sess.processing-pulse {
    animation: processingPulse 0.8s ease-in-out infinite;
    box-shadow: 0 0 10px var(--cyan), 0 0 20px var(--cyan-dim);
  }
  @keyframes processingPulse {
    0%, 100% { opacity: 1; transform: scale(1); }
    50% { opacity: 0.6; transform: scale(1.4); }
  }
  .processing-tag {
    font-family: var(--font-display);
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 1px;
    text-transform: uppercase;
    color: var(--cyan);
    background: var(--cyan-dim);
    border: 1px solid #00d4ff33;
    border-radius: 3px;
    padding: 1px 5px;
    animation: processingPulse 0.8s ease-in-out infinite;
  }

  /* Machine Health Row */
  .health-row {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .health-chip {
    display: flex;
    align-items: center;
    gap: 5px;
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 5px 10px;
    min-width: 120px;
    cursor: default;
    transition: border-color 0.15s;
  }
  .health-chip:hover { border-color: var(--border-bright); }
  .health-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .health-name {
    font-family: var(--font-display);
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 1px;
    color: var(--text-1);
    flex: 1;
  }
  .health-status {
    font-size: 8px;
    font-family: var(--font-mono);
    white-space: nowrap;
  }

  /* Recent Activity Feed */
  .activity-feed {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .activity-row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 8px;
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: 4px;
    border-left-width: 2px;
  }
  .activity-tool   { border-left-color: var(--cyan); }
  .activity-text   { border-left-color: var(--green); }
  .activity-prompt { border-left-color: var(--amber); }
  .activity-source {
    font-family: var(--font-display);
    font-size: 7px;
    font-weight: 700;
    letter-spacing: 1px;
    flex-shrink: 0;
    min-width: 36px;
  }
  .activity-source-atlas { color: #7eb8ff; }
  .activity-source-pixel { color: #7effa0; }
  .activity-icon {
    font-size: 10px;
    flex-shrink: 0;
    color: var(--text-3);
    width: 12px;
    text-align: center;
  }
  .activity-label {
    font-size: 10px;
    color: var(--text-1);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
    flex: 1;
  }

  /* Services Grid */
  .services-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
    gap: 6px;
  }
  .service-chip {
    display: flex;
    align-items: center;
    gap: 8px;
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 7px 10px;
    transition: border-color 0.2s ease;
  }
  .service-chip:hover { border-color: var(--border-bright); }
  .service-chip.service-up { border-color: #00ff8833; }
  .service-chip.service-down { border-color: #ff335522; }

  .service-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex-shrink: 0;
    background: var(--text-3);
  }
  .service-dot-up {
    background: var(--green);
    box-shadow: 0 0 6px var(--green);
  }
  .service-dot-down {
    background: var(--red);
    box-shadow: 0 0 6px var(--red);
  }
  .service-dot-checking {
    background: var(--amber);
    animation: blink 1.2s infinite;
  }

  .service-info {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }
  .service-name {
    font-family: var(--font-display);
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.5px;
    color: var(--text-1);
  }
  .service-meta {
    font-family: var(--font-mono);
    font-size: 8px;
    color: var(--text-3);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
