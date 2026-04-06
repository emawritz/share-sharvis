<script lang="ts">
  import { saveJarvisConfig, testSshConnection } from '../../../api';
  import { addToast } from '../../../stores/notifications';
  import { handleError } from '../../../utils';
  import { t, tr } from '$lib/i18n';
  import type { JarvisConfig, MachineConfigToml } from '../../../types';

  let { cfg }: { cfg: JarvisConfig } = $props();

  let editingMachine = $state<MachineConfigToml | null>(null);
  let testingSSH = $state(false);

  async function save() {
    try {
      await saveJarvisConfig(cfg);
      addToast(t('settings.configSaved'), 'success');
    } catch (e) {
      addToast('Error: ' + handleError(e), 'error');
    }
  }

  function addMachine() {
    editingMachine = {
      id: '', name: '', host: '', os: 'linux', role: '',
      gpu: undefined, enabled: true, tags: [], repos: []
    };
  }

  function editMachine(m: MachineConfigToml) {
    editingMachine = JSON.parse(JSON.stringify(m));
  }

  function cancelEdit() {
    editingMachine = null;
  }

  function saveMachine() {
    if (!editingMachine) return;
    if (!editingMachine.id) {
      editingMachine.id = editingMachine.name.toLowerCase().replace(/[^a-z0-9]/g, '');
    }
    const idx = cfg.machines.findIndex(m => m.id === editingMachine!.id);
    if (idx >= 0) {
      cfg.machines[idx] = editingMachine;
    } else {
      cfg.machines = [...cfg.machines, editingMachine];
    }
    editingMachine = null;
    save();
  }

  function removeMachine(id: string) {
    cfg.machines = cfg.machines.filter(m => m.id !== id);
    save();
  }

  async function testSSH() {
    if (!editingMachine?.host || editingMachine.host === 'local') return;
    testingSSH = true;
    try {
      const result = await testSshConnection(editingMachine.host);
      if (result.status === 'ok') {
        addToast('SSH OK: ' + result.detail, 'success');
      } else {
        addToast('SSH failed: ' + result.detail, 'error');
      }
    } catch (e) {
      addToast('SSH error: ' + handleError(e), 'error');
    }
    testingSSH = false;
  }

  function addRepo() {
    if (!editingMachine) return;
    editingMachine.repos = [...editingMachine.repos, { name: '', path: '', github: '' }];
  }

  function removeRepo(idx: number) {
    if (!editingMachine) return;
    editingMachine.repos = editingMachine.repos.filter((_, i) => i !== idx);
  }
</script>

<div class="machines-panel">
  {#if editingMachine}
    <div class="machine-editor">
      <h3>{editingMachine.id ? $tr('machineSettings.editTitle') : $tr('machineSettings.addTitle')}</h3>
      <div class="editor-grid">
        <label class="jarvis-label">{$tr('machineSettings.name')} <input class="jarvis-input" type="text" bind:value={editingMachine.name} placeholder="PIXEL" /></label>
        <label class="jarvis-label">{$tr('machineSettings.host')} <input class="jarvis-input" type="text" bind:value={editingMachine.host} placeholder="pixel or local" /></label>
        <label class="jarvis-label">{$tr('machineSettings.ip')} <input class="jarvis-input" type="text" bind:value={editingMachine.ip} placeholder="100.x.x.x" /></label>
        <label class="jarvis-label">{$tr('machineSettings.os')}
          <select class="jarvis-input" bind:value={editingMachine.os}>
            <option value="macos">macOS</option>
            <option value="linux">Linux</option>
            <option value="windows">Windows</option>
          </select>
        </label>
        <label class="jarvis-label">{$tr('machineSettings.role')} <input class="jarvis-input" type="text" bind:value={editingMachine.role} placeholder="frontend + GPU" /></label>
        <label class="jarvis-label">{$tr('machineSettings.gpu')} <input class="jarvis-input" type="text" bind:value={editingMachine.gpu} placeholder="RTX 3070" /></label>
        <label class="jarvis-label">{$tr('machineSettings.tags')} <input class="jarvis-input" type="text" value={editingMachine.tags.join(', ')}
          oninput={(e) => { if (editingMachine) editingMachine.tags = (e.target as HTMLInputElement).value.split(',').map(t => t.trim()).filter(Boolean); }} placeholder="frontend, gpu, remote" /></label>
      </div>
      {#if editingMachine.host && editingMachine.host !== 'local'}
        <button class="jarvis-btn jarvis-btn-test" onclick={testSSH} disabled={testingSSH}>
          {testingSSH ? $tr('common.testing') : $tr('machineSettings.testSsh')}
        </button>
      {/if}
      <h4>{$tr('machineSettings.repos')}</h4>
      {#each editingMachine.repos as repo, i}
        <div class="repo-row">
          <input class="jarvis-input" type="text" bind:value={repo.name} placeholder="repo-name" />
          <input class="jarvis-input" type="text" bind:value={repo.path} placeholder="~/path/to/repo" />
          <input class="jarvis-input" type="text" bind:value={repo.github} placeholder="user/repo" />
          <button class="jarvis-btn-remove" onclick={() => removeRepo(i)}>&times;</button>
        </div>
      {/each}
      <button class="jarvis-btn" onclick={addRepo}>{$tr('machineSettings.addRepo')}</button>
      <div class="jarvis-editor-actions">
        <button class="jarvis-btn jarvis-btn-cancel" onclick={cancelEdit}>{$tr('common.cancel')}</button>
        <button class="jarvis-btn jarvis-btn-primary" onclick={saveMachine}>{$tr('common.save')}</button>
      </div>
    </div>
  {:else}
    <div class="machines-list">
      {#each cfg.machines as m}
        <div class="machine-row">
          <span class="machine-name">{m.name}</span>
          <span class="machine-host">{m.host}</span>
          <span class="machine-os">{m.os}</span>
          <span class="machine-repos">{m.repos.length} repos</span>
          <button class="jarvis-btn" onclick={() => editMachine(m)}>{$tr('common.edit')}</button>
          <button class="jarvis-btn jarvis-btn-danger" onclick={() => removeMachine(m.id)}>{$tr('common.delete')}</button>
        </div>
      {/each}
    </div>
    <button class="jarvis-btn add" onclick={addMachine}>+ {$tr('machineSettings.addTitle')}</button>
  {/if}
</div>

<style>
  .machines-panel { padding: 10px 14px; overflow: auto; flex: 1; }
  .machines-list { display: flex; flex-direction: column; gap: 4px; }
  .machine-row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 10px;
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 5px;
  }
  .machine-row .machine-name { font-weight: 700; color: var(--text-0); font-size: 11px; min-width: 60px; }
  .machine-row .machine-host { color: var(--text-2); font-size: 10px; min-width: 60px; }
  .machine-row .machine-os { color: var(--text-3); font-size: 10px; min-width: 50px; }
  .machine-row .machine-repos { color: var(--text-3); font-size: 10px; flex: 1; }

  .machine-editor {
    padding: 8px;
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 6px;
  }
  .machine-editor h3 { font-size: 12px; font-weight: 700; color: var(--text-0); margin: 0 0 8px; }
  .machine-editor h4 { font-size: 11px; font-weight: 600; color: var(--text-1); margin: 8px 0 4px; }
  .editor-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px;
  }
  .repo-row {
    display: flex;
    gap: 4px;
    align-items: center;
    margin-bottom: 4px;
  }
  .repo-row :global(.jarvis-input) {
    flex: 1;
    padding: 3px 6px;
    font-size: 10px;
  }

</style>
