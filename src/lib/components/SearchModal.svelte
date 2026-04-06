<script lang="ts">
  import { tick, onDestroy } from 'svelte';
  import { tr } from '$lib/i18n';
  import { tasks } from '$lib/stores/tasks';
  import { machines } from '$lib/stores/machines';
  import { sendTask } from '$lib/api';
  import { addToast } from '$lib/stores/notifications';

  interface Props {
    open: boolean;
    onClose: () => void;
    onSwitchTab: (tab: string) => void;
    onAction: (action: string) => void;
  }

  let { open, onClose, onSwitchTab, onAction }: Props = $props();
  let query = $state('');
  let selectedIndex = $state(0);
  let alive = true;
  let inputEl: HTMLInputElement | undefined = $state();
  let overlayEl: HTMLDivElement | undefined = $state();
  let listEl: HTMLDivElement | undefined = $state();

  // ── Quick command mode ────────────────────────────────────
  // If query starts with ">" interpret as a quick command
  let isCommandMode = $derived(query.startsWith('>'));

  // Parse ">atlas: Do something" or ">pixel: Do something"
  let quickCommand = $derived.by(() => {
    if (!isCommandMode) return null;
    const rest = query.slice(1).trim();
    const colonIdx = rest.indexOf(':');
    if (colonIdx === -1) return { target: '', prompt: rest };
    const target = rest.slice(0, colonIdx).trim().toLowerCase();
    const prompt = rest.slice(colonIdx + 1).trim();
    return { target, prompt };
  });

  // ── Static data ───────────────────────────────────────────
  const tabDefs = [
    { id: 'Commits', icon: '⊙' },
    { id: 'Diff', icon: '≠' },
    { id: 'GitHub', icon: '⑂' },
    { id: 'Tareas', icon: '▶' },
    { id: 'Pipelines', icon: '⟿' },
    { id: 'Maquinas', icon: '◈' },
    { id: 'Capacidades', icon: '⬡' },
    { id: 'Timeline', icon: '∿' },
    { id: 'Eventos', icon: '≡' },
    { id: 'Logs', icon: '▷' },
    { id: 'Costos', icon: '$' },
    { id: 'Ajustes', icon: '⊕' },
  ];

  const tabKeyMap: Record<string, string> = {
    'Commits': 'tab.commits',
    'Diff': 'tab.diff',
    'GitHub': 'tab.github',
    'Tareas': 'tab.tasks',
    'Pipelines': 'tab.pipelines',
    'Maquinas': 'tab.machines',
    'Capacidades': 'tab.capabilities',
    'Timeline': 'tab.timeline',
    'Eventos': 'tab.events',
    'Logs': 'tab.logs',
    'Costos': 'tab.costs',
    'Ajustes': 'tab.settings',
  };

  const shortcuts = [
    { keys: 'Ctrl+P', desc: 'Planning mode' },
    { keys: 'Ctrl+/', desc: 'Atajos de teclado' },
    { keys: 'Ctrl+K', desc: 'Busqueda rapida' },
    { keys: 'Esc', desc: 'Cerrar modal / cancelar' },
    { keys: '↑ ↓', desc: 'Navegar resultados' },
    { keys: 'Enter', desc: 'Ejecutar seleccionado' },
  ];

  type ItemType = 'tab' | 'action' | 'task-dispatch' | 'task-recent' | 'shortcut';

  interface SearchItem {
    type: ItemType;
    id: string;
    label: string;
    icon: string;
    sublabel?: string;
    taskTarget?: string;   // for task-dispatch
    taskPrompt?: string;   // for task-dispatch quick send
  }

  // ── Build items ───────────────────────────────────────────
  let allItems = $derived.by(() => {
    const t = $tr;
    const machineList = Object.values($machines).filter(m => m.enabled);

    // Tabs
    const tabItems: SearchItem[] = tabDefs.map(tab => ({
      type: 'tab' as const,
      id: tab.id,
      label: t(tabKeyMap[tab.id]),
      icon: tab.icon,
    }));

    // Static actions
    const actionItems: SearchItem[] = [
      { type: 'action', id: 'kill-all', label: t('cmd.killActiveAgents'), icon: '✕' },
      { type: 'action', id: 'git-pull', label: t('cmd.gitPullBoth'), icon: '↓' },
      { type: 'action', id: 'planning', label: t('cmd.planningMode'), icon: '◎' },
      { type: 'action', id: 'clear-history', label: t('cmd.clearHistory'), icon: '⊘' },
    ];

    // Machine task-dispatch actions
    const dispatchItems: SearchItem[] = machineList.map(m => ({
      type: 'task-dispatch' as const,
      id: `dispatch-${m.id}`,
      label: `Enviar tarea a ${m.name}`,
      icon: '▶',
      taskTarget: m.id,
    }));

    // Shortcut items
    const shortcutItems: SearchItem[] = shortcuts.map(s => ({
      type: 'shortcut' as const,
      id: `shortcut-${s.keys}`,
      label: s.desc,
      icon: '⌨',
      sublabel: s.keys,
    }));

    return { tabItems, actionItems, dispatchItems, shortcutItems };
  });

  // Recent tasks (last 5 by id desc)
  let recentTaskItems = $derived.by((): SearchItem[] => {
    const taskList = [...$tasks].sort((a, b) => b.id - a.id).slice(0, 5);
    return taskList.map(task => {
      const age = task.startedAt ? formatAge(task.startedAt) : '';
      return {
        type: 'task-recent' as const,
        id: `task-${task.id}`,
        label: task.prompt.length > 60 ? task.prompt.slice(0, 60) + '...' : task.prompt,
        icon: task.status === 'running' ? '●' : task.status === 'error' ? '✕' : '✓',
        sublabel: `#${task.id} ${task.target.toUpperCase()}${age ? ' · ' + age : ''}`,
        taskTarget: task.target,
        taskPrompt: task.prompt,
      };
    });
  });

  function formatAge(epochMs: number): string {
    const diff = Math.floor((Date.now() - epochMs * 1000) / 1000);
    if (diff < 60) return `hace ${diff}s`;
    if (diff < 3600) return `hace ${Math.floor(diff / 60)}m`;
    if (diff < 86400) return `hace ${Math.floor(diff / 3600)}h`;
    return `hace ${Math.floor(diff / 86400)}d`;
  }

  // ── Filtered groups ───────────────────────────────────────
  let filteredGroups = $derived.by(() => {
    if (isCommandMode) {
      // In command mode, show machine dispatch items filtered by target name
      const rest = query.slice(1).trim().toLowerCase();
      const machineList = Object.values($machines).filter(m => m.enabled);
      const dispatchItems: SearchItem[] = machineList
        .filter(m => rest === '' || m.id.toLowerCase().includes(rest) || m.name.toLowerCase().includes(rest))
        .map(m => ({
          type: 'task-dispatch' as const,
          id: `dispatch-${m.id}`,
          label: `Enviar tarea a ${m.name}`,
          icon: '▶',
          taskTarget: m.id,
        }));
      return { tabs: [], actions: [], dispatches: dispatchItems, recents: [], shortcuts: [] };
    }

    const q = query.toLowerCase().trim();

    function matches(item: SearchItem): boolean {
      if (!q) return true;
      return item.label.toLowerCase().includes(q) ||
             item.id.toLowerCase().includes(q) ||
             (item.sublabel?.toLowerCase().includes(q) ?? false);
    }

    const { tabItems, actionItems, dispatchItems, shortcutItems } = allItems;
    const tabs = tabItems.filter(matches);
    const actions = [...actionItems, ...dispatchItems].filter(matches);
    const recents = recentTaskItems.filter(matches);
    const scuts = shortcutItems.filter(matches);

    return { tabs, actions, dispatches: [], recents, shortcuts: scuts };
  });

  // Flat list for keyboard navigation (order: tabs, actions, recents, shortcuts)
  let flatList = $derived.by((): SearchItem[] => {
    const { tabs, actions, dispatches, recents, shortcuts: scuts } = filteredGroups;
    if (isCommandMode) return dispatches;
    return [...tabs, ...actions, ...recents, ...scuts];
  });

  // Global index helpers
  function groupStart(group: 'tabs' | 'actions' | 'recents' | 'shortcuts'): number {
    const { tabs, actions, recents } = filteredGroups;
    if (group === 'tabs') return 0;
    if (group === 'actions') return tabs.length;
    if (group === 'recents') return tabs.length + actions.length;
    return tabs.length + actions.length + recents.length;
  }

  // ── Effects ───────────────────────────────────────────────
  $effect(() => {
    // eslint-disable-next-line @typescript-eslint/no-unused-expressions
    query;
    selectedIndex = 0;
  });

  $effect(() => {
    if (open) {
      query = '';
      selectedIndex = 0;
      tick().then(() => { if (alive) inputEl?.focus(); });
    }
  });

  onDestroy(() => { alive = false; });

  // ── Keyboard handling ─────────────────────────────────────
  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      onClose();
      return;
    }
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      selectedIndex = Math.min(selectedIndex + 1, flatList.length - 1);
      scrollToSelected();
      return;
    }
    if (e.key === 'ArrowUp') {
      e.preventDefault();
      selectedIndex = Math.max(selectedIndex - 1, 0);
      scrollToSelected();
      return;
    }
    if (e.key === 'Enter') {
      e.preventDefault();
      if (isCommandMode && quickCommand && quickCommand.target && quickCommand.prompt) {
        executeQuickCommand(quickCommand.target, quickCommand.prompt);
      } else {
        const item = flatList[selectedIndex];
        if (item) selectItem(item);
      }
      return;
    }
  }

  function scrollToSelected() {
    tick().then(() => {
      if (!alive) return;
      const el = listEl?.querySelector(`[data-index="${selectedIndex}"]`);
      el?.scrollIntoView({ block: 'nearest' });
    });
  }

  async function executeQuickCommand(target: string, prompt: string) {
    // Find best matching machine
    const machineList = Object.values($machines).filter(m => m.enabled);
    const match = machineList.find(m =>
      m.id.toLowerCase() === target || m.name.toLowerCase() === target
    );
    if (!match) {
      addToast(`Maquina "${target}" no encontrada`, 'error');
      return;
    }
    onClose();
    try {
      await sendTask(match.id, prompt);
      addToast(`Tarea enviada a ${match.name}`, 'success');
    } catch (err) {
      addToast('Error: ' + (typeof err === 'string' ? err : String(err)), 'error');
    }
  }

  function selectItem(item: SearchItem) {
    if (item.type === 'tab') {
      onClose();
      onSwitchTab(item.id);
    } else if (item.type === 'action') {
      onClose();
      onAction(item.id);
    } else if (item.type === 'task-dispatch') {
      // Focus the input and pre-fill with ">machineid: "
      query = `>${item.taskTarget}: `;
      tick().then(() => { if (alive) inputEl?.focus(); });
    } else if (item.type === 'task-recent') {
      // Re-dispatch the same task prompt
      onClose();
      onAction(`redispatch:${item.taskTarget}:${item.taskPrompt}`);
    } else if (item.type === 'shortcut') {
      // Shortcuts are informational; just close
      onClose();
    }
  }

  function handleOverlayClick(e: MouseEvent) {
    if ((e.target as HTMLElement)?.classList?.contains('search-overlay')) {
      onClose();
    }
  }
