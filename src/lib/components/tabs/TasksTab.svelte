<script lang="ts">
  import { tasks } from '../../stores/tasks';
  import { getTaskHistory, countTaskHistory, searchTaskHistory, getTaskHistoryByMachine, clearTaskHistory, sendTaskGraph } from '../../api';
  import { addToast } from '../../stores/notifications';
  import { handleError } from '../../utils';
  import type { TaskHistoryEntry, TaskGraph } from '../../types';
  import { t, tr } from '$lib/i18n';
  import { exportCsv } from '../../utils';
  import { machines } from '../../stores/machines';

  let machineIds = $derived(Object.keys($machines));

  let expandedTasks = $state<Set<number>>(new Set());
  let expandedPrompts = $state<Set<number>>(new Set());
  let showGraphForm = $state(false);
  let graphJson = $state(JSON.stringify({
    nodes: [
      { id: "lint", target: "atlas", prompt: "Run linter and fix issues", dependsOn: [], onFailure: "stop" },
      { id: "test", target: "atlas", prompt: "Run tests", dependsOn: ["lint"], onFailure: "stop" },
      { id: "build", target: "pixel", prompt: "Build the frontend", dependsOn: [], onFailure: "stop" },
      { id: "deploy", target: "atlas", prompt: "Deploy if all checks passed", dependsOn: ["test", "build"], onFailure: "stop" }
    ]
  }, null, 2));
  let graphRunning = $state(false);

  // History section state
  let historyOpen = $state(false);
  let history = $state<TaskHistoryEntry[]>([]);
  let historyOffset = $state(0);
  let historyTotal = $state(0);
  let historyMachineFilter = $state('');
  let loadingHistory = $state(false);
  let historySearchResults = $state<TaskHistoryEntry[]>([]);
  let historySearchActive = $state(false);
  let showClearConfirm = $state(false);
  const HISTORY_PAGE_SIZE = 50;

  // Search & filter state
  let searchQuery = $state('');
  let statusFilter = $state('all');
  let targetFilter = $state('all');
  let sortOrder = $state<'newest' | 'oldest'>('newest');

  const statusOptions = [
    { value: 'all', key: 'tasks.all' },
    { value: 'running', key: 'tasks.running' },
    { value: 'pending', key: 'tasks.pending' },
    { value: 'done', key: 'tasks.done' },
    { value: 'error', key: 'tasks.error' },
    { value: 'killed', key: 'tasks.killed' },
    { value: 'timeout', key: 'tasks.timeout' },
  ];

  let targetOptions = $derived([
    { value: 'all', key: 'tasks.all' },
    ...machineIds.map(id => ({ value: id, key: `tasks.${id}` })),
    { value: 'both', key: 'tasks.both' },
  ]);

  let filteredTasks = $derived.by(() => {
    const q = searchQuery.toLowerCase().trim();
    let result = [...$tasks];

    // Search filter
    if (q) {
      result = result.filter((t) =>
        t.prompt.toLowerCase().includes(q) ||
        t.target.toLowerCase().includes(q) ||
        t.status.toLowerCase().includes(q) ||
        (t.output && t.output.toLowerCase().includes(q))
      );
    }

    // Status filter
    if (statusFilter !== 'all') {
      result = result.filter((t) => t.status === statusFilter);
    }

    // Target filter
    if (targetFilter !== 'all') {
      result = result.filter((t) => t.target === targetFilter);
    }

    // Sort
    result.sort((a, b) => {
      const aTime = a.startedAt ?? a.id;
      const bTime = b.startedAt ?? b.id;
      return sortOrder === 'newest' ? bTime - aTime : aTime - bTime;
    });

    return result;
  });

  let totalCount = $derived($tasks.length);

  // Displayed history entries: search results or regular history
  let displayedHistory = $derived(historySearchActive ? historySearchResults : history);

  // Search bar triggers history search when 3+ chars
  $effect(() => {
    const q = searchQuery.trim();
    if (historyOpen && q.length >= 3) {
      triggerHistorySearch(q);
    } else if (historyOpen && q.length < 3 && historySearchActive) {
      historySearchActive = false;
      historySearchResults = [];
    }
  });

  async function triggerHistorySearch(query: string) {
    historySearchActive = true;
    try {
      historySearchResults = await searchTaskHistory(query, 50);
    } catch (e) {
      addToast(handleError(e), 'error');
      historySearchResults = [];
    }
  }

  function statusBadgeClass(status: string): string {
    switch (status) {
      case 'running': return 'sb-running';
      case 'done': return 'sb-done';
      case 'error': case 'killed': return 'sb-error';
      case 'timeout': return 'sb-timeout';
      case 'pending': return 'sb-pending';
      default: return 'sb-pending';
    }
  }

  function targetBadgeClass(target: string): string {
    if (target === 'both') return 'tb-both';
    return `tb-${target}`;
  }

  function taskDuration(task: { startedAt?: number; finishedAt?: number }): string | null {
    if (!task.startedAt || !task.finishedAt) return null;
    const secs = Math.round((task.finishedAt - task.startedAt) / 1000);
    if (secs < 60) return `${secs}s`;
    const m = Math.floor(secs / 60);
    const s = secs % 60;
    return `${m}m${s}s`;
  }

  function toggleExpand(id: number) {
    const next = new Set(expandedTasks);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    expandedTasks = next;
  }

  function togglePromptExpand(id: number) {
    const next = new Set(expandedPrompts);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    expandedPrompts = next;
  }

  async function copyPrompt(prompt: string) {
    try {
      await navigator.clipboard.writeText(prompt);
      addToast(t('tasks.promptCopied'), 'success');
    } catch {
      addToast('Copy failed', 'error');
    }
  }

  function statusIcon(status: string): string {
    switch (status) {
      case 'running': return '▶';
      case 'done': return '✓';
      case 'error': case 'killed': return '✗';
      case 'timeout': return '✗';
      case 'pending': return '⏸';
      default: return '⏸';
    }
  }

  function handleSearchKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      searchQuery = '';
      (e.currentTarget as HTMLInputElement).blur();
    }
  }

  function getDisplay(output: string): string {
    const mi = output.indexOf('===PIXEL-TASK===');
    if (mi !== -1) return output.substring(0, mi).trim();
    return output;
  }

  function hasDeps(task: { dependsOn?: number[] }): boolean {
    return !!task.dependsOn && task.dependsOn.length > 0;
  }

  function conditionLabel(cond?: string): string {
    switch (cond) {
      case 'on_success': return t('tasks.ifOk');
      case 'on_failure': return t('tasks.ifFail');
      case 'always': return t('tasks.always');
      default: return '';
    }
  }

  function statusLabel(status: string): string {
    switch (status) {
      case 'running': return t('tasks.statusActive');
      case 'pending': return t('tasks.statusPending');
      case 'error': return t('tasks.statusError');
      case 'killed': return t('tasks.statusKilled');
      case 'timeout': return t('tasks.statusTimeout');
      default: return t('tasks.statusDone');
    }
  }

  async function loadHistory(append = false) {
    loadingHistory = true;
    try {
      let entries: TaskHistoryEntry[];
      const offset = append ? history.length : 0;
      if (historyMachineFilter) {
        entries = await getTaskHistoryByMachine(historyMachineFilter, HISTORY_PAGE_SIZE);
      } else {
        entries = await getTaskHistory(undefined, undefined, HISTORY_PAGE_SIZE, offset);
      }
      const total = await countTaskHistory(historyMachineFilter || undefined);
      historyTotal = total;
      if (append) {
        history = [...history, ...entries];
      } else {
        history = entries;
        historyOffset = 0;
      }
    } catch (e) {
      addToast(handleError(e), 'error');
      if (!append) { history = []; historyTotal = 0; }
    }
    loadingHistory = false;
  }

  async function loadMore() {
    historyOffset = history.length;
    await loadHistory(true);
  }

  async function onMachineFilterChange() {
    historySearchActive = false;
    historySearchResults = [];
    await loadHistory(false);
  }

  async function toggleHistory() {
    historyOpen = !historyOpen;
    if (historyOpen && history.length === 0) {
      await loadHistory(false);
    }
  }

  async function confirmClearHistory() {
    try {
      await clearTaskHistory();
      history = [];
      historyTotal = 0;
      historySearchResults = [];
      historySearchActive = false;
      showClearConfirm = false;
      addToast('History cleared', 'success');
    } catch (e) {
      addToast(handleError(e), 'error');
    }
  }

  function relaunch(entry: TaskHistoryEntry) {
    window.dispatchEvent(new CustomEvent('jarvis-relaunch', {
      detail: { target: entry.target, prompt: entry.prompt }
    }));
    addToast(t('tasks.promptCopied'), 'info');
  }

  function formatDuration(secs: number): string {
    if (secs < 60) return `${secs}s`;
    const m = Math.floor(secs / 60);
    const s = secs % 60;
    return `${m}m${s}s`;
  }

  function formatTime(ts: string): string {
    try {
      const d = new Date(ts);
      return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
    } catch { return ts; }
  }

  function exportTasks() {
    const rows = filteredTasks.map(t => ({
      id: t.id,
      target: t.target,
      status: t.status,
      prompt: t.prompt.slice(0, 100),
      timestamp: t.startedAt ? new Date(t.startedAt).toISOString() : '',
    }));
    exportCsv(rows, 'jarvis-tasks');
    addToast('Tasks exported', 'success');
  }

  async function runGraph() {
    if (graphRunning) return;
    let graph: TaskGraph;
    try {
      graph = JSON.parse(graphJson);
    } catch {
      addToast('Invalid JSON', 'error');
      return;
    }
    if (!graph.nodes || !Array.isArray(graph.nodes) || graph.nodes.length === 0) {
      addToast('Graph must have at least one node', 'error');
      return;
    }
    graphRunning = true;
    try {
      const ids = await sendTaskGraph(graph);
      addToast(`Graph started: ${ids.length} task(s) queued`, 'success');
      showGraphForm = false;
    } catch (e) {
      addToast(typeof e === 'string' ? e : String(e), 'error');
    } finally {
      graphRunning = false;
    }
  }
