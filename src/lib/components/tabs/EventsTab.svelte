<script lang="ts">
  import { listen } from '@tauri-apps/api/event';
  import { onMount, tick } from 'svelte';
  import { t, tr } from '$lib/i18n';

  interface JarvisEvent {
    id: string;
    timestamp: number;
    type: 'task' | 'planning' | 'rule' | 'conflict' | 'error' | 'system';
    severity: 'info' | 'warning' | 'error' | 'success';
    title: string;
    detail?: string;
    machine?: string;
  }

  let events = $state<JarvisEvent[]>([]);
  let search = $state('');
  let typeFilter = $state<'all' | JarvisEvent['type']>('all');
  let severityFilter = $state<'all' | JarvisEvent['severity']>('all');
  let machineFilter = $state<string>('all');
  let expandedId = $state<string | null>(null);
  let autoScroll = $state(true);
  let eventListEl: HTMLDivElement | undefined = $state();

  const MAX_EVENTS = 500;

  function addEvent(evt: Omit<JarvisEvent, 'id' | 'timestamp'>) {
    const newEvent: JarvisEvent = {
      ...evt,
      id: crypto.randomUUID(),
      timestamp: Date.now(),
    };
    events = [newEvent, ...events].slice(0, MAX_EVENTS);
  }

  /** Unique machine ids found across all events */
  let knownMachines = $derived.by(() => {
    const seen = new Set<string>();
    for (const e of events) {
      if (e.machine) seen.add(e.machine);
    }
    return Array.from(seen).sort();
  });

  let filteredEvents = $derived.by(() => {
    let result = events;
    if (typeFilter !== 'all') {
      result = result.filter(e => e.type === typeFilter);
    }
    if (severityFilter !== 'all') {
      result = result.filter(e => e.severity === severityFilter);
    }
    if (machineFilter !== 'all') {
      result = result.filter(e => e.machine === machineFilter);
    }
    if (search.trim()) {
      const q = search.trim().toLowerCase();
      result = result.filter(e =>
        e.title.toLowerCase().includes(q) ||
        (e.detail && e.detail.toLowerCase().includes(q))
      );
    }
    return result;
  });

  function formatTime(ts: number): string {
    const d = new Date(ts);
    return d.toLocaleTimeString('es-AR', { hour: '2-digit', minute: '2-digit', second: '2-digit', hour12: false });
  }

  /** Format full timestamp: "Lunes 17 Mar 14:30:25" */
  function formatFullTime(ts: number): string {
    const d = new Date(ts);
    const weekday = d.toLocaleDateString('es-AR', { weekday: 'long' });
    const day = d.getDate();
    const month = d.toLocaleDateString('es-AR', { month: 'short' });
    const time = d.toLocaleTimeString('es-AR', { hour: '2-digit', minute: '2-digit', second: '2-digit', hour12: false });
    const capitalized = weekday.charAt(0).toUpperCase() + weekday.slice(1);
    return `${capitalized} ${day} ${month} ${time}`;
  }

  function toggleExpand(id: string) {
    expandedId = expandedId === id ? null : id;
  }

  function clearEvents() {
    events = [];
    expandedId = null;
  }

  let typeLabels = $derived<Record<string, string>>({
    all: $tr('events.all'), task: $tr('events.task'), planning: $tr('events.planning'), rule: $tr('events.rule'),
    conflict: $tr('events.conflict'), error: $tr('events.error'), system: $tr('events.system')
  });

  let severityLabels = $derived<Record<string, string>>({
    all: $tr('events.all'), info: $tr('events.info'), warning: $tr('events.warning'), error: $tr('events.error'), success: $tr('events.success')
  });

  $effect(() => {
    const _len = events.length;
    if (autoScroll && eventListEl) {
      tick().then(() => {
        if (eventListEl) {
          eventListEl.scrollTop = eventListEl.scrollHeight;
        }
      });
    }
  });

  onMount(() => {
    const unlisteners: Promise<() => void>[] = [];

    unlisteners.push(
      listen<{ id: number; target: string }>('task-started', (event) => {
        addEvent({
          type: 'task',
          severity: 'info',
          title: t('events.taskStartedIn') + ' ' + event.payload.target,
          detail: `Task ID: ${event.payload.id}`,
          machine: event.payload.target,
        });
      })
    );

    unlisteners.push(
      listen<{ id: number; target: string; output: string }>('task-done', (event) => {
        const isError = event.payload.output.toLowerCase().includes('error');
        addEvent({
          type: 'task',
          severity: isError ? 'error' : 'success',
          title: (isError ? t('events.taskFailed') : t('events.taskCompleted')) + ' ' + event.payload.target,
          detail: event.payload.output.slice(0, 500),
          machine: event.payload.target,
        });
      })
    );

    unlisteners.push(
      listen<Record<string, unknown>>('planning-update', (event) => {
        const payload = event.payload as { phase?: string; objetivo?: string };
        addEvent({
          type: 'planning',
          severity: 'info',
          title: `Planning: ${payload.phase || 'actualizado'}`,
          detail: payload.objetivo,
        });
      })
    );

    unlisteners.push(
      listen<{ rule: string; message: string }>('rule-alert', (event) => {
        addEvent({
          type: 'rule',
          severity: 'warning',
          title: t('events.ruleTriggered') + ' "' + event.payload.rule + '"',
          detail: event.payload.message,
        });
      })
    );

    unlisteners.push(
      listen<{ message: string }>('repo-conflict', (event) => {
        addEvent({
          type: 'conflict',
          severity: 'error',
          title: t('events.repoConflict'),
          detail: event.payload.message,
        });
      })
    );

    unlisteners.push(
      listen<{ target: string; reason: string }>('auto-routed', (event) => {
        addEvent({
          type: 'system',
          severity: 'info',
          title: t('events.autoAssigned') + ' ' + event.payload.target,
          detail: event.payload.reason,
          machine: event.payload.target,
        });
      })
    );

    // System boot event
    addEvent({
      type: 'system',
      severity: 'info',
      title: t('events.logStarted'),
    });

    return () => {
      unlisteners.forEach(p => p.then(fn => fn()).catch(() => {}));
    };
  });
