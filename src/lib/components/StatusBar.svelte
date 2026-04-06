<script lang="ts">
  import { onMount } from 'svelte';
  import { session } from '$lib/stores/session';
  import { runningCount } from '$lib/stores/tasks';
  import { machines } from '$lib/stores/machines';
  import { locale, tr } from '$lib/i18n';
  import { theme } from '$lib/stores/theme';
  import { getSystemStats, type SystemStats } from '$lib/api';

  let onlineCount = $derived(
    Object.values($machines).filter((m) => m.health?.online === true).length
  );

  let branch = $derived($session.rama || '—');
  let themeLabel = $derived($theme === 'dark' ? '◐ Dark' : '◑ Light');
  let localeLabel = $derived($locale === 'es' ? 'ES' : 'EN');

  let stats: SystemStats | null = $state(null);

  function formatUptime(secs: number): string {
    const d = Math.floor(secs / 86400);
    const h = Math.floor((secs % 86400) / 3600);
    const m = Math.floor((secs % 3600) / 60);
    if (d > 0) return `${d}d ${h}h`;
    if (h > 0) return `${h}h ${m}m`;
    return `${m}m`;
  }

  function memPercent(used: number, total: number): number {
    return total > 0 ? Math.round((used / total) * 100) : 0;
  }

  onMount(() => {
    const load = () => {
      getSystemStats().then(s => { stats = s; }).catch(() => {});
    };
    load();
    const interval = setInterval(load, 5000);
    return () => clearInterval(interval);
  });
</script>

<footer class="status-bar" aria-label="Status bar">
  <div class="status-left">
    <span class="status-item" title={$tr('status.branch', { name: branch })}>
      ⎇ {branch}
    </span>
    <span class="status-divider"></span>
    <span class="status-item" title={$tr('status.machines', { count: onlineCount })}>
      ◈ {onlineCount}
    </span>
    <span class="status-divider"></span>
    <span class="status-item" class:has-tasks={$runningCount > 0} title={$tr('status.tasks', { count: $runningCount })}>
      ▶ {$runningCount}
    </span>
  </div>

  <div class="status-right">
    {#if stats}
      <span class="status-item" class:warn={stats.cpuUsage > 80} title="CPU usage">
        CPU {stats.cpuUsage.toFixed(0)}%
      </span>
      <span class="status-divider"></span>
      <span class="status-item" class:warn={memPercent(stats.memoryUsed, stats.memoryTotal) > 85} title="Memory: {stats.memoryUsed.toFixed(1)} / {stats.memoryTotal.toFixed(1)} GB">
        MEM {stats.memoryUsed.toFixed(1)}/{stats.memoryTotal.toFixed(0)}G
      </span>
      <span class="status-divider"></span>
      <span class="status-item" title="Disk: {stats.diskUsed}G / {stats.diskTotal}G">
        DISK {stats.diskUsed}/{stats.diskTotal}G
      </span>
      <span class="status-divider"></span>
      <span class="status-item" title="System uptime">
        UP {formatUptime(stats.uptimeSecs)}
      </span>
    {:else}
      <span class="status-item">...</span>
    {/if}
    <span class="status-divider"></span>
    <span class="status-item">
      {localeLabel}
    </span>
    <span class="status-divider"></span>
    <span class="status-item">
      {themeLabel}
    </span>
  </div>
</footer>

<style>
  .status-bar {
    position: relative;
    flex-shrink: 0;
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 22px;
    padding: 0 8px;
    background: var(--bg-1);
    border-top: 1px solid var(--border);
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--text-2);
    user-select: none;
    z-index: 10;
  }

  .status-left,
  .status-right {
    display: flex;
    align-items: center;
    gap: 0;
  }

  .status-item {
    display: flex;
    align-items: center;
    padding: 0 6px;
    height: 22px;
    line-height: 22px;
    white-space: nowrap;
  }

  .status-item.has-tasks {
    color: var(--cyan);
  }

  .status-item.warn {
    color: var(--warning, #f0a020);
  }

  .status-divider {
    width: 1px;
    height: 12px;
    background: var(--border);
    opacity: 0.5;
  }
</style>
