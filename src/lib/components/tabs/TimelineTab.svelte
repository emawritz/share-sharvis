<script lang="ts">
  import { onMount } from 'svelte';
  import { fetchTimeline, getJarvisConfig } from '../../api';
  import { addToast } from '../../stores/notifications';
  import { handleError } from '../../utils';
  import { t, tr } from '$lib/i18n';
  import type { TimelineResponse, HeatmapEntry, TimelineError, FileChange } from '../../types';
  import Skeleton from '$lib/components/Skeleton.svelte';

  let target = $state('');
  let targets = $state<{id: string, name: string}[]>([]);
  let data = $state<TimelineResponse | null>(null);
  let loading = $state(false);

  // Search / filter state
  let searchRaw = $state('');
  let searchTerm = $state('');
  let searchDebounce: ReturnType<typeof setTimeout> | null = null;

  function onSearchInput(e: Event) {
    const val = (e.target as HTMLInputElement).value;
    searchRaw = val;
    if (searchDebounce) clearTimeout(searchDebounce);
    searchDebounce = setTimeout(() => { searchTerm = val.toLowerCase(); }, 200);
  }

  function fmtTokens(n: number): string {
    if (n >= 1000000) return (n / 1000000).toFixed(1) + 'M';
    if (n >= 1000) return (n / 1000).toFixed(0) + 'K';
    return String(n);
  }

  async function refresh() {
    loading = true;
    try {
      data = await fetchTimeline(target);
    } catch (e) {
      addToast(t('timeline.errorLoading') + ': ' + handleError(e), 'error');
      data = null;
    }
    loading = false;
  }

  function handleTargetChange() {
    refresh();
  }

  onMount(() => { loadTargets(); });

  async function loadTargets() {
    try {
      const cfg = await getJarvisConfig();
      targets = cfg.machines.filter(m => m.enabled).map(m => ({ id: m.id, name: m.name }));
      if (targets.length > 0 && !targets.find(t => t.id === target)) {
        target = targets[0].id;
      }
    } catch { /* ignore */ }
    refresh();
  }

  let summaryItems = $derived.by(() => {
    if (!data) return [];
    const s = data.summary || {};
    let toolCallsTotal: number | string = '-';
    if (s.toolCalls && typeof s.toolCalls === 'object') {
      toolCallsTotal = Object.values(s.toolCalls as Record<string, number>).reduce((a, b) => a + b, 0);
    } else if (typeof s.toolCalls === 'number') {
      toolCallsTotal = s.toolCalls;
    }
    return [
      { val: String(data.eventCount || 0), label: t('timeline.events'), isError: false },
      { val: s.totalTokens ? fmtTokens(s.totalTokens) : '-', label: t('timeline.tokens'), isError: false },
      { val: String(toolCallsTotal), label: t('timeline.tools'), isError: false },
      { val: s.durationHuman || (s.duration ? Math.round(s.duration / 1000) + 's' : '-'), label: t('timeline.duration'), isError: false },
      { val: String((data.errors || []).length), label: t('timeline.errors'), isError: (data.errors || []).length > 0 },
      { val: String((data.files || []).length), label: t('timeline.files'), isError: false }
    ];
  });

  let heatmap = $derived(data?.heatmap || []);
  let maxHeatCount = $derived(Math.max(...heatmap.map((h: HeatmapEntry) => h.count || 0), 1));

  // Filtered errors and files
  let filteredErrors = $derived.by(() => {
    const errs: TimelineError[] = data?.errors || [];
    if (!searchTerm) return errs;
    return errs.filter(err =>
      (err.error || '').toLowerCase().includes(searchTerm) ||
      (err.tool || '').toLowerCase().includes(searchTerm) ||
      (err.command || '').toLowerCase().includes(searchTerm) ||
      (err.timestamp || '').toLowerCase().includes(searchTerm)
    );
  });

  let filteredFiles = $derived.by(() => {
    const fs: FileChange[] = data?.files || [];
    if (!searchTerm) return fs;
    return fs.filter(f =>
      (f.path || '').toLowerCase().includes(searchTerm)
    );
  });

  // Visible event count for display
  let visibleCount = $derived(filteredErrors.length + filteredFiles.length);

  // --- Activity Heatmap by hour (computed client-side from heatmap data) ---
  // heatmap entries have `minute` (e.g. "14:32") and `count` fields
  let hourlyActivity = $derived.by(() => {
    if (!data?.heatmap) return new Array<number>(24).fill(0);
    const buckets = new Array<number>(24).fill(0);
    for (const entry of data.heatmap) {
      if (!entry.minute) continue;
      const parts = entry.minute.split(':');
      const hour = parseInt(parts[0], 10);
      if (!isNaN(hour) && hour >= 0 && hour < 24) {
        buckets[hour] += entry.count || 0;
      }
    }
    return buckets;
  });
  let hourlyMax = $derived(Math.max(...hourlyActivity, 1));

  // --- Event type filter ---
  const EVENT_TYPES = ['all', 'bash', 'read', 'write', 'edit', 'tool', 'error'] as const;
  type EventTypeFilter = typeof EVENT_TYPES[number];
  let eventTypeFilter = $state<EventTypeFilter>('all');

  // Filtered files by event type (read/write/edit)
  let filteredFilesByType = $derived.by(() => {
    const files = filteredFiles;
    if (eventTypeFilter === 'all') return files;
    if (eventTypeFilter === 'read') return files.filter(f => (f.reads || 0) > 0);
    if (eventTypeFilter === 'write') return files.filter(f => (f.writes || 0) > 0);
    if (eventTypeFilter === 'edit') return files.filter(f => (f.edits || 0) > 0);
    return files;
  });

  // Filtered errors by event type
  let filteredErrorsByType = $derived.by(() => {
    const errs = filteredErrors;
    if (eventTypeFilter === 'all') return errs;
    if (eventTypeFilter === 'error') return errs;
    if (eventTypeFilter === 'bash') return errs.filter(e => (e.tool || '').toLowerCase().includes('bash'));
    if (eventTypeFilter === 'tool') return errs;
    return errs;
  });

  // Tool calls breakdown from heatmap tools for filtering
  let toolCallBreakdown = $derived.by(() => {
    if (!data?.heatmap) return {} as Record<string, number>;
    const counts: Record<string, number> = {};
    for (const entry of data.heatmap) {
      if (!entry.tools) continue;
      for (const [tool, count] of Object.entries(entry.tools)) {
        counts[tool] = (counts[tool] || 0) + count;
      }
    }
    return counts;
  });

  function getErrorText(err: unknown): string {
    if (typeof err === 'string') return err;
    if (err && typeof err === 'object') {
      const obj = err as Record<string, unknown>;
      if (obj.error) {
        const text = String(obj.error).substring(0, 300);
        return obj.tool ? `[${obj.tool}] ${text}` : text;
      }
      if (obj.message) return String(obj.message);
      return JSON.stringify(err).substring(0, 200);
    }
    return String(err);
  }

  function getFileName(f: unknown): string {
    if (typeof f === 'string') return f;
    if (f && typeof f === 'object') {
      const obj = f as Record<string, unknown>;
      return String(obj.path || obj.file || '');
    }
    return '';
  }

  function getFileCount(f: unknown): number {
    if (f && typeof f === 'object') {
      return (f as Record<string, number>).count || 0;
    }
    return 0;
  }

  // CSV export
  function exportCSV() {
    if (!data) return;

    const machineName = targets.find(t => t.id === target)?.name || target;
    const rows: string[][] = [['timestamp', 'machine', 'type', 'content']];

    // Add error rows
    for (const err of (data.errors || [])) {
      const content = err.tool
        ? `[${err.tool}] ${err.command || ''}: ${err.error || ''}`
        : `${err.command || ''}: ${err.error || ''}`;
      rows.push([
        err.timestamp || '',
        machineName,
        'error',
        '"' + content.replace(/"/g, '""').replace(/\n/g, ' ') + '"'
      ]);
    }

    // Add file rows
    for (const f of (data.files || [])) {
      const content = `reads:${f.reads} edits:${f.edits} writes:${f.writes} total:${f.total}`;
      rows.push([
        '',
        machineName,
        'file',
        '"' + (f.path || '').replace(/"/g, '""') + ' — ' + content + '"'
      ]);
    }

    // Add heatmap rows
    for (const h of (data.heatmap || [])) {
      const tools = h.tools ? Object.entries(h.tools).map(([k, v]) => `${k}:${v}`).join(' ') : '';
      rows.push([
        h.minute || '',
        machineName,
        'activity',
        '"' + `count:${h.count || 0} ${tools}`.replace(/"/g, '""') + '"'
      ]);
    }

    const csv = rows.map(r => r.join(',')).join('\n');
    const blob = new Blob([csv], { type: 'text/csv' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `jarvis-timeline-${new Date().toISOString().slice(0, 10)}.csv`;
    a.click();
    URL.revokeObjectURL(url);
  }
</script>

<div class="timeline-panel">
  <div class="timeline-header">
    <div class="section-label">{$tr('tab.timeline')}</div>
    <select class="timeline-target-select" bind:value={target} onchange={handleTargetChange}>
      {#each targets as t}
        <option value={t.id}>{t.name}</option>
      {/each}
    </select>
    {#if data}
      <button class="export-btn" onclick={exportCSV} title="Exportar CSV">
        &#x2193; CSV
      </button>
    {/if}
  </div>

  {#if data}
    <div class="filter-bar">
      <input
        class="search-input"
        type="text"
        placeholder="Buscar en timeline..."
        value={searchRaw}
        oninput={onSearchInput}
      />
      <select
        class="event-type-select"
        bind:value={eventTypeFilter}
      >
        {#each EVENT_TYPES as et}
          <option value={et}>{et === 'all' ? 'Todos' : et}</option>
        {/each}
      </select>
      {#if searchTerm}
        <span class="event-count">{visibleCount} eventos</span>
      {:else}
        <span class="event-count">{(data.errors?.length || 0) + (data.files?.length || 0)} eventos</span>
      {/if}
    </div>
  {/if}

  {#if loading}
    <div class="timeline-summary">
      {#each [0, 1, 2, 3, 4, 5] as _}
        <div class="timeline-stat-card">
          <Skeleton width="60%" height="20px" />
          <Skeleton width="80%" height="10px" />
        </div>
      {/each}
    </div>
    <Skeleton width="100%" height="40px" variant="card" />
  {:else if data}
    <div class="timeline-summary">
      {#each summaryItems as item}
        <div class="timeline-stat-card">
          <div class="timeline-stat-val" style={item.isError ? 'color:var(--red)' : ''}>{item.val}</div>
          <div class="timeline-stat-label">{item.label}</div>
        </div>
      {/each}
    </div>

    {#if heatmap.length > 0}
      <div class="section-label">{$tr('tab.timeline')}</div>
      <div class="heatmap-bar">
        {#each heatmap as cell, idx}
          <div
            class="heatmap-cell"
            style="height:{Math.max(2, ((cell.count || 0) / maxHeatCount) * 100)}%"
            title="{cell.minute || idx}: {cell.count || 0} events"
          ></div>
        {/each}
      </div>
    {/if}

    <!-- Activity Heatmap by Hour -->
    {#if hourlyActivity.some(v => v > 0)}
      <div class="section-label">Actividad por hora</div>
      <div class="hour-heatmap-card">
        <div class="hour-heatmap-grid">
          {#each hourlyActivity as count, hour}
            {@const intensity = count > 0 ? Math.max(0.08, count / hourlyMax) : 0}
            <div
              class="hour-cell"
              class:hour-cell-active={count > 0}
              style="background: rgba(0, 212, 255, {intensity}); border-color: rgba(0, 212, 255, {Math.min(1, intensity * 2)})"
              title="{hour.toString().padStart(2, '0')}:00 — {count} eventos"
            >
              <span class="hour-cell-label">{hour.toString().padStart(2, '0')}</span>
              {#if count > 0}
                <span class="hour-cell-count">{count}</span>
              {/if}
            </div>
          {/each}
        </div>
        <div class="hour-heatmap-meta">
          <span class="hour-heatmap-peak">
            {#if hourlyMax > 0}
              {@const peakHour = hourlyActivity.indexOf(hourlyMax)}
              Pico: {peakHour.toString().padStart(2, '0')}:00 ({hourlyMax} eventos)
            {/if}
          </span>
        </div>
      </div>
    {/if}

    {#if filteredErrorsByType.length > 0}
      <div class="section-label error-label">{$tr('timeline.errors')} ({filteredErrorsByType.length})</div>
      <div class="timeline-errors">
        {#each filteredErrorsByType.slice(0, 10) as err}
          <div class="timeline-error">{getErrorText(err)}</div>
        {/each}
      </div>
    {:else if searchTerm && (data.errors || []).length > 0}
      <div class="no-results">Sin errores que coincidan con "{searchRaw}"</div>
    {/if}

    {#if filteredFilesByType.length > 0}
      <div class="section-label">{$tr('timeline.files')}</div>
      <div class="timeline-files-list">
        {#each filteredFilesByType.slice(0, 20) as f}
          {@const fName = getFileName(f)}
          {@const fCount = getFileCount(f)}
          <span class="timeline-file-chip" title={fName}>
            {fName.split('/').pop()}
            {#if fCount}
              <span class="timeline-file-count">x{fCount}</span>
            {/if}
          </span>
        {/each}
      </div>
    {:else if searchTerm && (data.files || []).length > 0}
      <div class="no-results">Sin archivos que coincidan con "{searchRaw}"</div>
    {/if}

    {#if !data.eventCount && filteredErrors.length === 0 && filteredFiles.length === 0 && !searchTerm}
      <div class="empty-state">
        <span class="empty-icon">&#x25CC;</span>
        <div class="empty-title">No activity recorded</div>
        <div class="empty-hint">Run tasks to see timeline data</div>
      </div>
    {/if}
  {:else}
    <div class="empty-state">
      <span class="empty-icon">&#x25CC;</span>
      <div class="empty-title">{$tr('timeline.errorLoading')}</div>
      <div class="empty-hint">No JSONL data found for this machine</div>
    </div>
  {/if}
</div>

<style>
  .timeline-panel {
    padding: 8px 14px;
    overflow-y: auto;
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .timeline-header {
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
  .error-label { color: var(--red); }
  .timeline-target-select {
    background: var(--bg-1);
    color: var(--text-0);
    border: 1px solid var(--border);
    padding: 4px 8px;
    border-radius: var(--radius);
    font-family: var(--font-display);
    font-size: 10px;
    cursor: pointer;
    margin-left: auto;
    -webkit-appearance: none;
    appearance: none;
  }
  .export-btn {
    background: var(--bg-2);
    color: var(--cyan);
    border: 1px solid var(--border);
    padding: 4px 10px;
    border-radius: var(--radius);
    font-family: var(--font-display);
    font-size: 10px;
    font-weight: 700;
    cursor: pointer;
    letter-spacing: 1px;
    transition: background 0.15s, border-color 0.15s;
    flex-shrink: 0;
  }
  .export-btn:hover {
    background: var(--bg-3);
    border-color: var(--cyan);
  }
  .filter-bar {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .search-input {
    flex: 1;
    background: var(--bg-1);
    color: var(--text-0);
    border: 1px solid var(--border);
    padding: 5px 10px;
    border-radius: var(--radius);
    font-size: 11px;
    font-family: var(--font-mono);
    outline: none;
    transition: border-color 0.15s;
  }
  .search-input::placeholder { color: var(--text-3); }
  .search-input:focus { border-color: var(--cyan); }
  .event-count {
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 700;
    color: var(--text-2);
    letter-spacing: 1px;
    white-space: nowrap;
    text-transform: uppercase;
  }
  .no-results {
    font-size: 10px;
    color: var(--text-3);
    font-style: italic;
    padding: 4px 2px;
  }
  .timeline-summary {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
    gap: 6px;
  }
  .timeline-stat-card {
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 8px 10px;
    text-align: center;
  }
  .timeline-stat-val {
    font-family: var(--font-display);
    font-size: 18px;
    font-weight: 700;
    color: var(--cyan);
  }
  .timeline-stat-label {
    font-size: 9px;
    color: var(--text-2);
    text-transform: uppercase;
    letter-spacing: 1px;
    margin-top: 2px;
  }
  .heatmap-bar {
    display: flex;
    align-items: flex-end;
    gap: 1px;
    height: 40px;
    flex-shrink: 0;
  }
  .heatmap-cell {
    flex: 1;
    min-width: 2px;
    background: var(--cyan);
    border-radius: 1px 1px 0 0;
    opacity: 0.7;
    transition: opacity 0.15s ease;
  }
  .heatmap-cell:hover { opacity: 1; }
  .timeline-errors {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .timeline-error {
    background: #ff335510;
    border: 1px solid #ff335522;
    border-radius: var(--radius);
    padding: 6px 10px;
    font-size: 11px;
    color: var(--red);
    line-height: 1.4;
  }
  .timeline-files-list {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .timeline-file-chip {
    background: var(--bg-3);
    border: 1px solid var(--border);
    border-radius: 3px;
    padding: 2px 8px;
    font-size: 10px;
    color: var(--text-1);
    font-family: var(--font-mono);
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .timeline-file-count {
    font-family: var(--font-display);
    font-size: 8px;
    font-weight: 700;
    color: var(--amber);
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
  }
  .empty-icon { font-size: 24px; opacity: 0.4; }
  .empty-title { font-size: 12px; font-weight: 600; color: var(--text-2); }
  .empty-hint { font-size: 10px; color: var(--text-3); }

  /* Event type filter select */
  .event-type-select {
    background: var(--bg-1);
    color: var(--text-0);
    border: 1px solid var(--border);
    padding: 4px 8px;
    border-radius: var(--radius);
    font-family: var(--font-display);
    font-size: 10px;
    cursor: pointer;
    -webkit-appearance: none;
    appearance: none;
    flex-shrink: 0;
  }
  .event-type-select:focus { outline: none; border-color: var(--cyan); }

  /* Hour heatmap */
  .hour-heatmap-card {
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 10px 14px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .hour-heatmap-grid {
    display: grid;
    grid-template-columns: repeat(12, 1fr);
    gap: 3px;
  }
  .hour-cell {
    border-radius: 3px;
    border: 1px solid var(--border);
    padding: 3px 2px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 1px;
    cursor: default;
    transition: border-color 0.15s, background 0.15s;
    min-height: 32px;
    justify-content: center;
  }
  .hour-cell-active { cursor: pointer; }
  .hour-cell-label {
    font-family: var(--font-display);
    font-size: 7px;
    font-weight: 700;
    color: var(--text-2);
    letter-spacing: 0.3px;
  }
  .hour-cell-count {
    font-family: var(--font-display);
    font-size: 8px;
    font-weight: 700;
    color: var(--cyan);
  }
  .hour-heatmap-meta {
    font-family: var(--font-mono);
    font-size: 9px;
    color: var(--text-3);
    display: flex;
    justify-content: flex-end;
  }
  .hour-heatmap-peak { color: var(--cyan); opacity: 0.8; }
</style>