</script>

{#if open}
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="search-overlay" role="dialog" aria-modal="true" aria-label={$tr('search.title')} tabindex="-1" onclick={handleOverlayClick} onkeydown={handleKeydown} bind:this={overlayEl}>
  <div class="search-modal">
    <div class="search-input-wrapper">
      <svg class="search-icon" width="16" height="16" viewBox="0 0 16 16" fill="none">
        <circle cx="6.5" cy="6.5" r="5" stroke="currentColor" stroke-width="1.5"/>
        <line x1="10" y1="10" x2="14.5" y2="14.5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
      </svg>
      <input
        type="text"
        class="search-input"
        class:command-mode={isCommandMode}
        placeholder={isCommandMode ? '>maquina: descripcion de la tarea...' : $tr('search.placeholder') + ' · > para comandos'}
        bind:value={query}
        bind:this={inputEl}
      />
      <kbd class="search-esc">Esc</kbd>
    </div>

    {#if isCommandMode}
      <div class="command-hint">
        {#if quickCommand && quickCommand.target && quickCommand.prompt}
          <span class="hint-ready">↵ Enviar "{quickCommand.prompt.slice(0, 40)}{quickCommand.prompt.length > 40 ? '...' : ''}" a <strong>{quickCommand.target}</strong></span>
        {:else}
          <span class="hint-tip">Escribe <code>&gt;atlas: ...</code> o <code>&gt;pixel: ...</code> para enviar una tarea rapida</span>
        {/if}
      </div>
    {/if}

    <div class="search-results" bind:this={listEl}>
      {#if isCommandMode}
        {@const dispatches = filteredGroups.dispatches}
        {#if dispatches.length > 0}
          <div class="search-group-label">⚡ COMANDO RAPIDO</div>
          {#each dispatches as item, i}
            <button
              type="button"
              class="search-item"
              class:selected={selectedIndex === i}
              data-index={i}
              onmouseenter={() => { selectedIndex = i; }}
              onclick={() => selectItem(item)}
            >
              <span class="search-item-icon task-dispatch">{item.icon}</span>
              <span class="search-item-label">{item.label}</span>
            </button>
          {/each}
        {:else}
          <div class="search-empty">Maquina no encontrada</div>
        {/if}
      {:else}
        {@const { tabs, actions, recents, shortcuts: scuts } = filteredGroups}
        {@const total = tabs.length + actions.length + recents.length + scuts.length}
        {#if total === 0}
          <div class="search-empty">{$tr('search.noResults')}</div>
        {:else}
          {#if tabs.length > 0}
            <div class="search-group-label">📋 {$tr('search.tabs')}</div>
            {#each tabs as item, i}
              {@const idx = groupStart('tabs') + i}
              <button
                type="button"
                class="search-item"
                class:selected={selectedIndex === idx}
                data-index={idx}
                onmouseenter={() => { selectedIndex = idx; }}
                onclick={() => selectItem(item)}
              >
                <span class="search-item-icon">{item.icon}</span>
                <span class="search-item-label">{item.label}</span>
              </button>
            {/each}
          {/if}

          {#if actions.length > 0}
            <div class="search-group-label">🔧 {$tr('search.actions')}</div>
            {#each actions as item, i}
              {@const idx = groupStart('actions') + i}
              <button
                type="button"
                class="search-item"
                class:selected={selectedIndex === idx}
                data-index={idx}
                onmouseenter={() => { selectedIndex = idx; }}
                onclick={() => selectItem(item)}
              >
                <span class="search-item-icon" class:task-dispatch={item.type === 'task-dispatch'}>{item.icon}</span>
                <span class="search-item-label">{item.label}</span>
              </button>
            {/each}
          {/if}

          {#if recents.length > 0}
            <div class="search-group-label">🕐 TAREAS RECIENTES</div>
            {#each recents as item, i}
              {@const idx = groupStart('recents') + i}
              <button
                type="button"
                class="search-item"
                class:selected={selectedIndex === idx}
                data-index={idx}
                onmouseenter={() => { selectedIndex = idx; }}
                onclick={() => selectItem(item)}
              >
                <span class="search-item-icon task-recent" class:running={item.icon === '●'} class:error={item.icon === '✕'}>{item.icon}</span>
                <span class="search-item-content">
                  <span class="search-item-label">{item.label}</span>
                  {#if item.sublabel}
                    <span class="search-item-sub">{item.sublabel}</span>
                  {/if}
                </span>
              </button>
            {/each}
          {/if}

          {#if scuts.length > 0}
            <div class="search-group-label">⌨ ATAJOS</div>
            {#each scuts as item, i}
              {@const idx = groupStart('shortcuts') + i}
              <button
                type="button"
                class="search-item shortcut-item"
                class:selected={selectedIndex === idx}
                data-index={idx}
                onmouseenter={() => { selectedIndex = idx; }}
                onclick={() => selectItem(item)}
              >
                <span class="search-item-label shortcut-label">{item.label}</span>
                <kbd class="shortcut-kbd">{item.sublabel}</kbd>
              </button>
            {/each}
          {/if}
        {/if}
      {/if}
    </div>
  </div>
</div>
{/if}

<style>
  .search-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.7);
    z-index: 2100;
    display: flex;
    justify-content: center;
    align-items: flex-start;
    padding-top: 20vh;
    backdrop-filter: blur(4px);
  }
  .search-modal {
    background: var(--bg-0);
    border: 1px solid var(--border-bright);
    border-radius: 12px;
    width: 520px;
    max-width: 90vw;
    max-height: 480px;
    display: flex;
    flex-direction: column;
    box-shadow: 0 24px 80px rgba(0, 0, 0, 0.6);
    overflow: hidden;
  }
  .search-input-wrapper {
    display: flex;
    align-items: center;
    padding: 12px 16px;
    gap: 10px;
    border-bottom: 1px solid var(--border);
  }
  .search-icon {
    flex-shrink: 0;
    color: var(--text-3);
  }
  .search-input {
    flex: 1;
    background: transparent;
    border: none;
    outline: none;
    color: var(--text-0);
    font-family: var(--font-mono);
    font-size: 14px;
    caret-color: var(--cyan);
  }
  .search-input.command-mode {
    color: var(--cyan);
  }
  .search-input::placeholder {
    color: var(--text-3);
  }
  .search-input:focus {
    outline: none;
  }
  .search-esc {
    background: var(--bg-3);
    color: var(--text-3);
    padding: 2px 6px;
    border-radius: 4px;
    font-family: var(--font-mono);
    font-size: 10px;
    border: 1px solid var(--border);
    flex-shrink: 0;
  }
  .command-hint {
    padding: 6px 16px;
    background: rgba(0, 255, 200, 0.04);
    border-bottom: 1px solid var(--border);
    font-family: var(--font-mono);
    font-size: 11px;
  }
  .hint-ready {
    color: var(--cyan);
  }
  .hint-tip {
    color: var(--text-3);
  }
  .hint-tip code {
    color: var(--cyan);
    background: var(--bg-3);
    padding: 0 4px;
    border-radius: 3px;
  }
  .search-results {
    flex: 1;
    overflow-y: auto;
    padding: 8px;
  }
  .search-group-label {
    font-family: var(--font-display);
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 1px;
    text-transform: uppercase;
    color: var(--text-3);
    padding: 8px 8px 4px;
  }
  .search-item {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
    border-radius: 6px;
    cursor: pointer;
    transition: background 0.1s ease;
    width: 100%;
    background: transparent;
    border: none;
    text-align: left;
  }
  .search-item:hover,
  .search-item.selected {
    background: var(--bg-2);
  }
  .search-item.selected {
    outline: 1px solid var(--cyan);
    outline-offset: -1px;
  }
  .search-item-icon {
    width: 22px;
    height: 22px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--bg-3);
    border-radius: 4px;
    font-family: var(--font-mono);
    font-size: 11px;
    font-weight: 700;
    color: var(--cyan);
    flex-shrink: 0;
  }
  .search-item-icon.task-dispatch {
    color: var(--green, #00ff88);
  }
  .search-item-icon.task-recent {
    color: var(--text-2);
  }
  .search-item-icon.task-recent.running {
    color: var(--cyan);
    animation: pulse 1.5s ease-in-out infinite;
  }
  .search-item-icon.task-recent.error {
    color: var(--red, #ff4444);
  }
  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.4; }
  }
  .search-item-content {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
    flex: 1;
  }
  .search-item-label {
    font-size: 13px;
    color: var(--text-1);
    font-family: var(--font-mono);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .search-item-sub {
    font-size: 10px;
    color: var(--text-3);
    font-family: var(--font-mono);
  }
  /* Shortcut items: label left, kbd right */
  .shortcut-item {
    justify-content: space-between;
  }
  .shortcut-label {
    color: var(--text-2);
    font-size: 12px;
  }
  .shortcut-kbd {
    background: var(--bg-3);
    color: var(--cyan);
    padding: 2px 7px;
    border-radius: 4px;
    font-family: var(--font-mono);
    font-size: 10px;
    border: 1px solid var(--border);
    white-space: nowrap;
    flex-shrink: 0;
  }
  .search-empty {
    padding: 24px 16px;
    text-align: center;
    color: var(--text-3);
    font-size: 13px;
    font-family: var(--font-mono);
  }
</style>
