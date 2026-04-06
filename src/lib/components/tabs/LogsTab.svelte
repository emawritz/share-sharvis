<script lang="ts">
  import { atlasFeed, pixelFeed, clearFeed } from '../../stores/session';
  import type { Activity, AppLogStats } from '../../types';
  import type { AppLogEntry } from '../../types';
  import { getAppLogs, clearAppLogs, getAppLogStats } from '../../api';
  import { onMount, onDestroy } from 'svelte';

  // Top-level mode: "agent" or "app"
  let mode = $state<'agent' | 'app'>('agent');

  // Agent selector
  let selectedAgent = $state<'atlas' | 'pixel'>('atlas');

  // Filter input (raw, updated immediately)
  let filterInput = $state('');
  // Debounced filter text
  let filterText = $state('');
  let debounceTimer: ReturnType<typeof setTimeout> | null = null;

  $effect(() => {
    const val = filterInput;
    if (debounceTimer !== null) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => { filterText = val; }, 200);
  });

  // Level filter for app logs
  type LevelFilter = 'all' | 'error' | 'warn' | 'info' | 'debug';
  let levelFilter = $state<LevelFilter>('all');

  // Auto-scroll
  let autoScroll = $state(true);

  // Scroll container ref
  let scrollEl = $state<HTMLElement | null>(null);

  // Feed selection: last 100 items from selected agent
  let rawFeed = $derived(selectedAgent === 'atlas' ? $atlasFeed : $pixelFeed);
  let feed = $derived(rawFeed.slice(-100));

  // Filtered items (agent mode)
  let filtered = $derived.by(() => {
    const q = filterText.trim().toLowerCase();
    if (!q) return feed;
    return feed.filter((a) => {
      const name = (a.name || '').toLowerCase();
      const content = (a.content || '').toLowerCase();
      const detail = (a.detail || '').toLowerCase();
      return name.includes(q) || content.includes(q) || detail.includes(q);
    });
  });

  // App logs state
  let appLogs = $state<AppLogEntry[]>([]);
  let appLogsError = $state('');
  let lastAppLogTs = $state<string | undefined>(undefined);
  let appLogPollInterval: ReturnType<typeof setInterval> | null = null;
  let appLogStats = $state<AppLogStats | null>(null);

  async function fetchAppLogs() {
    try {
      const newEntries = await getAppLogs(lastAppLogTs);
      if (newEntries.length > 0) {
        appLogs = [...appLogs, ...newEntries].slice(-500);
        lastAppLogTs = appLogs[appLogs.length - 1]?.timestamp;
      }
      appLogsError = '';
    } catch (e) {
      appLogsError = typeof e === 'string' ? e : String(e);
    }
  }

  async function fetchAppLogStats() {
    try {
      appLogStats = await getAppLogStats();
    } catch {
      // non-critical — ignore
    }
  }

  async function handleClearBackendLogs() {
    try {
      await clearAppLogs();
      appLogs = [];
      lastAppLogTs = undefined;
      appLogStats = null;
    } catch (e) {
      appLogsError = typeof e === 'string' ? e : String(e);
    }
  }

  function exportLogsAsTxt() {
    const lines = filteredAppLogs.map(
      (e) => `${formatAppLogTime(e.timestamp)} [${e.level.toUpperCase()}] ${e.message}`
    );
    const blob = new Blob([lines.join('\n')], { type: 'text/plain' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `jarvis-logs-${new Date().toISOString().slice(0, 19).replace(/:/g, '-')}.txt`;
    a.click();
    URL.revokeObjectURL(url);
  }

  // Filtered app logs (with level + text filter)
  let filteredAppLogs = $derived.by(() => {
    const q = filterText.trim().toLowerCase();
    return appLogs.filter((e) => {
      if (levelFilter !== 'all' && e.level.toLowerCase() !== levelFilter) return false;
      if (!q) return true;
      return e.message.toLowerCase().includes(q) || e.level.toLowerCase().includes(q);
    });
  });

  // Auto-scroll effect
  let alive = $state(false);
  onMount(() => {
    alive = true;
    fetchAppLogs();
    fetchAppLogStats();
    appLogPollInterval = setInterval(() => {
      fetchAppLogs();
      fetchAppLogStats();
    }, 5000);
    return () => { alive = false; };
  });

  onDestroy(() => {
    if (appLogPollInterval !== null) clearInterval(appLogPollInterval);
    if (debounceTimer !== null) clearTimeout(debounceTimer);
  });

  $effect(() => {
    const _len = filtered.length;
    const _appLen = filteredAppLogs.length;
    if (autoScroll && alive && scrollEl) {
      scrollEl.scrollTop = scrollEl.scrollHeight;
    }
  });

  // Log level class for agent mode (based on text content)
  function logLineClass(line: string): string {
    const l = line.toLowerCase();
    if (l.includes('error') || l.includes('err ') || l.includes('[e]')) return 'log-error';
    if (l.includes('warn') || l.includes('[w]')) return 'log-warn';
    if (l.includes('info') || l.includes('[i]')) return 'log-info';
    if (l.includes('debug') || l.includes('[d]')) return 'log-debug';
    return 'log-default';
  }

  // App log level class
  function appLogLevelClass(level: string): string {
    const l = level.toLowerCase();
    if (l === 'error') return 'log-error';
    if (l === 'warn') return 'log-warn';
    if (l === 'info') return 'log-info';
    if (l === 'debug') return 'log-debug';
    return 'log-default';
  }

  // Highlight matching text in a string, returns HTML string
  function highlightText(text: string, query: string): string {
    if (!query) return escapeHtml(text);
    const escaped = escapeHtml(text);
    const escapedQuery = escapeRegex(query);
    return escaped.replace(
      new RegExp(`(${escapedQuery})`, 'gi'),
      '<mark class="log-highlight">$1</mark>'
    );
  }

  function escapeHtml(s: string): string {
    return s
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;');
  }

  function escapeRegex(s: string): string {
    return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  }

  function typeBadge(type: Activity['type']): string {
    if (type === 'tool') return 'badge-tool';
    if (type === 'prompt') return 'badge-prompt';
    return 'badge-text';
  }

  function typeLabel(type: Activity['type']): string {
    if (type === 'tool') return 'TOOL';
    if (type === 'prompt') return 'PROMPT';
    return 'TEXT';
  }

  function itemName(a: Activity): string {
    if (a.type === 'tool' && a.name) return a.name;
    if (a.content) return a.content.substring(0, 60);
    return '';
  }

  function itemDetail(a: Activity): string {
    if (!a.detail) return '';
    return a.detail.substring(0, 80);
  }

  function formatAppLogTime(ts: string): string {
    try {
      return new Date(ts).toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit', second: '2-digit' });
    } catch {
      return ts;
    }
  }

  // Copy visible logs to clipboard
  async function copyAllLogs() {
    let text: string;
    if (mode === 'agent') {
      text = filtered
        .map((a) => `[${typeLabel(a.type)}] ${itemName(a)}${itemDetail(a) ? ' | ' + itemDetail(a) : ''}`)
        .join('\n');
    } else {
      text = filteredAppLogs
        .map((e) => `${formatAppLogTime(e.timestamp)} [${e.level.toUpperCase()}] ${e.message}`)
        .join('\n');
    }
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      // fallback: do nothing silently
    }
  }

  // Clear local view (does not affect backend for agent mode)
  function clearLocalView() {
    if (mode === 'agent') {
      clearFeed(selectedAgent);
    } else {
      void handleClearBackendLogs();
    }
  }
</script>

<div class="logs-panel">
  <div class="logs-header">
    <div class="section-label">Logs / Feed</div>
    <div class="logs-controls">
      <!-- Mode toggle -->
      <div class="mode-toggle">
        <button
          class="mode-btn"
          class:active={mode === 'agent'}
          onclick={() => mode = 'agent'}
        >Agent</button>
        <button
          class="mode-btn"
          class:active={mode === 'app'}
          onclick={() => mode = 'app'}
        >App</button>
      </div>

      {#if mode === 'agent'}
        <!-- Agent toggle -->
        <div class="agent-toggle">
          <button
            class="agent-btn"
            class:active={selectedAgent === 'atlas'}
            onclick={() => selectedAgent = 'atlas'}
          >ATLAS</button>
          <button
            class="agent-btn"
            class:active={selectedAgent === 'pixel'}
            onclick={() => selectedAgent = 'pixel'}
          >PIXEL</button>
        </div>
      {/if}

      <!-- Filter -->
      <input
        type="text"
        class="feed-filter"
        placeholder="Filtrar..."
        bind:value={filterInput}
        spellcheck="false"
        autocomplete="off"
      />

      <!-- Auto-scroll toggle -->
      <label class="autoscroll-label" title="Auto-scroll al final">
        <input type="checkbox" bind:checked={autoScroll} />
        <span>Auto</span>
      </label>

      <!-- Copy all -->
      <button class="action-btn" onclick={copyAllLogs} title="Copiar todo">&#x2398; Copiar</button>

      {#if mode === 'app'}
        <!-- Export logs as .txt -->
        <button class="action-btn" onclick={exportLogsAsTxt} title="Exportar como .txt">&#x21E9; Export</button>
      {/if}

      <!-- Clear view -->
      <button
        class="clear-btn"
        onclick={clearLocalView}
        title={mode === 'app' ? 'Limpiar logs del backend' : 'Limpiar vista'}
      >&#x2715; Limpiar</button>
    </div>
  </div>

  {#if mode === 'app'}
    <!-- Level filter pills -->
    <div class="level-pills">
      {#each (['all', 'error', 'warn', 'info', 'debug'] as const) as lvl}
        <button
          class="level-pill pill-{lvl}"
          class:active={levelFilter === lvl}
          onclick={() => levelFilter = lvl}
        >{lvl === 'all' ? 'Todos' : lvl.charAt(0).toUpperCase() + lvl.slice(1)}</button>
      {/each}
    </div>

    <!-- Stats bar -->
    {#if appLogStats !== null}
      <div class="stats-bar">
        <span class="stat-item stat-total">
          <span class="stat-label">Total</span>
          <span class="stat-value">{appLogStats.total}</span>
        </span>
        {#if (appLogStats.by_level['error'] ?? 0) > 0}
          <span class="stat-item stat-error">
            <span class="stat-label">Error</span>
            <span class="stat-value">{appLogStats.by_level['error']}</span>
          </span>
        {/if}
        {#if (appLogStats.by_level['warn'] ?? 0) > 0}
          <span class="stat-item stat-warn">
            <span class="stat-label">Warn</span>
            <span class="stat-value">{appLogStats.by_level['warn']}</span>
          </span>
        {/if}
        {#if (appLogStats.by_level['info'] ?? 0) > 0}
          <span class="stat-item stat-info">
            <span class="stat-label">Info</span>
            <span class="stat-value">{appLogStats.by_level['info']}</span>
          </span>
        {/if}
        {#if (appLogStats.by_level['debug'] ?? 0) > 0}
          <span class="stat-item stat-debug">
            <span class="stat-label">Debug</span>
            <span class="stat-value">{appLogStats.by_level['debug']}</span>
          </span>
        {/if}
      </div>
    {/if}
  {/if}

  {#if mode === 'agent'}
    <div class="feed-meta">
      <span class="meta-text">{rawFeed.length} total &bull; {filtered.length} líneas</span>
      {#if filterText}
        <span class="meta-filter">"{filterText}"</span>
      {/if}
    </div>

    {#if filtered.length === 0}
      <div class="empty-state">
        <span class="empty-icon">&#9685;</span>
        <span>Sin actividad{filterText ? ' (ninguna coincide)' : ' todavia'}</span>
      </div>
    {:else}
      <div class="feed-list" bind:this={scrollEl}>
        {#each filtered as item ((item.ts ?? 0) + item.type + (item.name ?? ''))}
          {@const nameStr = itemName(item)}
          {@const detailStr = itemDetail(item)}
          {@const levelCls = logLineClass(nameStr + ' ' + detailStr)}
          <div class="feed-item {levelCls}">
            <span class="feed-badge {typeBadge(item.type)}">{typeLabel(item.type)}</span>
            <span class="feed-name">{@html highlightText(nameStr, filterText.trim())}</span>
            {#if detailStr}
              <span class="feed-detail">{@html highlightText(detailStr, filterText.trim())}</span>
            {/if}
          </div>
        {/each}
      </div>
    {/if}
  {:else}
    <!-- App logs mode -->
    <div class="feed-meta">
      <span class="meta-text">{appLogs.length} total &bull; {filteredAppLogs.length} líneas</span>
      {#if filterText}
        <span class="meta-filter">"{filterText}"</span>
      {/if}
      {#if appLogsError}
        <span class="meta-error">{appLogsError}</span>
      {/if}
    </div>

    {#if filteredAppLogs.length === 0}
      <div class="empty-state">
        <span class="empty-icon">&#9685;</span>
        <span>{appLogsError ? 'Error cargando logs' : 'Sin logs de JARVIS'}{filterText ? ' (ninguna coincide)' : ''}</span>
      </div>
    {:else}
      <div class="feed-list" bind:this={scrollEl}>
        {#each filteredAppLogs as entry (entry.timestamp + entry.message)}
          {@const lvlCls = appLogLevelClass(entry.level)}
          <div class="feed-item app-log-item {lvlCls}">
            <span class="feed-badge badge-{entry.level}">{entry.level.toUpperCase()}</span>
            <span class="app-log-time">{formatAppLogTime(entry.timestamp)}</span>
            <span class="feed-name">{@html highlightText(entry.message, filterText.trim())}</span>
          </div>
        {/each}
      </div>
    {/if}
  {/if}
</div>

<style>
  .logs-panel {
    padding: 8px 14px;
    overflow: hidden;
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .logs-header {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    flex-shrink: 0;
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
  .logs-controls {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-left: auto;
    flex-wrap: wrap;
  }

  /* Mode toggle */
  .mode-toggle {
    display: flex;
    gap: 0;
  }
  .mode-btn {
    background: var(--bg-2);
    color: var(--text-2);
    border: 1px solid var(--border);
    padding: 3px 10px;
    font-size: 9px;
    font-family: var(--font-display);
    font-weight: 700;
    letter-spacing: 1px;
    cursor: pointer;
    transition: all 0.15s ease;
  }
  .mode-btn:first-child { border-radius: var(--radius) 0 0 var(--radius); }
  .mode-btn:last-child { border-radius: 0 var(--radius) var(--radius) 0; }
  .mode-btn.active {
    background: var(--text-2);
    color: var(--bg-0);
    border-color: var(--text-2);
  }

  /* Agent toggle */
  .agent-toggle {
    display: flex;
    gap: 0;
  }
  .agent-btn {
    background: var(--bg-2);
    color: var(--text-2);
    border: 1px solid var(--border);
    padding: 3px 10px;
    font-size: 9px;
    font-family: var(--font-display);
    font-weight: 700;
    letter-spacing: 1px;
    cursor: pointer;
    transition: all 0.15s ease;
  }
  .agent-btn:first-child { border-radius: var(--radius) 0 0 var(--radius); }
  .agent-btn:last-child { border-radius: 0 var(--radius) var(--radius) 0; }
  .agent-btn.active {
    background: var(--cyan);
    color: var(--bg-0);
    border-color: var(--cyan);
  }

  /* Filter input */
  .feed-filter {
    background: var(--bg-1);
    color: var(--text-0);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 3px 8px;
    font-family: var(--font-mono);
    font-size: 10px;
    width: 120px;
    transition: border-color 0.15s ease;
  }
  .feed-filter:focus {
    outline: none;
    border-color: var(--cyan);
  }

  /* Auto-scroll label */
  .autoscroll-label {
    display: flex;
    align-items: center;
    gap: 4px;
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 600;
    color: var(--text-2);
    cursor: pointer;
    letter-spacing: 0.5px;
  }
  .autoscroll-label input[type="checkbox"] {
    accent-color: var(--cyan);
    width: 12px;
    height: 12px;
    cursor: pointer;
  }

  /* Action button (copy) */
  .action-btn {
    background: var(--bg-2);
    color: var(--text-2);
    border: 1px solid var(--border);
    padding: 3px 8px;
    font-size: 9px;
    font-family: var(--font-display);
    font-weight: 600;
    letter-spacing: 0.5px;
    border-radius: var(--radius);
    cursor: pointer;
    transition: all 0.15s ease;
  }
  .action-btn:hover { background: var(--bg-3); border-color: var(--border-bright); color: var(--cyan); }

  /* Clear button */
  .clear-btn {
    background: var(--bg-2);
    color: var(--text-2);
    border: 1px solid var(--border);
    padding: 3px 8px;
    font-size: 9px;
    font-family: var(--font-display);
    font-weight: 600;
    letter-spacing: 0.5px;
    border-radius: var(--radius);
    cursor: pointer;
    transition: all 0.15s ease;
  }
  .clear-btn:hover { background: var(--bg-3); border-color: var(--border-bright); color: var(--red); }

  /* Level pills */
  .level-pills {
    display: flex;
    gap: 4px;
    flex-shrink: 0;
    flex-wrap: wrap;
  }
  .level-pill {
    background: var(--bg-2);
    color: var(--text-2);
    border: 1px solid var(--border);
    padding: 2px 8px;
    font-size: 9px;
    font-family: var(--font-display);
    font-weight: 600;
    letter-spacing: 0.5px;
    border-radius: 10px;
    cursor: pointer;
    transition: all 0.15s ease;
  }
  .level-pill.active { color: var(--bg-0); border-color: transparent; }
  .pill-all.active   { background: var(--text-2); }
  .pill-error.active { background: #ef5350; }
  .pill-warn.active  { background: #ffb74d; }
  .pill-info.active  { background: var(--cyan); }
  .pill-debug.active { background: var(--text-3); }

  /* Meta row */
  .feed-meta {
    display: flex;
    gap: 10px;
    align-items: center;
    flex-shrink: 0;
  }
  .meta-text {
    font-size: 9px;
    color: var(--text-3);
    font-family: var(--font-display);
  }
  .meta-filter {
    font-size: 9px;
    color: var(--amber);
    font-family: var(--font-mono);
    font-style: italic;
  }
  .meta-error {
    font-size: 9px;
    color: var(--red);
    font-family: var(--font-mono);
    font-style: italic;
  }

  /* Empty state */
  .empty-state {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    color: var(--text-3);
    font-size: 11px;
    font-style: italic;
  }
  .empty-icon {
    font-size: 24px;
    opacity: 0.4;
  }

  /* Feed list */
  .feed-list {
    flex: 1;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-height: 0;
  }

  /* Feed item */
  .feed-item {
    display: flex;
    align-items: baseline;
    gap: 6px;
    padding: 4px 8px;
    border-radius: var(--radius);
    background: var(--bg-1);
    border: 1px solid var(--border);
    min-width: 0;
    transition: background 0.1s ease;
  }
  .feed-item:hover {
    background: var(--bg-2);
  }

  /* Log level text colors */
  .log-error { color: #ef5350; border-left: 2px solid #ef5350; }
  .log-warn  { color: #ffb74d; border-left: 2px solid #ffb74d; }
  .log-info  { color: var(--cyan); }
  .log-debug { color: var(--text-3); }
  .log-default { color: var(--text-2); }

  /* Ensure feed-name inherits color from parent level class */
  .log-error .feed-name,
  .log-warn  .feed-name,
  .log-info  .feed-name,
  .log-debug .feed-name {
    color: inherit;
  }

  /* App log time */
  .app-log-time {
    font-family: var(--font-mono);
    font-size: 9px;
    color: var(--text-3);
    flex-shrink: 0;
    white-space: nowrap;
  }

  /* Badges */
  .feed-badge {
    font-family: var(--font-display);
    font-size: 8px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    padding: 1px 5px;
    border-radius: 3px;
    flex-shrink: 0;
  }
  .badge-tool {
    background: var(--cyan);
    color: var(--bg-0);
  }
  .badge-prompt {
    background: var(--amber);
    color: var(--bg-0);
  }
  .badge-text {
    background: var(--bg-3);
    color: var(--text-1);
    border: 1px solid var(--border);
  }
  .badge-warn {
    background: #ffb74d;
    color: var(--bg-0);
  }
  .badge-error {
    background: #ef5350;
    color: #fff;
  }
  .badge-info {
    background: var(--cyan);
    color: var(--bg-0);
  }
  .badge-debug {
    background: var(--bg-3);
    color: var(--text-2);
    border: 1px solid var(--border);
  }

  /* Name */
  .feed-name {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--text-1);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
    min-width: 0;
  }

  /* Detail */
  .feed-detail {
    font-size: 9px;
    color: var(--text-3);
    font-family: var(--font-mono);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 200px;
    flex-shrink: 0;
  }

  /* Search highlight */
  :global(.log-highlight) {
    background: rgba(255, 200, 0, 0.3);
    border-radius: 2px;
  }

  /* Stats bar */
  .stats-bar {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
    flex-wrap: wrap;
  }
  .stat-item {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 2px 8px;
    border-radius: 4px;
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.5px;
    border: 1px solid var(--border);
    background: var(--bg-2);
  }
  .stat-label {
    color: var(--text-3);
    text-transform: uppercase;
  }
  .stat-value {
    font-family: var(--font-mono);
    font-weight: 800;
  }
  .stat-total .stat-value { color: var(--text-1); }
  .stat-error { border-color: rgba(239,83,80,0.3); background: rgba(239,83,80,0.07); }
  .stat-error .stat-value { color: #ef5350; }
  .stat-warn  { border-color: rgba(255,183,77,0.3); background: rgba(255,183,77,0.07); }
  .stat-warn .stat-value  { color: #ffb74d; }
  .stat-info  { border-color: rgba(0,212,255,0.25); background: rgba(0,212,255,0.07); }
  .stat-info .stat-value  { color: var(--cyan); }
  .stat-debug { border-color: var(--border); }
  .stat-debug .stat-value { color: var(--text-3); }
</style>
