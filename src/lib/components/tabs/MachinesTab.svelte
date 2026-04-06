<script lang="ts">
  import { onMount } from 'svelte';
  import { machines, refreshMachinesStore, offlineMachines } from '../../stores/machines';
  import { toggleMachine, reconnectMachine, getMachineMetrics, executeMachineCommand, getMachineLogs } from '../../api';
  import type { MachineMetrics } from '../../api';
  import { addToast } from '../../stores/notifications';
  import { handleError } from '../../utils';
  import { t, tr } from '$lib/i18n';
  import Skeleton from '$lib/components/Skeleton.svelte';

  let reconnecting: Record<string, boolean> = $state({});
  let initialLoading = $state(true);
  let copiedId: string | null = $state(null);

  // Per-machine expandable panel state
  let metricsOpen: Record<string, boolean> = $state({});
  let metricsData: Record<string, MachineMetrics | null> = $state({});
  let metricsLoading: Record<string, boolean> = $state({});
  let metricsError: Record<string, string | null> = $state({});

  // Per-machine quick command state
  let cmdInput: Record<string, string> = $state({});
  let cmdRunning: Record<string, boolean> = $state({});
  let cmdOutput: Record<string, string | null> = $state({});
  let cmdError: Record<string, string | null> = $state({});

  // Per-machine logs state
  let logsOpen: Record<string, boolean> = $state({});
  let logsData: Record<string, string[]> = $state({});
  let logsLoading: Record<string, boolean> = $state({});
  let logsError: Record<string, string | null> = $state({});

  // Interval handles for auto-refresh
  let metricsIntervals: Record<string, ReturnType<typeof setInterval>> = {};

  function machineColor(id: string): string {
    const palette = ['#7eb8ff', '#7effa0', '#ffb74d', '#c084fc', '#f48fb1', '#4fc3f7', '#ff8a65', '#aed581'];
    let hash = 0;
    for (let i = 0; i < id.length; i++) hash = ((hash << 5) - hash + id.charCodeAt(i)) | 0;
    return palette[Math.abs(hash) % palette.length];
  }

  function pctClass(val: string | undefined): string {
    const n = parseInt(val || '');
    if (isNaN(n)) return 'off';
    if (n >= 90) return 'crit';
    if (n >= 70) return 'warn';
    return 'ok';
  }

  function pctNum(val: string | undefined): number {
    const n = parseInt(val || '');
    return isNaN(n) ? 0 : Math.min(100, Math.max(0, n));
  }

  function barColor(val: string | undefined): string {
    const n = parseInt(val || '');
    if (isNaN(n)) return 'var(--border)';
    if (n >= 80) return 'var(--red, #f43f5e)';
    if (n >= 60) return 'var(--amber, #f59e0b)';
    return 'var(--green, #00ff88)';
  }

  function metricBarColor(pct: number): string {
    if (pct >= 80) return 'var(--red, #f43f5e)';
    if (pct >= 60) return 'var(--amber, #f59e0b)';
    return 'var(--green, #00ff88)';
  }

  function metricPctClass(pct: number): string {
    if (pct >= 90) return 'crit';
    if (pct >= 70) return 'warn';
    return 'ok';
  }

  function formatBytes(kb: number): string {
    if (kb >= 1024 * 1024) return (kb / 1024 / 1024).toFixed(1) + ' GB';
    if (kb >= 1024) return (kb / 1024).toFixed(1) + ' MB';
    return kb.toFixed(0) + ' KB';
  }

  async function handleToggle(id: string, currentlyOn: boolean) {
    try {
      await toggleMachine(id, !currentlyOn);
      addToast(id.toUpperCase() + ' ' + (currentlyOn ? t('machines.disabled') : t('machines.enabled')), 'info');
      await refreshMachinesStore();
    } catch (e) {
      addToast('Error: ' + handleError(e), 'error');
    }
  }

  async function handleReconnect(id: string) {
    reconnecting = { ...reconnecting, [id]: true };
    try {
      const info = await reconnectMachine(id);
      if (info) {
        machines.update((m) => ({ ...m, [id]: info }));
        if (info.health?.online) {
          addToast(id.toUpperCase() + ' ' + t('machines.reconnected'), 'success');
        } else {
          addToast(id.toUpperCase() + ' ' + t('machines.stillOffline'), 'error');
        }
      }
    } catch (e) {
      addToast('Error: ' + handleError(e), 'error');
    } finally {
      reconnecting = { ...reconnecting, [id]: false };
    }
  }

  async function handleRefresh(id: string) {
    await handleReconnect(id);
  }

  async function copyToClipboard(id: string, text: string) {
    try {
      await navigator.clipboard.writeText(text);
      copiedId = id;
      addToast('IP copiada', 'info');
      setTimeout(() => { copiedId = null; }, 2000);
    } catch {
      addToast('No se pudo copiar', 'error');
    }
  }

  // ---- Metrics ----

  async function loadMetrics(id: string) {
    metricsLoading = { ...metricsLoading, [id]: true };
    metricsError = { ...metricsError, [id]: null };
    try {
      const data = await getMachineMetrics(id);
      metricsData = { ...metricsData, [id]: data };
    } catch (e) {
      metricsError = { ...metricsError, [id]: handleError(e) };
    } finally {
      metricsLoading = { ...metricsLoading, [id]: false };
    }
  }

  function toggleMetrics(id: string) {
    const isNowOpen = !metricsOpen[id];
    metricsOpen = { ...metricsOpen, [id]: isNowOpen };
    if (isNowOpen) {
      loadMetrics(id);
      // Auto-refresh every 30s
      if (!metricsIntervals[id]) {
        metricsIntervals[id] = setInterval(() => {
          if (metricsOpen[id]) {
            loadMetrics(id);
          }
        }, 30000);
      }
    } else {
      // Stop auto-refresh when panel is closed
      if (metricsIntervals[id]) {
        clearInterval(metricsIntervals[id]);
        delete metricsIntervals[id];
      }
    }
  }

  // ---- Quick Command ----

  async function runCommand(id: string) {
    const cmd = (cmdInput[id] || '').trim();
    if (!cmd) return;
    cmdRunning = { ...cmdRunning, [id]: true };
    cmdOutput = { ...cmdOutput, [id]: null };
    cmdError = { ...cmdError, [id]: null };
    try {
      const result = await executeMachineCommand(id, cmd);
      cmdOutput = { ...cmdOutput, [id]: result };
    } catch (e) {
      cmdError = { ...cmdError, [id]: handleError(e) };
    } finally {
      cmdRunning = { ...cmdRunning, [id]: false };
    }
  }

  function handleCmdKeydown(id: string, e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      runCommand(id);
    }
  }

  // ---- Logs ----

  async function loadLogs(id: string) {
    logsLoading = { ...logsLoading, [id]: true };
    logsError = { ...logsError, [id]: null };
    try {
      const lines = await getMachineLogs(id, 50);
      logsData = { ...logsData, [id]: lines };
    } catch (e) {
      logsError = { ...logsError, [id]: handleError(e) };
    } finally {
      logsLoading = { ...logsLoading, [id]: false };
    }
  }

  function toggleLogs(id: string) {
    const isNowOpen = !logsOpen[id];
    logsOpen = { ...logsOpen, [id]: isNowOpen };
    if (isNowOpen) {
      loadLogs(id);
    }
  }

  // Cleanup intervals on component unmount
  $effect(() => {
    return () => {
      for (const id of Object.keys(metricsIntervals)) {
        clearInterval(metricsIntervals[id]);
      }
    };
  });

  onMount(() => {
    refreshMachinesStore().then(() => { initialLoading = false; }).catch(() => { initialLoading = false; });
  });
