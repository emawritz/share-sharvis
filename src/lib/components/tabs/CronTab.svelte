<script lang="ts">
  import { onMount } from 'svelte';
  import { getCrons, saveCron, deleteCron, toggleCron, validateCronExpr, getCronNextRuns } from '$lib/api';
  import type { CronJob } from '$lib/types';
  import ConfirmModal from '$lib/components/ConfirmModal.svelte';
  import { addToast } from '$lib/stores/notifications';
  import { machines } from '$lib/stores/machines';

  let machineIds = $derived(Object.keys($machines));

  let crons = $state<CronJob[]>([]);
  let loading = $state(true);
  let saving = $state(false);
  let error = $state('');
  let showForm = $state(false);
  let confirmDeleteId = $state<string | null>(null);

  // Search/filter
  let searchQuery = $state('');

  // Next-run data from backend (live, refreshed every 60s)
  let nextRunMap = $state<Record<string, string | null>>({});

  // Countdown ticker — increments every minute to force $derived recomputation
  let countdownTick = $state(0);

  // Form state
  let editingId = $state('');
  let formName = $state('');
  let formExpr = $state('0 9 * * *');
  let formTarget = $state('');
  let formPrompt = $state('');
  let formError = $state('');

  // Expression validation state
  let exprValid = $state<boolean | null>(null);   // null = not validated yet
  let exprHint = $state('');                       // human readable or error text
  let exprDebounceTimer = $state<ReturnType<typeof setTimeout> | null>(null);

  // Common expression shortcuts
  const EXPR_SHORTCUTS: { label: string; expr: string }[] = [
    { label: 'Every hour',        expr: '0 * * * *'   },
    { label: 'Daily at 9am',      expr: '0 9 * * *'   },
    { label: 'Every Monday',      expr: '0 9 * * 1'   },
    { label: 'Midnight daily',    expr: '0 0 * * *'   },
  ];

  // Derived: filtered cron list
  let filteredCrons = $derived(
    searchQuery.trim() === ''
      ? crons
      : crons.filter(j => {
          const q = searchQuery.toLowerCase();
          return j.name.toLowerCase().includes(q) || j.target.toLowerCase().includes(q);
        })
  );

  // Derived: live countdowns keyed by cron id (recomputes every tick)
  let countdowns = $derived(
    (() => {
      void countdownTick; // reactive dependency
      const result: Record<string, string> = {};
      for (const id of Object.keys(nextRunMap)) {
        result[id] = relativeTime(nextRunMap[id]);
      }
      return result;
    })()
  );

  function onExprInput() {
    if (exprDebounceTimer !== null) clearTimeout(exprDebounceTimer);
    exprValid = null;
    exprHint = '';
    exprDebounceTimer = setTimeout(async () => {
      const expr = formExpr.trim();
      if (!expr) return;
      try {
        const desc = await validateCronExpr(expr);
        exprValid = true;
        exprHint = desc;
      } catch (e) {
        exprValid = false;
        exprHint = typeof e === 'string' ? e : String(e);
      }
    }, 500);
  }

  function applyShortcut(expr: string) {
    formExpr = expr;
    onExprInput();
  }

  async function copyExpr(expr: string) {
    try {
      await navigator.clipboard.writeText(expr);
      addToast('Copied!', 'success');
    } catch {
      addToast('Copy failed', 'error');
    }
  }

  async function loadNextRuns() {
    try {
      const runs = await getCronNextRuns();
      const map: Record<string, string | null> = {};
      for (const r of runs) map[r.id] = r.nextRun;
      nextRunMap = map;
    } catch {
      // silently ignore — not critical
    }
  }

  async function load() {
    loading = true;
    error = '';
    try {
      crons = await getCrons();
      await loadNextRuns();
    } catch (e) {
      error = typeof e === 'string' ? e : String(e);
    } finally {
      loading = false;
    }
  }

  function openAddForm() {
    editingId = '';
    formName = '';
    formExpr = '0 9 * * *';
    formTarget = machineIds[0] ?? '';
    formPrompt = '';
    formError = '';
    exprValid = null;
    exprHint = '';
    showForm = true;
    // Validate the default expression immediately
    setTimeout(() => onExprInput(), 0);
  }

  function openEditForm(job: CronJob) {
    editingId = job.id;
    formName = job.name;
    formExpr = job.cronExpr;
    formTarget = job.target;
    formPrompt = job.prompt;
    formError = '';
    exprValid = null;
    exprHint = '';
    showForm = true;
    setTimeout(() => onExprInput(), 0);
  }

  function cancelForm() {
    showForm = false;
    formError = '';
    exprValid = null;
    exprHint = '';
  }

  async function submitForm() {
    formError = '';
    if (!formName.trim()) { formError = 'Name is required'; return; }
    if (!formPrompt.trim()) { formError = 'Prompt is required'; return; }
    if (!formExpr.trim()) { formError = 'Cron expression is required'; return; }

    saving = true;
    try {
      const job: CronJob = {
        id: editingId,
        name: formName.trim(),
        cronExpr: formExpr.trim(),
        target: formTarget,
        prompt: formPrompt.trim(),
        enabled: true,
        runCount: 0,
      };
      const saved = await saveCron(job);
      if (editingId) {
        crons = crons.map(c => c.id === saved.id ? saved : c);
        addToast('Cron job updated', 'success');
      } else {
        crons = [...crons, saved];
        addToast('Cron job created', 'success');
      }
      showForm = false;
      await loadNextRuns();
    } catch (e) {
      formError = typeof e === 'string' ? e : String(e);
    } finally {
      saving = false;
    }
  }

  async function handleToggle(job: CronJob) {
    try {
      await toggleCron(job.id, !job.enabled);
      crons = crons.map(c => c.id === job.id ? { ...c, enabled: !c.enabled } : c);
      await loadNextRuns();
    } catch (e) {
      addToast('Error: ' + (typeof e === 'string' ? e : String(e)), 'error');
    }
  }

  async function enableAll() {
    const disabled = crons.filter(c => !c.enabled);
    if (disabled.length === 0) return;
    try {
      await Promise.all(disabled.map(c => toggleCron(c.id, true)));
      crons = crons.map(c => ({ ...c, enabled: true }));
      await loadNextRuns();
      addToast(`Enabled ${disabled.length} cron${disabled.length > 1 ? 's' : ''}`, 'success');
    } catch (e) {
      addToast('Error: ' + (typeof e === 'string' ? e : String(e)), 'error');
    }
  }

  async function disableAll() {
    const enabled = crons.filter(c => c.enabled);
    if (enabled.length === 0) return;
    try {
      await Promise.all(enabled.map(c => toggleCron(c.id, false)));
      crons = crons.map(c => ({ ...c, enabled: false }));
      await loadNextRuns();
      addToast(`Disabled ${enabled.length} cron${enabled.length > 1 ? 's' : ''}`, 'success');
    } catch (e) {
      addToast('Error: ' + (typeof e === 'string' ? e : String(e)), 'error');
    }
  }

  async function handleDelete() {
    if (!confirmDeleteId) return;
    const id = confirmDeleteId;
    confirmDeleteId = null;
    try {
      await deleteCron(id);
      crons = crons.filter(c => c.id !== id);
      addToast('Cron job deleted', 'success');
    } catch (e) {
      addToast('Error: ' + (typeof e === 'string' ? e : String(e)), 'error');
    }
  }

  /** Format relative time: "in 3h 20m", "in 45m", "in 30s", etc. */
  function relativeTime(iso: string | null | undefined): string {
    if (!iso) return '—';
    try {
      const diff = new Date(iso).getTime() - Date.now();
      if (diff <= 0) return 'now';
      const totalSecs = Math.floor(diff / 1000);
      const h = Math.floor(totalSecs / 3600);
      const m = Math.floor((totalSecs % 3600) / 60);
      const s = totalSecs % 60;
      if (h > 0 && m > 0) return `in ${h}h ${m}m`;
      if (h > 0) return `in ${h}h`;
      if (m > 0) return `in ${m}m`;
      return `in ${s}s`;
    } catch {
      return '—';
    }
  }

  function formatLastRun(iso?: string): string {
    if (!iso) return 'Never';
    try {
      const d = new Date(iso);
      return d.toLocaleString('en-US', { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit', hour12: false });
    } catch {
      return iso;
    }
  }

  onMount(() => {
    load();
    // Refresh next-run times every 60s
    const interval = setInterval(loadNextRuns, 60_000);
    // Tick countdown every 60s so $derived recalculates
    const tickInterval = setInterval(() => { countdownTick += 1; }, 60_000);
    return () => {
      clearInterval(interval);
      clearInterval(tickInterval);
    };
  });
</script>

<div class="cron-tab">
  <!-- Toolbar -->
  <div class="toolbar">
    <span class="toolbar-title">Scheduled Tasks</span>
    <button class="jarvis-btn add-btn" onclick={openAddForm}>+ Add Cron</button>
    <button class="jarvis-btn bulk-btn" onclick={enableAll} title="Enable all crons">All On</button>
    <button class="jarvis-btn bulk-btn" onclick={disableAll} title="Disable all crons">All Off</button>
    <button class="jarvis-btn" onclick={load}>Refresh</button>
  </div>

  <!-- Search bar -->
  <div class="search-bar">
    <input
      class="jarvis-input search-input"
      type="text"
      placeholder="Filter by name or target..."
      bind:value={searchQuery}
    />
    {#if searchQuery}
      <button class="clear-btn" onclick={() => { searchQuery = ''; }} aria-label="Clear search">×</button>
    {/if}
  </div>

  <!-- Inline form -->
  {#if showForm}
    <div class="form-panel">
      <div class="form-title">{editingId ? 'Edit Cron Job' : 'New Cron Job'}</div>
      <div class="form-row">
        <label class="form-label" for="cron-name">Name</label>
        <input id="cron-name" class="jarvis-input form-input" type="text" placeholder="Daily standup" bind:value={formName} />
      </div>
      <div class="form-row">
        <label class="form-label" for="cron-expr">
          Expression
          <span class="form-hint">minute hour day month weekday</span>
        </label>
        <input id="cron-expr" class="jarvis-input form-input mono" type="text" placeholder="0 9 * * *" bind:value={formExpr} oninput={onExprInput} />
        {#if exprValid === true}
          <div class="expr-hint valid">{exprHint}</div>
        {:else if exprValid === false}
          <div class="expr-hint invalid">{exprHint}</div>
        {/if}
        <!-- Expression shortcut panel -->
        <div class="shortcut-panel">
          {#each EXPR_SHORTCUTS as s (s.expr)}
            <button
              class="shortcut-btn"
              onclick={() => applyShortcut(s.expr)}
              title={s.expr}
            >{s.label}</button>
          {/each}
        </div>
      </div>
      <div class="form-row">
        <label class="form-label" for="cron-target">Target</label>
        <select id="cron-target" class="jarvis-input form-input form-select" bind:value={formTarget}>
          {#each machineIds as mid}
            <option value={mid}>{$machines[mid]?.name ?? mid}</option>
          {/each}
          <option value="both">both</option>
        </select>
      </div>
      <div class="form-row">
        <label class="form-label" for="cron-prompt">Prompt</label>
        <textarea id="cron-prompt" class="jarvis-input form-textarea" placeholder="Summarize yesterday's commits and open PRs" bind:value={formPrompt}></textarea>
      </div>
      {#if formError}
        <div class="form-error">{formError}</div>
      {/if}
      <div class="form-actions">
        <button class="jarvis-btn primary" onclick={submitForm} disabled={saving}>
          {saving ? 'Saving...' : editingId ? 'Update' : 'Create'}
        </button>
        <button class="jarvis-btn" onclick={cancelForm}>Cancel</button>
      </div>
    </div>
  {/if}

  <!-- Content -->
  <div class="list-area">
    {#if loading}
      <div class="empty">Loading...</div>
    {:else if error}
      <div class="empty error-msg">{error}</div>
    {:else if crons.length === 0}
      <div class="empty">No scheduled tasks</div>
    {:else if filteredCrons.length === 0}
      <div class="empty">No crons match "{searchQuery}"</div>
    {:else}
      <div class="cron-list">
        {#each filteredCrons as job (job.id)}
          <div class="cron-row" class:disabled={!job.enabled}>
            <label class="toggle-wrap" title={job.enabled ? 'Enabled — click to disable' : 'Disabled — click to enable'}>
              <input type="checkbox" class="toggle" checked={job.enabled} onchange={() => handleToggle(job)} />
              <span class="toggle-track"></span>
            </label>
            <div class="cron-info">
              <div class="cron-name">{job.name}</div>
              <div class="cron-meta">
                <span class="cron-expr mono">{job.cronExpr}</span>
                <button
                  class="copy-btn"
                  onclick={() => copyExpr(job.cronExpr)}
                  title="Copy expression"
                  aria-label="Copy cron expression"
                >Copy</button>
                <span class="cron-target badge-target">{job.target}</span>
                <span class="cron-next">Next: {countdowns[job.id] ?? relativeTime(job.nextRun)}</span>
                <span class="cron-last">Last: {formatLastRun(job.lastRun)}</span>
                <span class="cron-count">Runs: {job.runCount}</span>
              </div>
              <div class="cron-prompt">{job.prompt.slice(0, 120)}{job.prompt.length > 120 ? '...' : ''}</div>
            </div>
            <div class="cron-actions">
              <button class="action-btn edit-btn" onclick={() => openEditForm(job)} title="Edit">Edit</button>
              <button class="action-btn del-btn" onclick={() => { confirmDeleteId = job.id; }} title="Delete">Del</button>
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>

<ConfirmModal
  open={confirmDeleteId !== null}
  title="Delete Cron Job"
  message="This scheduled task will be permanently removed."
  confirmText="Delete"
  cancelText="Cancel"
  variant="danger"
  onConfirm={handleDelete}
  onCancel={() => { confirmDeleteId = null; }}
/>

<style>
  .cron-tab {
    display: flex;
    flex-direction: column;
    height: 100%;
    width: 100%;
    overflow: hidden;
  }

  .toolbar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }

  .toolbar-title {
    font-family: var(--font-display);
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 1px;
    color: var(--text-1);
    text-transform: uppercase;
    flex: 1;
  }

  .add-btn {
    color: var(--cyan);
    border-color: rgba(0, 212, 255, 0.3);
    background: rgba(0, 212, 255, 0.06);
  }
  .add-btn:hover {
    background: rgba(0, 212, 255, 0.12);
    border-color: rgba(0, 212, 255, 0.5);
  }

  .bulk-btn {
    font-size: 8px;
    padding: 2px 6px;
    color: var(--text-2);
  }
  .bulk-btn:hover {
    color: var(--cyan);
    border-color: rgba(0, 212, 255, 0.3);
    background: rgba(0, 212, 255, 0.06);
  }

  /* Search bar */
  .search-bar {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 5px 10px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
    background: var(--bg-1);
  }

  .search-input {
    flex: 1;
    font-size: 11px;
    padding: 3px 8px;
  }

  .clear-btn {
    background: none;
    border: none;
    color: var(--text-2);
    cursor: pointer;
    font-size: 14px;
    line-height: 1;
    padding: 0 2px;
    opacity: 0.6;
  }
  .clear-btn:hover { opacity: 1; }

  /* Form panel */
  .form-panel {
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
    background: var(--bg-2, #0d1a24);
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .form-title {
    font-family: var(--font-display);
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.5px;
    color: var(--cyan);
    text-transform: uppercase;
  }

  .form-row {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .form-label {
    font-size: 10px;
    color: var(--text-2);
    font-family: var(--font-display);
    font-weight: 600;
    letter-spacing: 0.3px;
    display: flex;
    align-items: baseline;
    gap: 8px;
  }

  .form-hint {
    font-family: var(--font-mono);
    font-size: 9px;
    color: var(--text-2);
    opacity: 0.6;
    font-weight: 400;
  }

  .form-input {
    font-size: 11px;
    padding: 4px 8px;
    width: 100%;
    box-sizing: border-box;
  }

  .form-select {
    cursor: pointer;
  }

  .form-textarea {
    font-size: 11px;
    padding: 4px 8px;
    width: 100%;
    box-sizing: border-box;
    min-height: 56px;
    resize: vertical;
    font-family: var(--font-mono);
  }

  .mono { font-family: var(--font-mono); }

  .form-error {
    color: var(--red, #f43f5e);
    font-size: 10px;
    padding: 3px 0;
  }

  .expr-hint {
    font-size: 10px;
    font-family: var(--font-mono);
    padding: 2px 0;
  }

  .expr-hint.valid {
    color: #4ade80;
  }

  .expr-hint.invalid {
    color: var(--red, #f43f5e);
  }

  /* Shortcut panel */
  .shortcut-panel {
    display: flex;
    gap: 4px;
    flex-wrap: wrap;
    margin-top: 2px;
  }

  .shortcut-btn {
    font-family: var(--font-display);
    font-size: 8px;
    font-weight: 600;
    letter-spacing: 0.3px;
    text-transform: uppercase;
    padding: 2px 6px;
    border-radius: 3px;
    border: 1px solid rgba(126, 184, 255, 0.2);
    background: rgba(126, 184, 255, 0.05);
    color: #7eb8ff;
    cursor: pointer;
    transition: all 0.1s;
  }
  .shortcut-btn:hover {
    background: rgba(126, 184, 255, 0.14);
    border-color: rgba(126, 184, 255, 0.4);
  }

  .form-actions {
    display: flex;
    gap: 6px;
  }

  .primary {
    color: var(--cyan);
    border-color: rgba(0, 212, 255, 0.4);
    background: rgba(0, 212, 255, 0.1);
  }
  .primary:hover:not(:disabled) {
    background: rgba(0, 212, 255, 0.18);
  }
  .primary:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  /* List area */
  .list-area {
    flex: 1;
    overflow-y: auto;
    min-height: 0;
  }

  .list-area::-webkit-scrollbar { width: 2px; }
  .list-area::-webkit-scrollbar-thumb { background: rgba(255,255,255,0.1); border-radius: 1px; }

  .empty {
    padding: 24px;
    text-align: center;
    color: var(--text-2);
    font-size: 11px;
  }

  .error-msg { color: var(--red, #f43f5e); }

  .cron-list {
    display: flex;
    flex-direction: column;
  }

  .cron-row {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 7px 10px;
    border-bottom: 1px solid var(--border);
    transition: background 0.1s;
  }

  .cron-row:hover { background: var(--bg-2, rgba(255,255,255,0.02)); }
  .cron-row.disabled { opacity: 0.5; }

  /* Toggle switch */
  .toggle-wrap {
    display: flex;
    align-items: center;
    margin-top: 2px;
    cursor: pointer;
    flex-shrink: 0;
  }

  .toggle {
    display: none;
  }

  .toggle-track {
    width: 24px;
    height: 12px;
    border-radius: 6px;
    background: #2a3a4a;
    border: 1px solid rgba(255,255,255,0.1);
    position: relative;
    transition: background 0.2s, border-color 0.2s;
    display: block;
  }

  .toggle-track::after {
    content: '';
    position: absolute;
    top: 1px;
    left: 1px;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: #5a7a8a;
    transition: transform 0.2s, background 0.2s;
  }

  .toggle:checked + .toggle-track {
    background: rgba(0, 212, 255, 0.2);
    border-color: rgba(0, 212, 255, 0.4);
  }

  .toggle:checked + .toggle-track::after {
    transform: translateX(12px);
    background: var(--cyan);
  }

  .cron-info {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .cron-name {
    font-family: var(--font-display);
    font-size: 11px;
    font-weight: 600;
    color: var(--text-0);
    letter-spacing: 0.3px;
  }

  .cron-meta {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .cron-expr {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--cyan);
    background: rgba(0, 212, 255, 0.06);
    padding: 1px 5px;
    border-radius: 3px;
    border: 1px solid rgba(0, 212, 255, 0.15);
  }

  /* Copy expression button */
  .copy-btn {
    font-family: var(--font-display);
    font-size: 7px;
    font-weight: 700;
    letter-spacing: 0.4px;
    text-transform: uppercase;
    padding: 1px 4px;
    border-radius: 2px;
    border: 1px solid rgba(0, 212, 255, 0.15);
    background: rgba(0, 212, 255, 0.04);
    color: var(--text-2);
    cursor: pointer;
    transition: all 0.1s;
    line-height: 1.4;
  }
  .copy-btn:hover {
    color: var(--cyan);
    border-color: rgba(0, 212, 255, 0.4);
    background: rgba(0, 212, 255, 0.1);
  }

  .badge-target {
    font-family: var(--font-display);
    font-size: 8px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    padding: 1px 5px;
    border-radius: 3px;
    background: rgba(126, 184, 255, 0.1);
    color: #7eb8ff;
    border: 1px solid rgba(126, 184, 255, 0.2);
  }

  .cron-next,
  .cron-last,
  .cron-count {
    font-size: 9px;
    color: var(--text-2);
  }

  .cron-prompt {
    font-size: 10px;
    color: var(--text-2);
    font-family: var(--font-mono);
    line-height: 1.4;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .cron-actions {
    display: flex;
    gap: 4px;
    flex-shrink: 0;
    align-items: flex-start;
    margin-top: 2px;
  }

  .action-btn {
    font-family: var(--font-display);
    font-size: 8px;
    font-weight: 600;
    letter-spacing: 0.5px;
    text-transform: uppercase;
    padding: 2px 6px;
    border-radius: 3px;
    border: 1px solid var(--border);
    background: var(--bg-1);
    color: var(--text-2);
    cursor: pointer;
    transition: all 0.1s;
  }

  .edit-btn:hover {
    color: var(--cyan);
    border-color: rgba(0, 212, 255, 0.3);
    background: rgba(0, 212, 255, 0.06);
  }

  .del-btn:hover {
    color: var(--red, #f43f5e);
    border-color: rgba(244, 63, 94, 0.3);
    background: rgba(244, 63, 94, 0.06);
  }
</style>
