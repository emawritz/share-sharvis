<script lang="ts">
  let {
    tabs,
    badges = {},
    activeTab = $bindable('')
  }: {
    tabs: string[];
    badges?: Record<string, number>;
    activeTab: string;
  } = $props();

  function selectTab(tab: string) {
    activeTab = tab;
  }
</script>

<div class="tab-bar" role="tablist" aria-label="Paneles inferiores">
  {#each tabs as tab}
    <button
      class="tab-btn"
      class:active={activeTab === tab}
      role="tab"
      aria-selected={activeTab === tab}
      aria-controls="tab-{tab}"
      onclick={() => selectTab(tab)}
    >
      {tab}
      {#if badges[tab] && badges[tab] > 0}
        <span class="tab-badge">{badges[tab]}</span>
      {/if}
    </button>
  {/each}
</div>

<style>
  .tab-bar {
    display: flex;
    gap: 0;
    background: var(--bg-1);
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
    overflow-x: auto;
  }
  .tab-btn {
    background: none;
    border: none;
    border-bottom: 2px solid transparent;
    color: var(--text-2);
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 1.5px;
    text-transform: uppercase;
    padding: 6px 16px;
    cursor: pointer;
    transition: color 0.15s ease, border-color 0.15s ease, background 0.15s ease;
    white-space: nowrap;
    position: relative;
  }
  .tab-btn:hover { color: var(--text-0); background: var(--bg-2); }
  .tab-btn.active { color: var(--cyan); border-bottom-color: var(--cyan); }
  .tab-badge {
    font-size: 8px;
    background: var(--cyan-dim);
    color: var(--cyan);
    padding: 0 5px;
    border-radius: 8px;
    margin-left: 4px;
    font-variant-numeric: tabular-nums;
  }
</style>
