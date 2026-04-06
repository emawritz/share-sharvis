<script lang="ts">
  import { getJarvisConfig } from '../../api';
  import { addToast } from '../../stores/notifications';
  import { handleError } from '../../utils';
  import { t, tr } from '$lib/i18n';
  import type { JarvisConfig } from '../../types';
  import ConnectionsSubtab from './settings/ConnectionsSubtab.svelte';
  import MachinesSubtab from './settings/MachinesSubtab.svelte';
  import GeneralSubtab from './settings/GeneralSubtab.svelte';

  let subTab = $state<'connections' | 'machines' | 'general'>('connections');
  let cfg = $state<JarvisConfig | null>(null);
  let loading = $state(true);

  $effect(() => {
    loadConfig();
  });

  async function loadConfig() {
    loading = true;
    try {
      cfg = await getJarvisConfig();
    } catch (e) {
      addToast('Error loading config: ' + handleError(e), 'error');
    }
    loading = false;
  }
</script>

<div class="settings-container">
  <div class="settings-subtabs">
    <button class:active={subTab === 'connections'} onclick={() => subTab = 'connections'}>{$tr('settings.connections')}</button>
    <button class:active={subTab === 'machines'} onclick={() => subTab = 'machines'}>{$tr('settings.machines')}</button>
    <button class:active={subTab === 'general'} onclick={() => subTab = 'general'}>{$tr('settings.general')}</button>
  </div>

  {#if loading}
    <div class="settings-loading">{$tr('settings.loadingConfig')}</div>
  {:else if !cfg}
    <div class="settings-loading">{$tr('settings.noConfig')}</div>
  {:else if subTab === 'connections'}
    <ConnectionsSubtab {cfg} />
  {:else if subTab === 'machines'}
    <MachinesSubtab {cfg} />
  {:else}
    <GeneralSubtab {cfg} onConfigReload={loadConfig} />
  {/if}
</div>

<style>
  .settings-container {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
    background: var(--bg-0);
  }
  .settings-subtabs {
    display: flex;
    gap: 0;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
    background: var(--bg-1);
    padding: 0 12px;
  }
  .settings-subtabs button {
    padding: 6px 16px;
    font-size: 11px;
    font-family: var(--font-display);
    text-transform: uppercase;
    letter-spacing: 1px;
    font-weight: 600;
    color: var(--text-2);
    background: none;
    border: none;
    border-bottom: 2px solid transparent;
    cursor: pointer;
    transition: color 0.15s, border-color 0.15s;
  }
  .settings-subtabs button:hover { color: var(--text-0); }
  .settings-subtabs button.active {
    color: var(--cyan);
    border-bottom-color: var(--cyan);
  }
  .settings-loading {
    padding: 20px;
    text-align: center;
    color: var(--text-3);
    font-size: 12px;
  }
</style>
