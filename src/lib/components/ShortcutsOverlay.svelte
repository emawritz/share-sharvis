<script lang="ts">
  import { tick } from 'svelte';
  import { tr } from '$lib/i18n';

  interface Props {
    open: boolean;
    onClose: () => void;
  }

  let { open, onClose }: Props = $props();
  let overlayEl: HTMLDivElement | undefined = $state();
  let searchQuery = $state('');
  let searchInputEl: HTMLInputElement | undefined = $state();

  $effect(() => {
    if (open) {
      tick().then(() => {
        searchInputEl?.focus();
        overlayEl?.focus();
      });
    } else {
      searchQuery = '';
    }
  });

  interface ShortcutItem {
    keys: string[];
    desc: string;
  }

  interface ShortcutGroup {
    label: string;
    color: string;
    items: ShortcutItem[];
  }

  const ALL_GROUPS: ShortcutGroup[] = [
    {
      label: $tr('shortcuts.nav'),
      color: '#7eb8ff',
      items: [
        { keys: ['Ctrl', '1-0'], desc: $tr('shortcuts.switchTabs') },
        { keys: ['Ctrl', 'K'], desc: $tr('shortcuts.globalSearch') },
        { keys: ['Ctrl', 'P'], desc: $tr('shortcuts.openPlanning') },
      ],
    },
    {
      label: 'Tabs',
      color: '#a78bfa',
      items: [
        { keys: ['Ctrl', '1'], desc: 'Overview' },
        { keys: ['Ctrl', '2'], desc: 'Commits' },
        { keys: ['Ctrl', '3'], desc: 'Diff' },
        { keys: ['Ctrl', '4'], desc: 'GitHub' },
        { keys: ['Ctrl', '5'], desc: 'Tareas' },
        { keys: ['Ctrl', '6'], desc: 'Sesiones' },
        { keys: ['Ctrl', '7'], desc: 'Pipelines' },
      ],
    },
    {
      label: $tr('shortcuts.actions'),
      color: '#ffb74d',
      items: [
        { keys: ['Enter'], desc: $tr('cmd.sendTask') },
        { keys: ['F5'], desc: 'Refrescar máquinas' },
        { keys: ['Ctrl', 'Shift', 'K'], desc: $tr('shortcuts.killAll') },
        { keys: ['Ctrl', 'Shift', 'C'], desc: 'Limpiar feed' },
      ],
    },
    {
      label: $tr('shortcuts.view'),
      color: '#7effa0',
      items: [
        { keys: ['Ctrl', 'B'], desc: $tr('shortcuts.togglePanels') },
        { keys: ['Esc'], desc: $tr('shortcuts.closeModal') },
        { keys: ['Ctrl', '/'], desc: $tr('shortcuts.help') },
        { keys: ['?'], desc: $tr('shortcuts.help') },
      ],
    },
  ];

  let groups = $derived(
    searchQuery.trim()
      ? ALL_GROUPS.map(g => ({
          ...g,
          items: g.items.filter(item =>
            item.desc.toLowerCase().includes(searchQuery.toLowerCase()) ||
            item.keys.join('+').toLowerCase().includes(searchQuery.toLowerCase())
          ),
        })).filter(g => g.items.length > 0)
      : ALL_GROUPS
  );

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.stopPropagation();
      onClose();
    }
  }

  function handleOverlayClick(e: MouseEvent) {
    if ((e.target as HTMLElement)?.classList?.contains('shortcuts-overlay')) {
      onClose();
    }
  }
</script>