</script>

<div class="tasks-panel" role="region" aria-label={$tr('tab.tasks')}>
  <div class="tasks-header-bar">
    <span>{$tr('tab.tasks')}</span>
    <div class="tasks-header-actions">
      <button class="export-btn" onclick={exportTasks} disabled={filteredTasks.length === 0} title="Export as CSV">
        Export
      </button>
      <button class="graph-btn" onclick={() => showGraphForm = !showGraphForm} title="Run task graph (parallel + dependencies)">
        {showGraphForm ? 'Cancel' : 'Graph'}
      </button>
    </div>
  </div>

  {#if showGraphForm}
    <div class="graph-form">
      <div class="graph-form-header">
        <span class="graph-form-title">Task Graph</span>
        <span class="graph-form-hint">Tasks with no deps run in parallel. Deps start when all predecessors complete.</span>
      </div>
      <div class="graph-example-hint">
        Fields: <code>id</code>, <code>target</code>, <code>prompt</code>, <code>dependsOn</code> (array of ids), <code>onFailure</code> ("stop" | "continue")
      </div>
      <textarea
        class="graph-textarea"
        bind:value={graphJson}
        rows={14}
        spellcheck={false}
      ></textarea>
      <button class="graph-run-btn" onclick={runGraph} disabled={graphRunning}>
        {graphRunning ? 'Queuing...' : 'Run Graph'}
      </button>
    </div>
  {/if}

  <!-- Search bar -->
  <div class="search-bar">
    <input
      type="text"
      class="jarvis-input search-input"
      placeholder={$tr('tasks.searchPlaceholder')}
      bind:value={searchQuery}
      onkeydown={handleSearchKeydown}
    />
    <select class="jarvis-input sort-select" bind:value={sortOrder}>
      <option value="newest">{$tr('tasks.newest')}</option>
      <option value="oldest">{$tr('tasks.oldest')}</option>
    </select>
  </div>

  <!-- Filter chips -->
  <div class="filter-chips-section">
    <div class="chip-group">
      <span class="chip-label">{$tr('tasks.status')}</span>
      {#each statusOptions as opt}
        <button
          class="filter-chip"
          class:chip-active={statusFilter === opt.value}
          onclick={() => statusFilter = opt.value}
        >{$tr(opt.key)}</button>
      {/each}
    </div>
    <div class="chip-group">
      <span class="chip-label">{$tr('tasks.target')}</span>
      {#each targetOptions as opt}
        <button
          class="filter-chip"
          class:chip-active={targetFilter === opt.value}
          onclick={() => targetFilter = opt.value}
        >{$tr(opt.key)}</button>
      {/each}
    </div>
  </div>

  <!-- Result count -->
  <div class="filter-count">
    {$tr('tasks.showing')} {filteredTasks.length} {$tr('tasks.of')} {totalCount} {$tr('tasks.tasksCount')}
  </div>

  {#if filteredTasks.length === 0}
    <div class="tasks-empty">
      {#if totalCount === 0}
        {$tr('tasks.noTasks')}
      {:else}
        {$tr('tasks.noResults')}
      {/if}
    </div>
  {:else}
    {#each filteredTasks as task (task.id)}
      <div class="task-item" class:task-dep={hasDeps(task)}>
        {#if hasDeps(task)}
          <div class="dep-connector" title="Depende de #{task.dependsOn?.join(', #')}"></div>
        {/if}
        <div class="task-row">
          <!-- Target badge -->
          <span class="task-badge {targetBadgeClass(task.target)}">{task.target}</span>
          <!-- Prompt: truncate at 100 chars with toggle -->
          <span class="task-prompt-text">
            {#if task.prompt.length > 100 && !expandedPrompts.has(task.id)}
              {task.prompt.substring(0, 100)}<button class="prompt-expand-btn" type="button" onclick={() => togglePromptExpand(task.id)}>... ver más</button>
            {:else}
              {task.prompt}<button class="prompt-copy-btn" type="button" onclick={() => copyPrompt(task.prompt)} title="Copiar prompt">&#x2398;</button>
              {#if task.prompt.length > 100}
                <button class="prompt-expand-btn" type="button" onclick={() => togglePromptExpand(task.id)}>ver menos</button>
              {/if}
            {/if}
          </span>
          <span class="task-meta">
            {#if hasDeps(task)}
              {@const label = conditionLabel(task.runCondition)}
              {#if label}
                <span class="task-condition-badge {task.runCondition}">{label}</span>
              {/if}
              <span class="task-dep-label">dep #{task.dependsOn?.join(', #')}</span>
            {/if}
            {#if task.orchestrate}
              <span class="task-chain-label">
                {task.pixelTaskId ? `Atlas \u2192 Pixel #${task.pixelTaskId}` : $tr('tasks.orchestrating')}
              </span>
            {/if}
            {#if taskDuration(task)}
              <span class="task-duration">{taskDuration(task)}</span>
            {/if}
            <!-- Copy button (visible when prompt is short) -->
            {#if task.prompt.length <= 100}
              <button class="task-copy-btn" type="button" onclick={() => copyPrompt(task.prompt)} title="Copiar prompt">&#x2398;</button>
            {/if}
            <!-- Status icon + badge -->
            <span class="task-badge {statusBadgeClass(task.status)}">
              <span class="status-icon">{statusIcon(task.status)}</span>
              {statusLabel(task.status)}
            </span>
          </span>
        </div>
        {#if task.output && task.status === 'done'}
          {@const display = getDisplay(task.output)}
          <div class="task-output-block" class:expanded={expandedTasks.has(task.id)}>
            {expandedTasks.has(task.id) ? display : display.substring(0, 600)}
          </div>
          {#if display.length > 600}
            <button class="task-expand-btn" type="button" onclick={() => toggleExpand(task.id)}>
              {expandedTasks.has(task.id) ? $tr('tasks.collapse') : $tr('tasks.viewFull')}
            </button>
          {/if}
        {/if}
      </div>
    {/each}
  {/if}

  <!-- ── History section ──────────────────────────────────────────── -->
  <div class="history-section">
    <button class="history-section-header" onclick={toggleHistory} aria-expanded={historyOpen}>
      <span class="history-section-chevron" class:open={historyOpen}>▶</span>
      <span class="history-section-title">{$tr('tasks.history')}</span>
      {#if historyTotal > 0}
        <span class="history-count-badge">{historyTotal}</span>
      {/if}
      <span class="history-section-spacer"></span>
      {#if historyOpen}
        <!-- Machine filter -->
        <select
          class="history-machine-filter"
          bind:value={historyMachineFilter}
          onchange={onMachineFilterChange}
          onclick={(e) => e.stopPropagation()}
        >
          <option value="">{$tr('common.all')}</option>
          {#each machineIds as mid}
            <option value={mid}>{($machines[mid]?.name ?? mid).toUpperCase()}</option>
          {/each}
        </select>
        <!-- Clear history -->
        <button
          class="history-clear-btn"
          onclick={(e) => { e.stopPropagation(); showClearConfirm = true; }}
          title="Clear all history"
        >
          Clear
        </button>
      {/if}
    </button>

    {#if historyOpen}
      <!-- Clear confirmation dialog -->
      {#if showClearConfirm}
        <div class="clear-confirm-bar">
          <span class="clear-confirm-text">Delete all task history? This cannot be undone.</span>
          <button class="clear-confirm-yes" onclick={confirmClearHistory}>Yes, clear</button>
          <button class="clear-confirm-no" onclick={() => showClearConfirm = false}>Cancel</button>
        </div>
      {/if}

      {#if historySearchActive && searchQuery.trim().length >= 3}
        <div class="history-search-note">
          Search results for "<strong>{searchQuery.trim()}</strong>" — {historySearchResults.length} found
        </div>
      {/if}

      {#if loadingHistory && displayedHistory.length === 0}
        <div class="tasks-empty">{$tr('tasks.loadingHistory')}</div>
      {:else if displayedHistory.length === 0}
        <div class="tasks-empty">{$tr('tasks.noTasks')}</div>
      {:else}
        {#each displayedHistory as entry (entry.id)}
          <div class="history-item" class:history-item-fail={entry.status === 'fail'}>
            <div class="history-row">
              <span class="history-status-badge" class:hbadge-ok={entry.status === 'success'} class:hbadge-fail={entry.status === 'fail'}>
                {entry.status === 'success' ? '✓' : '✗'}
              </span>
              <span class="history-target-chip">{entry.target.toUpperCase()}</span>
              <span class="history-prompt-text" class:strikethrough={entry.status === 'fail'}>{entry.prompt.slice(0, 80)}{entry.prompt.length > 80 ? '…' : ''}</span>
              <span class="history-meta">
                <span class="history-duration">{formatDuration(entry.durationSecs)}</span>
                <span class="history-time">{formatTime(entry.timestamp)}</span>
              </span>
              <button class="relaunch-btn" onclick={() => relaunch(entry)} title={t('tasks.relaunch')}>↻</button>
            </div>
          </div>
        {/each}

        {#if !historySearchActive && history.length < historyTotal}
          <div class="history-load-more">
            <button class="load-more-btn" onclick={loadMore} disabled={loadingHistory}>
              {loadingHistory ? 'Loading…' : `Show more (${historyTotal - history.length} remaining)`}
            </button>
          </div>
        {/if}
      {/if}
    {/if}
  </div>
</div>

<style>
  .tasks-panel {
    background: var(--bg-0);
    overflow-y: auto;
    flex: 1;
  }
  .tasks-panel::-webkit-scrollbar { width: 3px; }
  .tasks-panel::-webkit-scrollbar-thumb { background: var(--border-bright); border-radius: 2px; }
  .tasks-header-bar {
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 700;
    color: var(--text-2);
    text-transform: uppercase;
    letter-spacing: 2px;
    padding: 8px 14px 4px;
    position: sticky;
    top: 0;
    background: var(--bg-0);
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 6px;
    z-index: 2;
  }
  .tasks-header-bar::before {
    content: '';
    width: 3px;
    height: 3px;
    border-radius: 50%;
    background: var(--cyan);
  }
  .tasks-header-actions { display: flex; gap: 4px; align-items: center; }
  .export-btn {
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    background: var(--bg-3);
    color: var(--text-2);
    border: 1px solid var(--border);
    border-radius: 3px;
    padding: 2px 8px;
    cursor: pointer;
    transition: background 0.15s, color 0.15s, border-color 0.15s;
  }
  .export-btn:hover { background: var(--bg-2); color: var(--text-1); border-color: var(--border-bright); }
  .export-btn:disabled { opacity: 0.4; cursor: default; }
  .tasks-empty { font-size: 10px; color: var(--text-3); font-style: italic; padding: 8px 14px; }

  /* ── Search bar ─────────────────────────── */
  .search-bar {
    display: flex;
    gap: 6px;
    padding: 6px 14px 2px;
    position: sticky;
    top: 24px;
    background: var(--bg-0);
    z-index: 1;
  }
  .search-input {
    flex: 1;
    min-width: 0;
  }
  .sort-select {
    width: auto;
    flex-shrink: 0;
    font-size: 10px;
    cursor: pointer;
  }

  /* ── Filter chips ───────────────────────── */
  .filter-chips-section {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 4px 14px 2px;
    position: sticky;
    top: 52px;
    background: var(--bg-0);
    z-index: 1;
  }
  .chip-group {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-wrap: wrap;
  }
  .chip-label {
    font-family: var(--font-display);
    font-size: 8px;
    font-weight: 700;
    color: var(--text-3);
    text-transform: uppercase;
    letter-spacing: 1px;
    margin-right: 2px;
    flex-shrink: 0;
  }
  .filter-chip {
    font-family: var(--font-display);
    font-size: 8px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.8px;
    padding: 2px 8px;
    border-radius: 10px;
    border: 1px solid var(--border);
    background: var(--bg-2);
    color: var(--text-2);
    cursor: pointer;
    transition: all 0.15s;
  }
  .filter-chip:hover {
    background: var(--bg-3);
    color: var(--text-1);
    border-color: var(--border-bright);
  }
  .filter-chip.chip-active {
    background: var(--cyan-dim);
    color: var(--cyan);
    border-color: var(--cyan);
  }

  /* ── Filter count ───────────────────────── */
  .filter-count {
    font-family: var(--font-display);
    font-size: 9px;
    color: var(--text-3);
    padding: 2px 14px 4px;
    letter-spacing: 0.5px;
  }

  /* ── Task items ─────────────────────────── */
  .task-item {
    padding: 6px 14px;
    border-bottom: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .task-item:hover { background: #ffffff03; }
  .task-row { display: flex; align-items: center; gap: 8px; }

  /* ── Unified badge (target + status) ────── */
  .task-badge {
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 700;
    padding: 2px 8px;
    border-radius: 3px;
    text-transform: uppercase;
    letter-spacing: 0.8px;
    flex-shrink: 0;
  }
  /* Target badges */
  .tb-atlas { background: #0a1a33; color: #7eb8ff; border: 1px solid #2196f322; }
  .tb-pixel { background: #0a2a1a; color: #7effa0; border: 1px solid #4caf5022; }
  .tb-both  { background: var(--cyan-dim); color: var(--cyan); border: 1px solid #00d4ff33; }
  /* Status badges */
  .sb-running {
    background: var(--amber-dim);
    color: var(--amber);
    border: 1px solid #ffb80033;
    animation: blink 1.5s infinite;
  }
  .sb-done {
    background: var(--green-dim);
    color: var(--green);
    border: 1px solid #00ff8833;
  }
  .sb-error {
    background: #ff333510;
    color: var(--red);
    border: 1px solid #ff335522;
  }
  .sb-timeout {
    background: #ffdd0010;
    color: #ffdd00;
    border: 1px solid #ffdd0022;
  }
  .sb-pending {
    background: #66666610;
    color: var(--text-3);
    border: 1px solid var(--border);
  }

  .task-prompt-text {
    color: var(--text-1);
    font-size: 11px;
    flex: 1;
    min-width: 0;
    word-break: break-word;
    line-height: 1.4;
  }
  .prompt-expand-btn {
    background: none;
    border: none;
    color: var(--cyan);
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 700;
    cursor: pointer;
    padding: 0 2px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    vertical-align: middle;
  }
  .prompt-expand-btn:hover { text-decoration: underline; }
  .prompt-copy-btn {
    background: none;
    border: none;
    color: var(--text-3);
    font-size: 11px;
    cursor: pointer;
    padding: 0 2px;
    vertical-align: middle;
    opacity: 0.6;
  }
  .prompt-copy-btn:hover { color: var(--cyan); opacity: 1; }
  .task-copy-btn {
    background: none;
    border: none;
    color: var(--text-3);
    font-size: 11px;
    cursor: pointer;
    padding: 1px 3px;
    border-radius: 2px;
    opacity: 0.6;
    flex-shrink: 0;
  }
  .task-copy-btn:hover { color: var(--cyan); opacity: 1; background: var(--bg-2); }
  .status-icon {
    font-size: 9px;
    margin-right: 2px;
  }
  .task-meta { display: flex; align-items: center; gap: 8px; flex-shrink: 0; }
  .task-duration {
    font-family: var(--font-mono);
    font-size: 9px;
    color: var(--text-3);
  }
  .task-chain-label {
    font-size: 10px;
    color: var(--amber);
    font-family: var(--font-display);
  }
  .task-dep {
    padding-left: 28px;
    position: relative;
  }
  .dep-connector {
    position: absolute;
    left: 14px;
    top: 0;
    bottom: 0;
    width: 8px;
    border-left: 2px solid var(--border-bright);
    border-bottom: 2px solid var(--border-bright);
    border-bottom-left-radius: 4px;
    pointer-events: none;
  }
  .task-dep-label {
    font-family: var(--font-mono);
    font-size: 9px;
    color: var(--text-3);
  }
  .task-condition-badge {
    font-family: var(--font-display);
    font-size: 8px;
    font-weight: 700;
    padding: 1px 6px;
    border-radius: 3px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }
  .task-condition-badge.on_success {
    background: var(--green-dim);
    color: var(--green);
    border: 1px solid #00ff8822;
  }
  .task-condition-badge.on_failure {
    background: #ff333510;
    color: var(--red);
    border: 1px solid #ff335522;
  }
  .task-condition-badge.always {
    background: var(--cyan-dim);
    color: var(--cyan);
    border: 1px solid #00d4ff22;
  }
  .task-output-block {
    color: var(--text-2);
    font-size: 11px;
    background: var(--bg-1);
    padding: 6px 10px;
    border-radius: 4px;
    max-height: 50px;
    overflow-y: auto;
    white-space: pre-wrap;
    line-height: 1.5;
    border: 1px solid var(--border);
  }
  .task-output-block.expanded { max-height: none; }
  .task-expand-btn {
    background: none;
    border: none;
    color: var(--cyan);
    font-family: var(--font-display);
    font-size: 9px;
    cursor: pointer;
    padding: 2px 0;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }
  .task-expand-btn:hover { text-decoration: underline; }

  /* ── Graph form ─────────────────────────── */
  .graph-btn {
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    background: var(--bg-3);
    color: var(--amber);
    border: 1px solid #ffb80033;
    border-radius: 3px;
    padding: 2px 8px;
    cursor: pointer;
    transition: background 0.15s, border-color 0.15s;
  }
  .graph-btn:hover { background: var(--amber-dim); border-color: #ffb80066; }

  .graph-form {
    background: var(--bg-1);
    border-bottom: 1px solid var(--border);
    padding: 10px 14px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .graph-form-header {
    display: flex;
    align-items: baseline;
    gap: 8px;
    flex-wrap: wrap;
  }
  .graph-form-title {
    font-family: var(--font-display);
    font-size: 10px;
    font-weight: 700;
    color: var(--amber);
    text-transform: uppercase;
    letter-spacing: 1px;
  }
  .graph-form-hint {
    font-size: 9px;
    color: var(--text-3);
  }
  .graph-example-hint {
    font-size: 9px;
    color: var(--text-3);
    font-family: var(--font-mono);
  }
  .graph-example-hint code {
    color: var(--cyan);
    background: var(--bg-2);
    padding: 0 3px;
    border-radius: 2px;
  }
  .graph-textarea {
    width: 100%;
    background: var(--bg-0);
    color: var(--text-1);
    border: 1px solid var(--border);
    border-radius: 4px;
    font-family: var(--font-mono);
    font-size: 10px;
    line-height: 1.5;
    padding: 8px;
    resize: vertical;
    box-sizing: border-box;
  }
  .graph-textarea:focus {
    outline: none;
    border-color: var(--amber);
  }
  .graph-run-btn {
    font-family: var(--font-display);
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 1px;
    background: var(--amber-dim);
    color: var(--amber);
    border: 1px solid #ffb80044;
    border-radius: 4px;
    padding: 5px 16px;
    cursor: pointer;
    align-self: flex-start;
    transition: background 0.15s, border-color 0.15s;
  }
  .graph-run-btn:hover:not(:disabled) { background: #ffb80022; border-color: #ffb80099; }
  .graph-run-btn:disabled { opacity: 0.5; cursor: default; }

  /* ── History section ────────────────────── */
  .history-section {
    border-top: 1px solid var(--border);
    margin-top: 4px;
  }

  .history-section-header {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 7px 14px;
    background: var(--bg-1);
    border: none;
    cursor: pointer;
    text-align: left;
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 700;
    color: var(--text-2);
    text-transform: uppercase;
    letter-spacing: 1.5px;
    transition: background 0.12s;
  }
  .history-section-header:hover { background: var(--bg-2); }

  .history-section-chevron {
    font-size: 8px;
    color: var(--text-3);
    transition: transform 0.15s;
    display: inline-block;
  }
  .history-section-chevron.open { transform: rotate(90deg); }

  .history-section-title { color: var(--text-2); }

  .history-count-badge {
    font-family: var(--font-mono);
    font-size: 9px;
    background: var(--bg-3);
    color: var(--cyan);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 0px 6px;
    min-width: 20px;
    text-align: center;
  }

  .history-section-spacer { flex: 1; }

  .history-machine-filter {
    font-family: var(--font-display);
    font-size: 9px;
    background: var(--bg-0);
    color: var(--text-1);
    border: 1px solid var(--border);
    border-radius: 3px;
    padding: 2px 4px;
    cursor: pointer;
  }

  .history-clear-btn {
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    background: var(--bg-3);
    color: var(--red);
    border: 1px solid #ff335522;
    border-radius: 3px;
    padding: 2px 8px;
    cursor: pointer;
    transition: background 0.12s, border-color 0.12s;
  }
  .history-clear-btn:hover { background: #ff333510; border-color: #ff335566; }

  /* Clear confirm bar */
  .clear-confirm-bar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 14px;
    background: #ff333508;
    border-bottom: 1px solid #ff335522;
  }
  .clear-confirm-text {
    font-size: 10px;
    color: var(--text-2);
    flex: 1;
  }
  .clear-confirm-yes {
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    background: #ff333520;
    color: var(--red);
    border: 1px solid #ff335544;
    border-radius: 3px;
    padding: 3px 10px;
    cursor: pointer;
  }
  .clear-confirm-yes:hover { background: #ff333530; }
  .clear-confirm-no {
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    background: var(--bg-3);
    color: var(--text-2);
    border: 1px solid var(--border);
    border-radius: 3px;
    padding: 3px 10px;
    cursor: pointer;
  }
  .clear-confirm-no:hover { background: var(--bg-2); }

  /* History search note */
  .history-search-note {
    font-size: 9px;
    color: var(--text-3);
    padding: 4px 14px;
    font-style: italic;
  }
  .history-search-note strong { color: var(--cyan); font-style: normal; }

  /* History entries — muted, compact, distinct from live tasks */
  .history-item {
    padding: 5px 14px;
    border-bottom: 1px solid var(--border);
    background: var(--bg-0);
    opacity: 0.82;
    transition: opacity 0.1s, background 0.1s;
  }
  .history-item:hover { opacity: 1; background: #ffffff02; }
  .history-item.history-item-fail { background: #ff333504; }

  .history-row {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 10px;
  }

  .history-status-badge {
    font-family: var(--font-mono);
    font-size: 9px;
    font-weight: 700;
    width: 16px;
    text-align: center;
    flex-shrink: 0;
    color: var(--text-3);
  }
  .hbadge-ok { color: var(--green); }
  .hbadge-fail { color: var(--red); }

  .history-target-chip {
    font-family: var(--font-display);
    font-size: 8px;
    font-weight: 700;
    color: var(--text-3);
    letter-spacing: 0.5px;
    flex-shrink: 0;
  }

  .history-prompt-text {
    flex: 1;
    font-size: 10px;
    color: var(--text-3);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  .history-prompt-text.strikethrough { text-decoration: line-through; opacity: 0.65; }

  .history-meta {
    display: flex;
    gap: 6px;
    align-items: center;
    flex-shrink: 0;
  }
  .history-duration {
    font-family: var(--font-mono);
    font-size: 9px;
    color: var(--text-3);
  }
  .history-time {
    font-size: 9px;
    color: var(--text-3);
    opacity: 0.7;
  }

  .relaunch-btn {
    background: none;
    border: 1px solid var(--border);
    color: var(--cyan);
    font-size: 12px;
    padding: 1px 5px;
    border-radius: 3px;
    cursor: pointer;
    flex-shrink: 0;
    opacity: 0.6;
  }
  .relaunch-btn:hover { background: var(--cyan-dim); border-color: #00d4ff44; opacity: 1; }

  /* Load more */
  .history-load-more {
    display: flex;
    justify-content: center;
    padding: 8px 14px;
    border-bottom: 1px solid var(--border);
  }
  .load-more-btn {
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    background: var(--bg-2);
    color: var(--text-2);
    border: 1px solid var(--border);
    border-radius: 3px;
    padding: 4px 14px;
    cursor: pointer;
    transition: background 0.12s, color 0.12s;
  }
  .load-more-btn:hover:not(:disabled) { background: var(--bg-3); color: var(--text-1); border-color: var(--border-bright); }
  .load-more-btn:disabled { opacity: 0.4; cursor: default; }
</style>
