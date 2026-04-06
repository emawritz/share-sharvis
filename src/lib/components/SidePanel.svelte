<script lang="ts">
  import { tr } from '$lib/i18n';

  let {
    activePanel = $bindable(''),
    badges = {},
    onTogglePanels = () => {},
    panelsOpen = true,
  }: {
    activePanel: string;
    badges?: Record<string, number>;
    onTogglePanels?: () => void;
    panelsOpen?: boolean;
  } = $props();

  let collapsed = $state(
    typeof localStorage !== 'undefined'
      ? localStorage.getItem('jarvis-sidepanel-collapsed') === 'true'
      : false
  );

  $effect(() => {
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem('jarvis-sidepanel-collapsed', String(collapsed));
    }
  });

  type PanelItem = { id: string; label: string; icon: string };
  type PanelGroup = { group: string; items: PanelItem[] };

  const groups: PanelGroup[] = [
    {
      group: 'side.git',
      items: [
        { id: 'Commits', label: 'tab.commits', icon: '\u25C9' },
        { id: 'Diff', label: 'tab.diff', icon: '\u2261' },
        { id: 'GitHub', label: 'tab.github', icon: '\u2387' },
      ]
    },
    {
      group: 'side.work',
      items: [
        { id: 'Tareas', label: 'tab.tasks', icon: '\u2611' },
        { id: 'Pipelines', label: 'tab.pipelines', icon: '\u21A0' },
        { id: 'WhatsApp', label: 'tab.whatsapp', icon: '\uD83D\uDCAC' },
        { id: 'Docs', label: 'tab.docs', icon: '\uD83D\uDCC4' },
        { id: 'Research', label: 'tab.research', icon: '\uD83D\uDD0D' },
      ]
    },
    {
      group: 'side.infra',
      items: [
        { id: 'Maquinas', label: 'tab.machines', icon: '\u2395' },
        { id: 'Capacidades', label: 'tab.capabilities', icon: '\u29C9' },
      ]
    },
    {
      group: 'side.monitor',
      items: [
        { id: 'Timeline', label: 'tab.timeline', icon: '\u2234' },
        { id: 'Eventos', label: 'tab.events', icon: '\u2248' },
        { id: 'Logs', label: 'tab.logs', icon: '\u276F' },
      ]
    },
    {
      group: 'side.config',
      items: [
        { id: 'Costos', label: 'tab.costs', icon: '$' },
      ]
    },
  ];

  function toggle() {
    collapsed = !collapsed;
  }
</script>

<nav class="side-panel" class:collapsed aria-label={$tr('side.panelLabel')}>
  <div class="side-groups">
    {#each groups as g}
      <div class="side-group">
        <div class={collapsed ? 'group-label sr-only' : 'group-label'}>{$tr(g.group)}</div>
        {#each g.items as item}
          <button
            class="side-item"
            class:active={activePanel === item.id}
            aria-current={activePanel === item.id ? 'true' : undefined}
            title={collapsed ? $tr(item.label) : ''}
            onclick={() => { activePanel = item.id; if (!panelsOpen) onTogglePanels(); }}
          >
            <span class="side-icon">{item.icon}</span>
            <span class="side-label" class:collapsed>{$tr(item.label)}</span>
            {#if badges[item.id] && badges[item.id] > 0}
              <span class="side-badge">{badges[item.id]}</span>
            {/if}
          </button>
        {/each}
      </div>
    {/each}
  </div>
  <button
    class="side-toggle panels-toggle"
    class:panels-hidden={!panelsOpen}
    onclick={onTogglePanels}
    title={panelsOpen ? 'Ocultar paneles (Ctrl+B)' : 'Mostrar paneles (Ctrl+B)'}
  >
    {panelsOpen ? '\u25BC' : '\u25B2'}
    {#if !collapsed}
      <span class="toggle-text">{panelsOpen ? 'Ocultar' : 'Paneles'}</span>
    {/if}
  </button>
  <button class="side-toggle" onclick={toggle} aria-expanded={!collapsed} title={collapsed ? $tr('side.expand') : $tr('side.collapse')}>
    {collapsed ? '\u276F' : '\u276E'}
  </button>
</nav>

<style>
  .side-panel {
    width: 180px;
    min-width: 180px;
    background: var(--bg-1);
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    transition: width 0.2s ease, min-width 0.2s ease;
    overflow: hidden;
    flex-shrink: 0;
  }
  .side-panel.collapsed {
    width: 44px;
    min-width: 44px;
  }
  .side-groups {
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
    padding: 6px 0;
  }
  .side-group {
    margin-bottom: 4px;
  }
  .group-label {
    font-family: var(--font-display);
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 1.5px;
    text-transform: uppercase;
    color: var(--text-2);
    padding: 8px 12px 2px;
    white-space: nowrap;
    overflow: hidden;
    transition: opacity 0.2s ease, padding 0.2s ease;
  }
  .side-item {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 5px 12px 5px 10px;
    background: none;
    border: none;
    border-left: 2px solid transparent;
    color: var(--text-1);
    font-family: var(--font-mono);
    font-size: 11px;
    cursor: pointer;
    transition: background 0.15s, color 0.15s;
    white-space: nowrap;
    position: relative;
  }
  .side-item:hover {
    background: var(--bg-2);
    color: var(--text-0);
  }
  .side-item.active {
    color: var(--cyan);
    background: var(--cyan-dim);
    border-left-color: var(--cyan);
  }
  .side-icon {
    width: 18px;
    text-align: center;
    flex-shrink: 0;
    font-size: 13px;
  }
  .side-label {
    flex: 1;
    text-align: left;
    opacity: 1;
    overflow: hidden;
    transition: opacity 0.2s ease, width 0.2s ease;
  }
  .side-label.collapsed {
    opacity: 0;
    width: 0;
    overflow: hidden;
  }
  .side-badge {
    font-size: 8px;
    background: var(--cyan-dim);
    color: var(--cyan);
    padding: 0 5px;
    border-radius: 8px;
    font-variant-numeric: tabular-nums;
    flex-shrink: 0;
  }
  .side-toggle {
    background: none;
    border: none;
    border-top: 1px solid var(--border);
    color: var(--text-2);
    font-size: 12px;
    padding: 8px;
    cursor: pointer;
    transition: color 0.15s, background 0.15s;
  }
  .side-toggle:hover {
    color: var(--text-0);
    background: var(--bg-2);
  }
  .panels-toggle {
    display: flex;
    align-items: center;
    gap: 6px;
    justify-content: center;
  }
  .panels-toggle.panels-hidden {
    color: var(--cyan);
  }
  .toggle-text {
    font-size: 9px;
    letter-spacing: 0.5px;
  }
</style>