</script>

<div class="machines-grid">
  {#each Object.entries($machines) as [id, m]}
    {@const isOnline = m.health?.online === true}
    {@const isMonitorOffline = $offlineMachines.has(id)}
    {@const stats = (m.stats?.online || isOnline) ? m.stats : null}
    {@const enabled = m.enabled !== false}
    {@const hostDisplay = m.ip || (m.host !== 'local' ? m.host : 'localhost')}
    {@const metrics = metricsData[id] ?? null}
    <div class="machine-card" class:online={isOnline} class:offline={!isOnline} class:monitor-offline={isMonitorOffline}>
      <div class="machine-card-header">
        <div class="agent-dot" class:active={isOnline && !isMonitorOffline}></div>
        <span class="machine-name" style="color:{machineColor(id)}">{id.toUpperCase()}</span>
        <span class="machine-role">{m.role || m.os || ''}</span>
        {#if isMonitorOffline}
          <span class="offline-badge">OFFLINE</span>
        {/if}
        <button
          class="refresh-btn"
          class:spinning={reconnecting[id]}
          disabled={reconnecting[id]}
          title="Refrescar stats"
          aria-label="Refrescar {id}"
          onclick={() => handleRefresh(id)}
        >↻</button>
        <button
          class="machine-toggle"
          class:on={enabled}
          title={enabled ? $tr('machines.disable') : $tr('machines.enable')}
          aria-label="{enabled ? $tr('machines.disable') : $tr('machines.enable')} {id}"
          onclick={() => handleToggle(id, enabled)}
        ></button>
      </div>
      <div class="machine-ip">
        <span class="host-text">{hostDisplay}</span>
        {#if hostDisplay}
          <button
            class="copy-btn"
            class:copied={copiedId === id}
            title="Copiar IP"
            aria-label="Copiar IP de {id}"
            onclick={() => copyToClipboard(id, hostDisplay)}
          >{copiedId === id ? '✓' : '⎘'}</button>
        {/if}
        {#if m.os}
          <span class="machine-os">{m.os}</span>
        {/if}
      </div>
      {#if isMonitorOffline}
        <div class="machine-offline-row">
          <span class="machine-offline-label reconnecting-label">OFFLINE — reconnecting...</span>
          <button
            class="reconnect-btn"
            class:spinning={reconnecting[id]}
            disabled={reconnecting[id]}
            title={$tr('machines.reconnect')}
            onclick={() => handleReconnect(id)}
          >{reconnecting[id] ? '...' : $tr('machines.reconnect')}</button>
        </div>
      {:else if stats}
        <div class="machine-stats-bar">
          {#each [['CPU', stats.cpu], ['MEM', stats.mem], ['DSK', stats.disk]] as [label, val]}
            {@const pct = pctNum(val as string)}
            {@const color = barColor(val as string)}
            <div class="machine-stat-pill">
              <div class="stat-pill-top">
                <span class="ms-label">{label}</span>
                <span class="ms-val {pctClass(val as string)}">{val || '-'}</span>
              </div>
              {#if val && val !== '-'}
                <div class="stat-bar-track">
                  <div class="stat-bar-fill" style="width:{pct}%; background:{color}"></div>
                </div>
              {/if}
            </div>
          {/each}
          {#if metrics}
            <div class="machine-stat-pill" title="Network In">
              <div class="stat-pill-top">
                <span class="ms-label">NET↓</span>
                <span class="ms-val ok">{formatBytes(metrics.network_rx_kb)}</span>
              </div>
            </div>
            <div class="machine-stat-pill" title="Network Out">
              <div class="stat-pill-top">
                <span class="ms-label">NET↑</span>
                <span class="ms-val ok">{formatBytes(metrics.network_tx_kb)}</span>
              </div>
            </div>
          {/if}
          {#if stats.gpu && stats.gpu !== 'n/a' && stats.gpu !== '-'}
            {@const gpuMatch = (stats.gpu + '').match(/^(\d+)%/)}
            {@const gpuPct = gpuMatch ? gpuMatch[1] + '%' : stats.gpu}
            {@const gpuColor = barColor(gpuPct)}
            {@const gpuNum = pctNum(gpuPct)}
            <div class="machine-stat-pill" title={stats.gpu}>
              <div class="stat-pill-top">
                <span class="ms-label">GPU</span>
                <span class="ms-val {pctClass(gpuPct)}">{gpuPct}</span>
              </div>
              {#if gpuMatch}
                <div class="stat-bar-track">
                  <div class="stat-bar-fill" style="width:{gpuNum}%; background:{gpuColor}"></div>
                </div>
              {/if}
            </div>
          {/if}
        </div>
        {#if stats.uptime && stats.uptime !== '-'}
          <div class="machine-uptime">↑ {stats.uptime}</div>
        {/if}
      {:else}
        <div class="machine-offline-row">
          <span class="machine-offline-label">{$tr('machines.offline')}</span>
          <button
            class="reconnect-btn"
            class:spinning={reconnecting[id]}
            disabled={reconnecting[id]}
            title={$tr('machines.reconnect')}
            onclick={() => handleReconnect(id)}
          >{reconnecting[id] ? '...' : $tr('machines.reconnect')}</button>
        </div>
      {/if}

      <!-- Action buttons row -->
      {#if isOnline && !isMonitorOffline}
        <div class="machine-actions-row">
          <button
            class="action-btn"
            class:active={metricsOpen[id]}
            onclick={() => toggleMetrics(id)}
            title="Toggle detailed metrics"
          >
            <span class="action-icon">◈</span> Metrics
          </button>
          <button
            class="action-btn"
            class:active={logsOpen[id]}
            onclick={() => toggleLogs(id)}
            title="Toggle machine logs"
          >
            <span class="action-icon">≡</span> Logs
          </button>
        </div>
      {/if}

      <!-- Metrics expandable panel -->
      {#if metricsOpen[id]}
        <div class="expand-panel">
          <div class="panel-header">
            <span class="panel-title">METRICS</span>
            {#if metricsLoading[id]}
              <span class="panel-loading">loading…</span>
            {:else}
              <button class="panel-refresh-btn" onclick={() => loadMetrics(id)} title="Refresh metrics">↻</button>
            {/if}
          </div>
          {#if metricsError[id]}
            <div class="panel-error">{metricsError[id]}</div>
          {:else if metrics}
            <div class="metrics-grid">
              {#each [
                { label: 'CPU', pct: metrics.cpu_percent },
                { label: 'RAM', pct: metrics.ram_percent },
                { label: 'Disk', pct: metrics.disk_percent },
              ] as row}
                <div class="metric-row">
                  <span class="metric-label">{row.label}</span>
                  <div class="metric-bar-track">
                    <div
                      class="metric-bar-fill"
                      style="width:{Math.min(100, row.pct)}%; background:{metricBarColor(row.pct)}"
                    ></div>
                  </div>
                  <span class="metric-val {metricPctClass(row.pct)}">{row.pct.toFixed(1)}%</span>
                </div>
              {/each}
              <div class="metric-row">
                <span class="metric-label">Load</span>
                <div class="metric-bar-track">
                  <div
                    class="metric-bar-fill"
                    style="width:{Math.min(100, metrics.load_average * 25)}%; background:{metricBarColor(metrics.load_average * 25)}"
                  ></div>
                </div>
                <span class="metric-val ok">{metrics.load_average.toFixed(2)}</span>
              </div>
              <div class="metric-net-row">
                <span class="metric-label">Net ↓</span>
                <span class="metric-net-val">{formatBytes(metrics.network_rx_kb)}</span>
                <span class="metric-label" style="margin-left:10px">Net ↑</span>
                <span class="metric-net-val">{formatBytes(metrics.network_tx_kb)}</span>
              </div>
            </div>
          {:else if !metricsLoading[id]}
            <div class="panel-empty">No data yet</div>
          {/if}
        </div>
      {/if}

      <!-- Quick Command panel (always visible when online) -->
      {#if isOnline && !isMonitorOffline}
        <div class="quick-cmd-panel">
          <div class="quick-cmd-row">
            <input
              class="cmd-input"
              type="text"
              placeholder="$ quick command…"
              value={cmdInput[id] || ''}
              oninput={(e) => { cmdInput = { ...cmdInput, [id]: (e.currentTarget as HTMLInputElement).value }; }}
              onkeydown={(e) => handleCmdKeydown(id, e)}
              disabled={cmdRunning[id]}
              aria-label="Quick command for {id}"
            />
            <button
              class="cmd-run-btn"
              onclick={() => runCommand(id)}
              disabled={cmdRunning[id] || !(cmdInput[id] || '').trim()}
              title="Run command (Enter)"
            >{cmdRunning[id] ? '…' : '▶'}</button>
          </div>
          {#if cmdError[id]}
            <pre class="cmd-output cmd-output-error">{cmdError[id]}</pre>
          {:else if cmdOutput[id] !== null && cmdOutput[id] !== undefined}
            <pre class="cmd-output">{cmdOutput[id]}</pre>
          {/if}
        </div>
      {/if}

      <!-- Logs expandable panel -->
      {#if logsOpen[id]}
        <div class="expand-panel">
          <div class="panel-header">
            <span class="panel-title">LOGS (last 50 lines)</span>
            {#if logsLoading[id]}
              <span class="panel-loading">loading…</span>
            {:else}
              <button class="panel-refresh-btn" onclick={() => loadLogs(id)} title="Refresh logs">↻</button>
            {/if}
          </div>
          {#if logsError[id]}
            <div class="panel-error">{logsError[id]}</div>
          {:else if logsData[id]?.length}
            <div class="logs-scroll">
              <pre class="logs-pre">{logsData[id].join('\n')}</pre>
            </div>
          {:else if !logsLoading[id]}
            <div class="panel-empty">No logs found</div>
          {/if}
        </div>
      {/if}
    </div>
  {/each}
  {#if initialLoading && Object.keys($machines).length === 0}
    {#each [0, 1, 2] as _}
      <div class="machine-card">
        <div class="machine-card-header">
          <Skeleton width="12px" height="12px" variant="circle" />
          <Skeleton width="80px" height="14px" />
          <Skeleton width="50px" height="12px" />
        </div>
        <Skeleton width="120px" height="10px" />
        <div class="machine-stats-bar">
          <Skeleton width="60px" height="24px" variant="card" />
          <Skeleton width="60px" height="24px" variant="card" />
          <Skeleton width="60px" height="24px" variant="card" />
        </div>
      </div>
    {/each}
  {:else if Object.keys($machines).length === 0}
    <div class="empty-state">
      <span class="empty-icon">&#x25A1;</span>
      <div class="empty-title">{$tr('machines.noMachines')}</div>
      <div class="empty-hint">Add machines to ~/.config/jarvis/config.toml</div>
    </div>
  {/if}
</div>

<style>
  .machines-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
    gap: 10px;
    padding: 10px 14px;
    overflow-y: auto;
    flex: 1;
    align-content: start;
  }
  .machine-card {
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 14px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    transition: border-color 0.2s ease, box-shadow 0.2s ease;
    position: relative;
  }
  .machine-card::before {
    content: '';
    position: absolute;
    top: 0; left: 0; right: 0;
    height: 2px;
    background: var(--border);
    transition: background 0.2s ease;
    border-radius: 8px 8px 0 0;
  }
  .machine-card.online::before { background: var(--green); box-shadow: 0 0 8px var(--green-dim); }
  .machine-card:hover { border-color: var(--border-bright); box-shadow: 0 4px 16px rgba(0,0,0,0.3); }
  .machine-card.offline { opacity: 0.45; }
  .machine-card.monitor-offline {
    opacity: 1;
    border-color: rgba(244, 63, 94, 0.4);
    box-shadow: 0 0 12px rgba(244, 63, 94, 0.15);
  }
  .machine-card.monitor-offline::before { background: var(--red, #f43f5e); box-shadow: 0 0 8px rgba(244,63,94,0.4); }
  .offline-badge {
    font-family: var(--font-display);
    font-size: 7px;
    font-weight: 700;
    letter-spacing: 1px;
    text-transform: uppercase;
    color: var(--red, #f43f5e);
    background: rgba(244, 63, 94, 0.12);
    border: 1px solid rgba(244, 63, 94, 0.35);
    border-radius: 3px;
    padding: 1px 6px;
    flex-shrink: 0;
    animation: pulse-glow 2s infinite;
  }
  .reconnecting-label {
    color: var(--red, #f43f5e) !important;
    opacity: 0.85;
  }
  .machine-card-header { display: flex; align-items: center; gap: 8px; }
  .agent-dot {
    width: 6px; height: 6px;
    border-radius: 50%;
    flex-shrink: 0;
    background: var(--text-3);
    transition: background 0.3s ease, box-shadow 0.3s ease;
  }
  .agent-dot.active {
    background: var(--green);
    box-shadow: 0 0 8px var(--green);
    animation: pulse-glow 2s infinite;
  }
  .machine-name {
    font-family: var(--font-display);
    font-size: 13px;
    font-weight: 700;
    letter-spacing: 2px;
    text-transform: uppercase;
  }
  .machine-role {
    font-size: 8px;
    color: var(--text-2);
    background: var(--bg-3);
    padding: 2px 7px;
    border-radius: 3px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    font-weight: 600;
  }
  .refresh-btn {
    margin-left: auto;
    width: 22px; height: 22px;
    border-radius: 4px;
    border: 1px solid var(--border);
    background: transparent;
    color: var(--text-3);
    cursor: pointer;
    font-size: 13px;
    line-height: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: color 0.15s ease, border-color 0.15s ease, background 0.15s ease;
    flex-shrink: 0;
  }
  .refresh-btn:hover { color: var(--cyan); border-color: var(--cyan); background: #00d4ff11; }
  .refresh-btn:disabled { opacity: 0.4; cursor: default; }
  .refresh-btn.spinning { animation: spin 0.8s linear infinite; color: var(--cyan); }
  @keyframes spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }
  .machine-toggle {
    width: 30px; height: 15px;
    border-radius: 8px;
    border: 1px solid var(--border-bright);
    background: var(--bg-0);
    cursor: pointer;
    position: relative;
    transition: background 0.2s ease, border-color 0.2s ease;
    flex-shrink: 0;
  }
  .machine-toggle::after {
    content: '';
    position: absolute;
    top: 2px; left: 2px;
    width: 9px; height: 9px;
    border-radius: 50%;
    background: var(--text-3);
    transition: transform 0.2s ease, background 0.2s ease;
  }
  .machine-toggle.on { background: var(--green-dim); border-color: #00ff8844; }
  .machine-toggle.on::after { transform: translateX(15px); background: var(--green); }
  .machine-ip {
    font-size: 9px;
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .host-text { opacity: 0.8; }
  .copy-btn {
    font-size: 10px;
    color: var(--text-3);
    background: transparent;
    border: 1px solid transparent;
    border-radius: 3px;
    padding: 0 3px;
    cursor: pointer;
    transition: color 0.15s ease, border-color 0.15s ease, background 0.15s ease;
    line-height: 1.4;
    flex-shrink: 0;
  }
  .copy-btn:hover { color: var(--cyan); border-color: var(--border); background: #00d4ff11; }
  .copy-btn.copied { color: var(--green); }
  .machine-os { color: var(--text-2); }
  .machine-stats-bar { display: flex; gap: 6px; flex-wrap: wrap; }
  .machine-stat-pill {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 3px;
    background: var(--bg-0);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 4px 8px 5px;
    font-size: 10px;
    font-variant-numeric: tabular-nums;
  }
  .stat-pill-top {
    display: flex;
    align-items: center;
    gap: 4px;
    width: 100%;
  }
  .ms-label {
    color: var(--text-3);
    font-family: var(--font-display);
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.5px;
    text-transform: uppercase;
  }
  .ms-val { font-weight: 600; }
  .ms-val.ok { color: var(--green); }
  .ms-val.warn { color: var(--amber); }
  .ms-val.crit { color: var(--red); }
  .ms-val.off { color: var(--text-3); }
  .stat-bar-track {
    width: 52px;
    height: 3px;
    border-radius: 2px;
    background: var(--bg-2);
    flex-shrink: 0;
  }
  .stat-bar-fill {
    height: 100%;
    border-radius: 2px;
    transition: width 0.4s ease;
  }
  .machine-uptime {
    font-size: 9px;
    color: var(--text-3);
    letter-spacing: 0.3px;
    margin-top: -4px;
  }
  .machine-offline-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 4px 0;
  }
  .machine-offline-label {
    font-family: var(--font-display);
    font-size: 10px;
    color: var(--text-3);
    letter-spacing: 1px;
    text-transform: uppercase;
  }
  .reconnect-btn {
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
  .reconnect-btn:hover { background: #00d4ff22; border-color: #00d4ff55; }
  .reconnect-btn:disabled { opacity: 0.5; cursor: default; }
  .reconnect-btn.spinning { animation: pulse-glow 1s infinite; }

  /* Action buttons row */
  .machine-actions-row {
    display: flex;
    gap: 6px;
  }
  .action-btn {
    display: flex;
    align-items: center;
    gap: 4px;
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 600;
    letter-spacing: 0.5px;
    text-transform: uppercase;
    color: var(--text-3);
    background: var(--bg-0);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 4px 9px;
    cursor: pointer;
    transition: color 0.15s, border-color 0.15s, background 0.15s;
  }
  .action-btn:hover { color: var(--cyan); border-color: #00d4ff44; background: #00d4ff0a; }
  .action-btn.active { color: var(--cyan); border-color: #00d4ff66; background: #00d4ff15; }
  .action-icon { font-size: 10px; }

  /* Expandable panel */
  .expand-panel {
    background: var(--bg-0);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 10px 12px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .panel-header {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .panel-title {
    font-family: var(--font-display);
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 1px;
    text-transform: uppercase;
    color: var(--text-3);
    flex: 1;
  }
  .panel-loading {
    font-size: 9px;
    color: var(--text-3);
    font-style: italic;
  }
  .panel-refresh-btn {
    width: 18px; height: 18px;
    border-radius: 3px;
    border: 1px solid var(--border);
    background: transparent;
    color: var(--text-3);
    cursor: pointer;
    font-size: 11px;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: color 0.15s, border-color 0.15s;
  }
  .panel-refresh-btn:hover { color: var(--cyan); border-color: var(--cyan); }
  .panel-error {
    font-size: 10px;
    color: var(--red, #f43f5e);
    font-family: var(--font-mono, monospace);
    word-break: break-all;
  }
  .panel-empty {
    font-size: 10px;
    color: var(--text-3);
    font-style: italic;
  }

  /* Metrics grid */
  .metrics-grid {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .metric-row {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .metric-label {
    font-family: var(--font-display);
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.5px;
    text-transform: uppercase;
    color: var(--text-3);
    width: 30px;
    flex-shrink: 0;
  }
  .metric-bar-track {
    flex: 1;
    height: 4px;
    border-radius: 2px;
    background: var(--bg-2);
  }
  .metric-bar-fill {
    height: 100%;
    border-radius: 2px;
    transition: width 0.4s ease;
  }
  .metric-val {
    font-size: 10px;
    font-variant-numeric: tabular-nums;
    font-weight: 600;
    width: 44px;
    text-align: right;
    flex-shrink: 0;
  }
  .metric-val.ok { color: var(--green); }
  .metric-val.warn { color: var(--amber); }
  .metric-val.crit { color: var(--red); }
  .metric-net-row {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 2px;
  }
  .metric-net-val {
    font-size: 10px;
    font-variant-numeric: tabular-nums;
    color: var(--cyan);
    font-weight: 600;
  }

  /* Quick command panel */
  .quick-cmd-panel {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .quick-cmd-row {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .cmd-input {
    flex: 1;
    background: var(--bg-0);
    border: 1px solid var(--border);
    border-radius: 4px;
    color: var(--text-1);
    font-family: var(--font-mono, monospace);
    font-size: 10px;
    padding: 5px 8px;
    outline: none;
    transition: border-color 0.15s;
  }
  .cmd-input:focus { border-color: var(--cyan); }
  .cmd-input::placeholder { color: var(--text-3); }
  .cmd-input:disabled { opacity: 0.5; cursor: default; }
  .cmd-run-btn {
    width: 26px; height: 26px;
    border-radius: 4px;
    border: 1px solid var(--border);
    background: var(--bg-0);
    color: var(--cyan);
    cursor: pointer;
    font-size: 11px;
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    transition: background 0.15s, border-color 0.15s;
  }
  .cmd-run-btn:hover:not(:disabled) { background: #00d4ff15; border-color: var(--cyan); }
  .cmd-run-btn:disabled { opacity: 0.4; cursor: default; }
  .cmd-output {
    background: var(--bg-0);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 6px 8px;
    font-family: var(--font-mono, monospace);
    font-size: 9px;
    color: var(--text-2);
    white-space: pre-wrap;
    word-break: break-all;
    max-height: 120px;
    overflow-y: auto;
    margin: 0;
  }
  .cmd-output-error { color: var(--red, #f43f5e); border-color: rgba(244,63,94,0.3); }

  /* Logs panel */
  .logs-scroll {
    max-height: 200px;
    overflow-y: auto;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--bg-0);
  }
  .logs-pre {
    margin: 0;
    padding: 6px 8px;
    font-family: var(--font-mono, monospace);
    font-size: 9px;
    color: var(--text-2);
    white-space: pre-wrap;
    word-break: break-all;
  }

  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 40px 20px;
    color: var(--text-3);
    text-align: center;
    grid-column: 1 / -1;
  }
  .empty-icon { font-size: 24px; opacity: 0.4; }
  .empty-title { font-size: 12px; font-weight: 600; color: var(--text-2); }
  .empty-hint { font-size: 10px; color: var(--text-3); }
</style>