{#if open}
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="shortcuts-overlay" role="dialog" aria-modal="true" aria-label={$tr('shortcuts.title')} tabindex="-1" onclick={handleOverlayClick} onkeydown={handleKeydown} bind:this={overlayEl}>
  <div class="shortcuts-modal">
    <div class="shortcuts-header">
      <span class="shortcuts-title">{$tr('shortcuts.title')}</span>
      <button class="shortcuts-close" type="button" onclick={onClose} aria-label={$tr('common.close')}>&times;</button>
    </div>

    <!-- Search filter -->
    <div class="shortcuts-search-wrap">
      <span class="shortcuts-search-icon">⌕</span>
      <input
        class="shortcuts-search"
        type="text"
        placeholder="Buscar atajo..."
        bind:value={searchQuery}
        bind:this={searchInputEl}
        autocomplete="off"
        spellcheck={false}
      />
      {#if searchQuery}
        <button class="shortcuts-search-clear" onclick={() => searchQuery = ''} type="button">×</button>
      {/if}
    </div>

    <div class="shortcuts-body">
      {#each groups as group}
        <div class="shortcuts-group">
          <div class="shortcuts-group-label" style="color:{group.color}; border-color:{group.color}33">
            <span class="shortcuts-group-dot" style="background:{group.color}"></span>
            {group.label}
          </div>
          <div class="shortcuts-list">
            {#each group.items as shortcut}
              <div class="shortcut-row">
                <div class="shortcut-keys">
                  {#each shortcut.keys as key, i}
                    {#if i > 0}<span class="shortcut-plus">+</span>{/if}
                    <kbd>{key}</kbd>
                  {/each}
                </div>
                <span class="shortcut-desc">{shortcut.desc}</span>
              </div>
            {/each}
          </div>
        </div>
      {/each}

      {#if groups.length === 0}
        <div class="shortcuts-empty">Sin resultados para "{searchQuery}"</div>
      {/if}
    </div>

    <div class="shortcuts-footer">
      {$tr('shortcuts.macHint')}
    </div>
  </div>
</div>
{/if}

<style>
  .shortcuts-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.8);
    z-index: 2000;
    display: grid;
    place-items: center;
    backdrop-filter: blur(4px);
  }
  .shortcuts-modal {
    background: var(--bg-0);
    border: 1px solid var(--border-bright);
    border-radius: 12px;
    padding: 20px 24px;
    min-width: 380px;
    max-width: 460px;
    max-height: 80vh;
    box-shadow: 0 24px 80px rgba(0, 0, 0, 0.6);
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .shortcuts-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-shrink: 0;
  }
  .shortcuts-title {
    font-family: var(--font-display);
    font-size: 13px;
    font-weight: 700;
    letter-spacing: 1px;
    color: var(--text-0);
    text-transform: uppercase;
  }
  .shortcuts-close {
    background: none;
    border: none;
    color: var(--text-2);
    font-size: 18px;
    cursor: pointer;
    padding: 0 4px;
    line-height: 1;
    transition: color 0.15s ease;
  }
  .shortcuts-close:hover { color: var(--text-0); }

  /* Search */
  .shortcuts-search-wrap {
    position: relative;
    display: flex;
    align-items: center;
    flex-shrink: 0;
  }
  .shortcuts-search-icon {
    position: absolute;
    left: 10px;
    color: var(--text-3);
    font-size: 14px;
    pointer-events: none;
    line-height: 1;
  }
  .shortcuts-search {
    width: 100%;
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 7px 32px 7px 30px;
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--text-0);
    transition: border-color 0.15s, box-shadow 0.15s;
  }
  .shortcuts-search:focus {
    outline: none;
    border-color: var(--cyan);
    box-shadow: 0 0 0 2px rgba(0,212,255,0.12);
  }
  .shortcuts-search-clear {
    position: absolute;
    right: 8px;
    background: none;
    border: none;
    color: var(--text-3);
    font-size: 16px;
    cursor: pointer;
    line-height: 1;
    padding: 0 2px;
    transition: color 0.1s;
  }
  .shortcuts-search-clear:hover { color: var(--text-1); }

  /* Scrollable body */
  .shortcuts-body {
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-height: 0;
  }
  .shortcuts-body::-webkit-scrollbar { width: 4px; }
  .shortcuts-body::-webkit-scrollbar-thumb { background: var(--border-bright); border-radius: 2px; }

  .shortcuts-group {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .shortcuts-group-label {
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 1.5px;
    text-transform: uppercase;
    padding: 2px 8px 4px;
    border-bottom: 1px solid;
    margin-bottom: 2px;
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .shortcuts-group-dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .shortcuts-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .shortcut-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 5px 8px;
    border-radius: 6px;
    transition: background 0.1s ease;
  }
  .shortcut-row:hover { background: var(--bg-2); }
  .shortcut-keys {
    display: flex;
    align-items: center;
    gap: 3px;
    flex-shrink: 0;
  }
  .shortcut-keys kbd {
    background: var(--bg-3);
    color: var(--text-1);
    padding: 3px 8px;
    border-radius: 4px;
    font-family: var(--font-mono);
    font-size: 11px;
    font-weight: 600;
    border: 1px solid var(--border);
    box-shadow: 0 1px 0 rgba(0,0,0,0.4), inset 0 1px 0 rgba(255,255,255,0.06);
    min-width: 20px;
    text-align: center;
    display: inline-block;
  }
  .shortcut-plus {
    color: var(--text-3);
    font-size: 10px;
  }
  .shortcut-desc {
    font-size: 12px;
    color: var(--text-1);
    margin-left: 16px;
    flex: 1;
    text-align: right;
  }
  .shortcuts-empty {
    text-align: center;
    color: var(--text-3);
    font-size: 12px;
    padding: 20px;
    font-style: italic;
  }
  .shortcuts-footer {
    font-size: 10px;
    color: var(--text-3);
    text-align: center;
    padding-top: 4px;
    border-top: 1px solid var(--border);
    flex-shrink: 0;
  }
</style>