</script>

<div class="events-tab">
  <!-- Toolbar -->
  <div class="toolbar">
    <input
      class="search jarvis-input"
      type="text"
      placeholder={$tr('common.search') + '...'}
      bind:value={search}
    />

    <div class="filters">
      <!-- Type filter pills -->
      <div class="chip-group">
        {#each Object.keys(typeLabels) as key (key)}
          <button
            class="chip"
            class:active={typeFilter === key}
            onclick={() => { typeFilter = key as typeof typeFilter; }}
          >{typeLabels[key]}</button>
        {/each}
      </div>

      <!-- Severity filter pills -->
      <div class="chip-group">
        {#each Object.keys(severityLabels) as key (key)}
          <button
            class="chip severity-chip"
            class:active={severityFilter === key}
            class:chip-info={key === 'info'}
            class:chip-warning={key === 'warning'}
            class:chip-error={key === 'error'}
            class:chip-success={key === 'success'}
            onclick={() => { severityFilter = key as typeof severityFilter; }}
          >{severityLabels[key]}</button>
        {/each}
      </div>

      <!-- Machine filter dropdown -->
      {#if knownMachines.length > 0}
        <select
          class="machine-select jarvis-input"
          bind:value={machineFilter}
        >
          <option value="all">{$tr('events.all')}</option>
          {#each knownMachines as m (m)}
            <option value={m}>{m}</option>
          {/each}
        </select>
      {/if}
    </div>

    <div class="toolbar-right">
      <span class="count">{filteredEvents.length} {$tr('events.eventsCount')}</span>
      <label class="auto-toggle">
        <input type="checkbox" bind:checked={autoScroll} /> {$tr('events.autoScroll')}
      </label>
      <button class="jarvis-btn" onclick={clearEvents}>{$tr('events.clear')}</button>
    </div>
  </div>

  <!-- Event List -->
  <div class="event-list" bind:this={eventListEl}>
    {#if filteredEvents.length === 0}
      <div class="empty-state">
        <span class="empty-icon">&#x25A1;</span>
        <div class="empty-title">{$tr('events.noEvents')}</div>
        <div class="empty-hint">{search || typeFilter !== 'all' || severityFilter !== 'all' || machineFilter !== 'all' ? $tr('events.filtersActive') : 'Events appear here as tasks run'}</div>
      </div>
    {:else}
      {#each filteredEvents as evt (evt.id)}
        <button
          class="event-row severity-{evt.severity}"
          class:expanded={expandedId === evt.id}
          onclick={() => toggleExpand(evt.id)}
        >
          <div class="event-main">
            <span class="timestamp">{formatTime(evt.timestamp)}</span>
            <span class="dot dot-{evt.severity}"></span>
            <span class="type-badge badge-{evt.type}">{evt.type}</span>
            <span class="title title-{evt.severity}">{evt.title}</span>
            {#if evt.machine}
              <span class="machine">{evt.machine}</span>
            {/if}
            <span class="expand-hint">{expandedId === evt.id ? '▾' : '▸'}</span>
          </div>

          {#if expandedId === evt.id}
            <div class="event-detail-panel">
              <!-- Timestamp row -->
              <div class="detail-row">
                <span class="detail-label">Fecha</span>
                <span class="detail-value detail-timestamp">{formatFullTime(evt.timestamp)}</span>
              </div>

              <!-- Machine badge -->
              {#if evt.machine}
                <div class="detail-row">
                  <span class="detail-label">Máquina</span>
                  <span class="detail-machine-badge">{evt.machine}</span>
                </div>
              {/if}

              <!-- Type + Severity badges -->
              <div class="detail-row">
                <span class="detail-label">Tipo</span>
                <span class="type-badge badge-{evt.type}">{evt.type}</span>
                <span class="sev-badge sev-{evt.severity}">{evt.severity}</span>
              </div>

              <!-- Full message -->
              {#if evt.detail}
                <div class="detail-row detail-row-block">
                  <span class="detail-label">Detalle</span>
                  <pre class="detail-pre severity-pre-{evt.severity}">{evt.detail}</pre>
                </div>
              {:else}
                <div class="detail-row">
                  <span class="detail-label">Detalle</span>
                  <span class="detail-value" style="opacity:0.4">—</span>
                </div>
              {/if}
            </div>
          {/if}
        </button>
      {/each}
    {/if}
  </div>
</div>

<style>
  .events-tab {
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
    flex-wrap: wrap;
  }

  .search {
    width: 160px;
    font-size: 11px;
  }

  .filters {
    display: flex;
    gap: 6px;
    flex: 1;
    min-width: 0;
    align-items: center;
    flex-wrap: wrap;
  }

  .chip-group {
    display: flex;
    gap: 2px;
  }

  .chip {
    padding: 2px 8px;
    font-size: 9px;
    font-family: var(--font-display);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    border: 1px solid var(--border);
    border-radius: 3px;
    background: var(--bg-1);
    color: var(--text-2);
    cursor: pointer;
    transition: all 0.15s;
  }

  .chip:hover {
    color: var(--text-1);
    border-color: var(--border-bright);
  }

  .chip.active {
    background: var(--cyan-dim);
    color: var(--cyan);
    border-color: var(--cyan);
  }

  /* Severity chip active states get appropriate colors */
  .chip-info.active { background: #0e3a4a; color: var(--cyan); border-color: var(--cyan); }
  .chip-warning.active { background: #3a2e00; color: var(--amber); border-color: var(--amber); }
  .chip-error.active { background: #3a0e0e; color: var(--red); border-color: var(--red); }
  .chip-success.active { background: #0e3a1a; color: var(--green); border-color: var(--green); }

  .machine-select {
    font-size: 10px;
    padding: 2px 6px;
    height: 22px;
    cursor: pointer;
  }

  .toolbar-right {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-left: auto;
  }

  .count {
    font-size: 10px;
    color: var(--text-2);
  }

  .auto-toggle {
    font-size: 9px;
    color: var(--text-2);
    display: flex;
    align-items: center;
    gap: 3px;
    cursor: pointer;
  }

  .auto-toggle input {
    accent-color: var(--cyan);
  }

  .event-list {
    flex: 1;
    overflow-y: auto;
    min-height: 0;
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

  .event-row {
    display: flex;
    flex-direction: column;
    width: 100%;
    padding: 4px 10px;
    border: none;
    border-bottom: 1px solid var(--border);
    background: var(--bg-1);
    cursor: pointer;
    text-align: left;
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-0);
    transition: background 0.1s;
    border-left: 3px solid transparent;
  }

  .event-row:hover {
    background: var(--bg-2);
  }

  .event-row.severity-info    { border-left-color: var(--cyan); }
  .event-row.severity-warning { border-left-color: var(--amber); }
  .event-row.severity-error   { border-left-color: var(--red); }
  .event-row.severity-success { border-left-color: var(--green); }

  .event-row.expanded {
    background: var(--bg-2);
  }

  .event-main {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 20px;
  }

  .timestamp {
    font-size: 10px;
    color: var(--text-2);
    flex-shrink: 0;
    font-variant-numeric: tabular-nums;
  }

  .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .dot-info    { background: var(--cyan); }
  .dot-warning { background: var(--amber); }
  .dot-error   { background: var(--red); }
  .dot-success { background: var(--green); }

  .type-badge {
    padding: 1px 6px;
    font-size: 8px;
    font-family: var(--font-display);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    border-radius: 3px;
    flex-shrink: 0;
  }

  .badge-task     { background: #2196f322; color: #64b5f6; }
  .badge-planning { background: #9c27b022; color: #ce93d8; }
  .badge-rule     { background: #ffb80022; color: var(--amber); }
  .badge-conflict { background: #ff335522; color: var(--red); }
  .badge-error    { background: #ff335522; color: var(--red); }
  .badge-system   { background: #4a5a6a22; color: var(--text-1); }

  .title {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* Color-coded titles by severity */
  .title-error   { color: var(--red); }
  .title-warning { color: var(--amber); }
  .title-success { color: var(--green); }
  .title-info    { color: var(--text-0); }

  .machine {
    font-size: 9px;
    color: var(--text-2);
    padding: 1px 5px;
    border: 1px solid var(--border);
    border-radius: 3px;
    flex-shrink: 0;
  }

  .expand-hint {
    font-size: 8px;
    color: var(--text-3);
    flex-shrink: 0;
    margin-left: auto;
  }

  /* ── Detail Panel ──────────────────────────────── */

  .event-detail-panel {
    display: flex;
    flex-direction: column;
    gap: 5px;
    padding: 8px 8px 8px 22px;
    border-top: 1px solid var(--border);
    margin-top: 4px;
    background: var(--bg-0, #0d1117);
    border-radius: 0 0 4px 4px;
  }

  .detail-row {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 10px;
  }

  .detail-row-block {
    align-items: flex-start;
    flex-direction: column;
    gap: 4px;
  }

  .detail-label {
    font-size: 9px;
    font-family: var(--font-display);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--text-3);
    min-width: 60px;
    flex-shrink: 0;
  }

  .detail-value {
    color: var(--text-1);
  }

  .detail-timestamp {
    color: var(--text-0);
    font-variant-numeric: tabular-nums;
    font-family: var(--font-mono);
    font-size: 10px;
  }

  .detail-machine-badge {
    padding: 2px 8px;
    font-size: 9px;
    font-family: var(--font-display);
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    border-radius: 4px;
    background: #1a3a5a;
    color: #64b5f6;
    border: 1px solid #1e4a7a;
  }

  .sev-badge {
    padding: 1px 6px;
    font-size: 8px;
    font-family: var(--font-display);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    border-radius: 3px;
  }

  .sev-info    { background: #0e3a4a; color: var(--cyan); border: 1px solid var(--cyan); }
  .sev-warning { background: #3a2e00; color: var(--amber); border: 1px solid var(--amber); }
  .sev-error   { background: #3a0e0e; color: var(--red); border: 1px solid var(--red); }
  .sev-success { background: #0e3a1a; color: var(--green); border: 1px solid var(--green); }

  .detail-pre {
    font-family: var(--font-mono);
    font-size: 10px;
    line-height: 1.5;
    white-space: pre-wrap;
    word-break: break-word;
    margin: 0;
    padding: 6px 10px;
    border-radius: 4px;
    background: #0d1117;
    border: 1px solid var(--border);
    color: var(--text-1);
    width: 100%;
    box-sizing: border-box;
    max-height: 200px;
    overflow-y: auto;
  }

  .severity-pre-error   { border-color: #3a1a1a; color: var(--red); }
  .severity-pre-warning { border-color: #3a2e00; color: var(--amber); }
  .severity-pre-success { color: var(--green); }
  .severity-pre-info    { color: var(--text-1); }
</style>
