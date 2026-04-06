<script lang="ts">
  import { session, lastHeartbeat } from '../stores/session';
  import { sendTask as apiSendTask, sendTaskChain, executeAction, sendAgentMessage } from '../api';
  import { addToast } from '../stores/notifications';
  import { handleError } from '../utils';
  import { planningModalOpen } from '../stores/planning';
  import { estimatedCost, tokenStats, startTokenTracking, stopTokenTracking } from '../stores/tokens';
  import { presets as presetsStore, loadPresets, savePreset, deletePreset as removePreset, extractTemplateVars, applyTemplate } from '../stores/presets';
  import { machines } from '../stores/machines';
  import { get } from 'svelte/store';
  import ConfirmModal from './ConfirmModal.svelte';
  import type { Preset, TaskChainStep, RepoConfigToml } from '../types';
  import { appVisible } from '../stores/visibility';
  import { t, tr } from '$lib/i18n';
  import { onMount } from 'svelte';

  let target = $state('auto');
  let selectedRepo = $state('');
  let prompt = $state('');
  let sending = $state(false);
  let presetsOpen = $state(false);
  let inputEl: HTMLInputElement | undefined = $state();

  // Command history (terminal-style up/down arrow navigation)
  const HISTORY_KEY = 'jarvis-cmd-history';
  const HISTORY_MAX = 50;
  let cmdHistory = $state<string[]>(
    (() => { try { return JSON.parse(localStorage.getItem(HISTORY_KEY) ?? '[]'); } catch { return []; } })()
  );
  let historyIndex = $state(-1);
  let historyDraft = $state(''); // draft text saved when user starts browsing history

  // Compute repos available for the selected machine
  let availableRepos = $derived.by((): RepoConfigToml[] => {
    const machineMap = $machines;
    const machineData = machineMap[target];
    if (!machineData || !machineData.repos || machineData.repos.length <= 1) return [];
    return machineData.repos;
  });

  // Reset repo selection when target changes
  $effect(() => {
    target; // depend on target
    selectedRepo = '';
  });

  // Heartbeat
  let heartbeatDead = $state(false);
  $effect(() => {
    const interval = setInterval(() => {
      if (!$appVisible) return;
      heartbeatDead = Date.now() - $lastHeartbeat > 12000;
    }, 5000);
    return () => clearInterval(interval);
  });

  // Start token tracking on mount and listen for relaunch events
  onMount(() => {
    startTokenTracking();

    const handler = (e: CustomEvent<{target: string, prompt: string}>) => {
      target = e.detail.target;
      prompt = e.detail.prompt;
    };
    window.addEventListener('jarvis-relaunch', handler as EventListener);

    return () => {
      stopTokenTracking();
      window.removeEventListener('jarvis-relaunch', handler as EventListener);
    };
  });

  // Template state
  let templateMode = $state(false);
  let templateVars = $state<Record<string, string>>({});
  let templatePreset = $state<Preset | null>(null);

  // Presets from store
  let presets = $derived($presetsStore);

  // Dynamic machine list from config
  let machineList = $derived(Object.values($machines));

  // Confirm modal state for kill-all
  let showKillConfirm = $state(false);

  // Preset name input state (replaces window.prompt)
  let showPresetNameInput = $state(false);
  let newPresetName = $state('');


  function togglePresets() {
    presetsOpen = !presetsOpen;
    if (presetsOpen) loadPresets();
  }

  function closePresets() {
    presetsOpen = false;
  }

  function selectPreset(p: Preset) {
    const vars = extractTemplateVars(p.prompt);
    if (vars.length > 0) {
      templatePreset = p;
      templateVars = Object.fromEntries(vars.map(v => [v, '']));
      templateMode = true;
      presetsOpen = false;
    } else {
      target = p.target;
      prompt = p.prompt;
      presetsOpen = false;
      inputEl?.focus();
    }
  }

  function cancelTemplate() {
    templateMode = false;
    templatePreset = null;
    templateVars = {};
  }

  function sendTemplate() {
    if (!templatePreset) return;
    const finalPrompt = applyTemplate(templatePreset.prompt, templateVars);
    target = templatePreset.target;
    prompt = finalPrompt;
    templateMode = false;
    templatePreset = null;
    handleSend();
  }

  function handleDeletePreset(idx: number) {
    removePreset(idx);
  }

  function addPreset() {
    if (!prompt.trim()) {
      addToast(t('cmd.writeTaskFirst'), 'error');
      return;
    }
    newPresetName = '';
    showPresetNameInput = true;
  }

  function confirmAddPreset(name: string) {
    showPresetNameInput = false;
    if (!name.trim()) return;
    // 'auto' is not a valid preset target; default to 'both' when auto-routing is selected
    const presetTarget: Preset['target'] = target === 'auto' ? 'both' : target;
    savePreset(name.trim(), presetTarget, prompt.trim());
    addToast(t('cmd.presetSaved', { name: name.trim() }), 'success');
  }

  function saveToHistory(cmd: string) {
    if (!cmd) return;
    // Prepend, deduplicate, cap at HISTORY_MAX
    const deduped = [cmd, ...cmdHistory.filter(h => h !== cmd)].slice(0, HISTORY_MAX);
    cmdHistory = deduped;
    try { localStorage.setItem(HISTORY_KEY, JSON.stringify(deduped)); } catch { /* ignore */ }
    historyIndex = -1;
    historyDraft = '';
  }

  async function handleSend() {
    const p = prompt.trim();
    if (!p) return;

    // Check for @target message syntax
    const msgMatch = p.match(/^@(atlas|pixel|all)\s+(.+)/i);
    if (msgMatch) {
      const msgTo = msgMatch[1].toLowerCase();
      const msgContent = msgMatch[2];
      try {
        saveToHistory(p);
        await sendAgentMessage('user', msgTo, 'info', msgContent);
        addToast(t('cmd.messageSent', { target: msgTo }), 'info');
        prompt = '';
      } catch (e) {
        addToast('Error: ' + handleError(e), 'error');
      }
      return;
    }

    sending = true;
    try {
      saveToHistory(p);
      const repoArg = selectedRepo || undefined;
      await apiSendTask(target, p, undefined, repoArg);
      prompt = '';
      addToast(t('cmd.taskSent', { target }), 'info');
    } catch (e) {
      addToast('Error: ' + handleError(e), 'error');
    } finally {
      sending = false;
    }
  }

  function handleInputKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !sending) {
      handleSend();
      return;
    }

    if (e.key === 'ArrowUp') {
      e.preventDefault();
      if (cmdHistory.length === 0) return;
      if (historyIndex === -1) {
        // Save current draft before entering history
        historyDraft = prompt;
      }
      if (historyIndex < cmdHistory.length - 1) {
        historyIndex += 1;
        prompt = cmdHistory[historyIndex];
      }
      return;
    }

    if (e.key === 'ArrowDown') {
      e.preventDefault();
      if (historyIndex === -1) return;
      if (historyIndex > 0) {
        historyIndex -= 1;
        prompt = cmdHistory[historyIndex];
      } else {
        // Back to the draft (index -1)
        historyIndex = -1;
        prompt = historyDraft;
        historyDraft = '';
      }
      return;
    }

    // Any other key: exit history browsing (user is typing fresh)
    if (historyIndex !== -1) {
      historyIndex = -1;
      historyDraft = '';
    }
  }

  // Chain builder state
  let chainOpen = $state(false);
  let chainSteps = $state<TaskChainStep[]>([]);
  let sendingChain = $state(false);

  function defaultMachineId(): string {
    const ids = Object.keys(get(machines));
    return ids.length > 0 ? ids[0] : 'atlas';
  }

  function toggleChain() {
    chainOpen = !chainOpen;
    if (chainOpen && chainSteps.length === 0) {
      chainSteps = [{ target: defaultMachineId(), prompt: '', runCondition: 'on_success' }];
    }
  }

  function addChainStep() {
    chainSteps = [...chainSteps, { target: defaultMachineId(), prompt: '', runCondition: 'on_success' }];
  }

  function removeChainStep(idx: number) {
    chainSteps = chainSteps.filter((_, i) => i !== idx);
    if (chainSteps.length === 0) chainOpen = false;
  }

  function updateChainStep(idx: number, field: keyof TaskChainStep, value: string) {
    chainSteps = chainSteps.map((s, i) => i === idx ? { ...s, [field]: value } : s);
  }

  async function handleSendChain() {
    const valid = chainSteps.filter(s => s.prompt.trim());
    if (valid.length === 0) {
      addToast(t('cmd.addAtLeastOneStep'), 'error');
      return;
    }
    sendingChain = true;
    try {
      const tasks = await sendTaskChain(valid);
      addToast(t('cmd.chainSent', { count: String(tasks.length) }), 'success');
      chainSteps = [];
      chainOpen = false;
    } catch (e) {
      addToast('Error: ' + handleError(e), 'error');
    } finally {
      sendingChain = false;
    }
  }

  async function handleGitPull() {
    addToast(t('cmd.executingGitPull'), 'info');
    try {
      await executeAction('git-pull');
      addToast(t('cmd.gitPullDone'), 'success');
    } catch (e) {
      addToast('Error: ' + handleError(e), 'error');
    }
  }

  function handleKillAll() {
    showKillConfirm = true;
  }

  async function confirmKillAll() {
    showKillConfirm = false;
    addToast(t('cmd.killingAgents'), 'info');
    try {
      await executeAction('kill-all');
      addToast(t('cmd.agentsStopped'), 'success');
    } catch (e) {
      addToast('Error: ' + handleError(e), 'error');
    }
  }

  async function handleClear() {
    addToast(t('cmd.historyCleared'), 'info');
    try {
      await executeAction('clear-history');
    } catch { /* ignore */ }
  }

  // Keyboard shortcuts (CommandBar-specific; global shortcuts are in +page.svelte)
  function handleGlobalKeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && !e.shiftKey && e.key === 'k') {
      e.preventDefault();
      inputEl?.focus();
    }
    if (e.key === 'Escape') { closePresets(); cancelTemplate(); }
  }

  // Close presets on outside click
  function handleWindowClick(e: MouseEvent) {
    const wrapper = document.querySelector('.presets-wrapper');
    if (wrapper && !wrapper.contains(e.target as Node)) {
      closePresets();
    }
  }
</script>

<svelte:window onkeydown={handleGlobalKeydown} onclick={handleWindowClick} />

{#if chainOpen}
  <div class="chain-panel" role="region" aria-label={$tr('cmd.taskChain')}>
    <div class="chain-header">
      <span class="chain-title">{$tr('cmd.taskChain')}</span>
      <span class="chain-subtitle">{chainSteps.length} {chainSteps.length === 1 ? $tr('cmd.step') : $tr('cmd.steps')}</span>
    </div>
    <div class="chain-steps">
      {#each chainSteps as step, idx}
        <div class="chain-step">
          <span class="chain-step-num">#{idx + 1}</span>
          {#if idx > 0}
            <span class="chain-arrow">|</span>
          {/if}
          <select
            class="chain-target"
            value={step.target}
            onchange={(e) => updateChainStep(idx, 'target', (e.target as HTMLSelectElement).value)}
          >
            {#each machineList as m}
              <option value={m.id}>{m.name}</option>
            {/each}
          </select>
          <input
            class="chain-prompt"
            type="text"
            placeholder={$tr('cmd.stepPrompt', { n: String(idx + 1) })}
            value={step.prompt}
            oninput={(e) => updateChainStep(idx, 'prompt', (e.target as HTMLInputElement).value)}
          />
          {#if idx > 0}
            <select
              class="chain-condition"
              value={step.runCondition}
              onchange={(e) => updateChainStep(idx, 'runCondition', (e.target as HTMLSelectElement).value)}
            >
              <option value="on_success">{$tr('cmd.ifOk')}</option>
              <option value="on_failure">{$tr('cmd.ifFail')}</option>
              <option value="always">{$tr('cmd.always')}</option>
            </select>
          {/if}
          <button class="chain-remove" type="button" title={$tr('cmd.removeStep')} onclick={() => removeChainStep(idx)}>&times;</button>
        </div>
      {/each}
    </div>
    <div class="chain-actions">
      <button class="chain-add-btn" type="button" onclick={addChainStep}>{$tr('cmd.addStep')}</button>
      <span style="flex:1"></span>
      <button class="chain-cancel-btn" type="button" onclick={() => { chainOpen = false; chainSteps = []; }}>{$tr('common.cancel')}</button>
      <button class="chain-send-btn" type="button" onclick={handleSendChain} disabled={sendingChain}>{sendingChain ? $tr('common.sending') : $tr('cmd.executeChain')}</button>
    </div>
  </div>
{/if}

<div class="command-bar" role="form" aria-label={$tr('cmd.sendTask')}>
  <div class="actions-bar">
    <button class="action-btn md-load" type="button" title="Cargar archivo .md" onclick={() => window.dispatchEvent(new CustomEvent('jarvis-switch-tab', { detail: 'Docs' }))}>Cargar .md</button>
    <button class="action-btn" type="button" aria-label={$tr('cmd.gitPullBoth')} title={$tr('cmd.gitPullBoth')} onclick={handleGitPull}>Git Pull</button>
    <button class="action-btn danger" type="button" aria-label={$tr('cmd.killActiveAgents')} title={$tr('cmd.killAllProcesses')} onclick={handleKillAll}>{$tr('cmd.killAll')}</button>
    <button class="action-btn" type="button" aria-label={$tr('cmd.clearHistory')} title={$tr('cmd.clearHistoryFull')} onclick={handleClear}>{$tr('cmd.clearHistory')}</button>
    <button class="action-btn plan" type="button" aria-label={$tr('cmd.planningMode')} title={$tr('cmd.planningModeDesc')} onclick={() => planningModalOpen.set(true)}>Plan</button>
    <button class="action-btn chain" class:active={chainOpen} type="button" aria-label={$tr('cmd.taskChain')} title={$tr('cmd.taskChainCreate')} onclick={toggleChain}>{$tr('cmd.taskChain')}</button>
  </div>
  <div class="presets-wrapper">
    <button class="presets-btn" type="button" aria-label={$tr('cmd.taskPresets')} title={$tr('cmd.taskPresets')} onclick={togglePresets}>Presets</button>
    {#if presetsOpen}
      <div class="presets-dropdown open">
        {#each presets as preset, idx}
          <div
            class="preset-item"
            role="button"
            tabindex="0"
            onclick={() => selectPreset(preset)}
            onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); selectPreset(preset); } }}
          >
            <span class="preset-target-tag {preset.target}">{preset.target}</span>
            <span class="preset-name">{preset.name}</span>
            <span class="preset-prompt">{preset.prompt}</span>
            <button
              class="preset-delete"
              type="button"
              title={$tr('cmd.deletePreset')}
              aria-label={$tr('cmd.deletePreset') + ' ' + preset.name}
              onclick={(e) => { e.stopPropagation(); handleDeletePreset(idx); }}
            >&times;</button>
          </div>
        {/each}
        <div
          class="preset-add"
          role="button"
          tabindex="0"
          onclick={addPreset}
          onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); addPreset(); } }}
        >
          <span class="preset-add-icon">+</span>
          {$tr('cmd.saveAsPreset')}
        </div>
      </div>
    {/if}
    {#if templateMode && templatePreset}
      <div class="template-fill">
        <div class="template-title">{templatePreset.name}</div>
        {#each Object.keys(templateVars) as varName}
          <label class="template-var">
            <span>{varName.replace(/_/g, ' ')}</span>
            <input type="text" bind:value={templateVars[varName]} placeholder={varName} />
          </label>
        {/each}
        <div class="template-actions">
          <button class="action-btn" type="button" onclick={cancelTemplate}>{$tr('common.cancel')}</button>
          <button class="action-btn save" type="button" onclick={sendTemplate}>{$tr('common.send')}</button>
        </div>
      </div>
    {/if}
  </div>
  <span class="separator"></span>
  <label for="targetSelect" class="sr-only">{$tr('cmd.destination')}</label>
  <select id="targetSelect" class="cmd-select" bind:value={target}>
    <option value="auto">{$tr('cmd.auto')}</option>
    <option value="both">{$tr('cmd.both')}</option>
    {#each machineList as m}
      <option value={m.id}>{m.name}</option>
    {/each}
  </select>
  {#if availableRepos.length > 0}
    <label for="repoSelect" class="sr-only">Repo</label>
    <select id="repoSelect" class="cmd-repo-select" bind:value={selectedRepo} title="Seleccionar repo">
      <option value="">repo auto</option>
      {#each availableRepos as repo}
        <option value={repo.name}>{repo.name}</option>
      {/each}
    </select>
  {/if}
  <label for="promptInput" class="sr-only">{$tr('cmd.sendTask')}</label>
  <input
    id="promptInput"
    class="cmd-input"
    type="text"
    placeholder={$tr('cmd.placeholder')}
    autocomplete="off"
    spellcheck="false"
    bind:value={prompt}
    bind:this={inputEl}
    onkeydown={handleInputKeydown}
  />
  <button
    class="cmd-send"
    type="button"
    aria-label={$tr('cmd.sendTask')}
    onclick={handleSend}
    disabled={sending}
  >{sending ? $tr('common.sending') : $tr('common.send')}</button>
  <span class="separator"></span>
  <div class="cost-display" title="Tokens: {$tokenStats.totalTokens.toLocaleString()} ({$tokenStats.inputTokens.toLocaleString()} in / {$tokenStats.outputTokens.toLocaleString()} out)">
    <span class="cost-label">USD</span>
    <span class="cost-val">${$estimatedCost.toFixed(2)}</span>
  </div>
  <div class="heartbeat" class:dead={heartbeatDead} title={$tr('header.sessionStatus')}></div>
</div>

<ConfirmModal
  open={showKillConfirm}
  title={$tr('cmd.killAgentsTitle')}
  message={$tr('cmd.killAgentsMsg')}
  confirmText={$tr('cmd.killAll')}
  cancelText={$tr('common.cancel')}
  variant="danger"
  onConfirm={confirmKillAll}
  onCancel={() => showKillConfirm = false}
/>

<ConfirmModal
  open={showPresetNameInput}
  title={$tr('cmd.savePreset')}
  message={$tr('cmd.presetNamePrompt')}
  confirmText={$tr('common.save')}
  cancelText={$tr('common.cancel')}
  variant="default"
  showInput={true}
  inputPlaceholder={$tr('cmd.presetNamePlaceholder')}
  inputValue={newPresetName}
  onConfirm={() => {}}
  onCancel={() => showPresetNameInput = false}
  onConfirmWithValue={confirmAddPreset}
/>

<style>
  .command-bar {
    background: var(--bg-2);
    border-top: 1px solid var(--border);
    padding: 10px 16px;
    display: flex;
    gap: 8px;
    align-items: center;
    flex-shrink: 0;
    position: relative;
  }
  .command-bar::before {
    content: '';
    position: absolute;
    top: -1px;
    left: 0; right: 0;
    height: 1px;
    background: linear-gradient(90deg, transparent, var(--cyan-dim), transparent);
  }
  .actions-bar {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-shrink: 0;
  }
  .action-btn {
    background: var(--bg-1);
    color: var(--text-2);
    border: 1px solid var(--border);
    padding: 5px 10px;
    border-radius: var(--radius);
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 600;
    cursor: pointer;
    letter-spacing: 0.5px;
    text-transform: uppercase;
    transition: background 0.15s ease, color 0.15s ease, border-color 0.15s ease;
    white-space: nowrap;
  }
  .action-btn:hover { background: var(--bg-3); color: var(--text-0); border-color: var(--border-bright); }
  .action-btn.danger:hover { color: var(--red); border-color: #ff335544; background: #ff335510; }
  .action-btn.plan:hover { color: var(--cyan); border-color: #00d4ff44; background: #00d4ff10; }
  .action-btn.md-load { color: var(--cyan); border-color: #00d4ff44; }
  .action-btn.md-load:hover { background: #00d4ff18; border-color: var(--cyan); }
  .separator {
    width: 1px;
    height: 20px;
    background: var(--border);
    flex-shrink: 0;
  }
  .cmd-select {
    background: var(--bg-1);
    color: var(--text-0);
    border: 1px solid var(--border-bright);
    padding: 9px 12px;
    border-radius: var(--radius);
    font-family: var(--font-display);
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 1px;
    cursor: pointer;
    text-transform: uppercase;
    -webkit-appearance: none;
    appearance: none;
    background-image: url("data:image/svg+xml,%3Csvg width='10' height='6' xmlns='http://www.w3.org/2000/svg'%3E%3Cpath d='M0 0l5 6 5-6z' fill='%234a5a6a'/%3E%3C/svg%3E");
    background-repeat: no-repeat;
    background-position: right 10px center;
    padding-right: 28px;
  }
  .cmd-select:hover { border-color: var(--text-2); }
  .cmd-repo-select {
    background: var(--bg-1);
    color: var(--text-1);
    border: 1px solid var(--border);
    padding: 9px 10px;
    border-radius: var(--radius);
    font-family: var(--font-mono);
    font-size: 10px;
    cursor: pointer;
    -webkit-appearance: none;
    appearance: none;
    max-width: 120px;
  }
  .cmd-repo-select:hover { border-color: var(--text-2); }
  .cmd-repo-select:focus { outline: none; border-color: var(--cyan); }
  .cmd-input {
    flex: 1;
    background: var(--bg-0);
    color: var(--text-0);
    border: 1px solid var(--border);
    padding: 9px 14px;
    border-radius: var(--radius);
    font-family: var(--font-mono);
    font-size: 12px;
    transition: border-color 0.15s ease, box-shadow 0.15s ease;
  }
  .cmd-input:focus {
    outline: none;
    border-color: var(--cyan);
    box-shadow: 0 0 0 3px var(--cyan-dim), 0 0 16px var(--cyan-dim);
  }
  .cmd-input:focus-visible {
    outline: 2px solid var(--cyan);
    outline-offset: -1px;
  }
  .cmd-input::placeholder { color: var(--text-3); }
  .cmd-send {
    background: linear-gradient(180deg, #0088cc 0%, #006699 100%);
    color: #fff;
    border: 1px solid #0099dd;
    padding: 9px 24px;
    border-radius: var(--radius);
    font-family: var(--font-display);
    font-size: 11px;
    font-weight: 700;
    cursor: pointer;
    letter-spacing: 2px;
    text-transform: uppercase;
    transition: background 0.15s ease, box-shadow 0.15s ease, transform 0.1s ease;
    box-shadow: 0 2px 8px rgba(0,136,204,0.2);
  }
  .cmd-send:hover {
    background: linear-gradient(180deg, #0099dd 0%, #0077aa 100%);
    box-shadow: 0 2px 16px rgba(0,136,204,0.4);
  }
  .cmd-send:active { transform: scale(0.98); }
  .cmd-send:disabled { opacity: 0.3; cursor: not-allowed; transform: none; }
  .cost-display {
    font-family: var(--font-display);
    font-size: 10px;
    color: var(--text-2);
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .cost-label { color: var(--text-3); }
  .cost-val { color: var(--amber); font-weight: 600; }
  .heartbeat {
    width: 6px; height: 6px;
    border-radius: 50%;
    background: var(--green);
    box-shadow: 0 0 6px var(--green);
    flex-shrink: 0;
    transition: background 0.3s ease, box-shadow 0.3s ease;
  }
  .heartbeat.dead { background: var(--red); box-shadow: 0 0 6px var(--red); }
  /* Presets */
  .presets-wrapper {
    position: relative;
    flex-shrink: 0;
  }
  .presets-btn {
    background: var(--bg-1);
    color: var(--text-2);
    border: 1px solid var(--border);
    padding: 5px 10px;
    border-radius: var(--radius);
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 600;
    cursor: pointer;
    letter-spacing: 0.5px;
    text-transform: uppercase;
    transition: background 0.15s ease, color 0.15s ease, border-color 0.15s ease;
    white-space: nowrap;
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .presets-btn:hover { background: var(--bg-3); color: var(--text-0); border-color: var(--border-bright); }
  .presets-btn::after {
    content: '';
    width: 0; height: 0;
    border-left: 3px solid transparent;
    border-right: 3px solid transparent;
    border-top: 4px solid currentColor;
    flex-shrink: 0;
  }
  .presets-dropdown {
    position: absolute;
    bottom: 100%;
    left: 0;
    margin-bottom: 6px;
    background: var(--bg-2);
    border: 1px solid var(--border-bright);
    border-radius: var(--radius);
    box-shadow: 0 8px 32px rgba(0,0,0,0.5);
    min-width: 320px;
    max-height: 280px;
    overflow-y: auto;
    z-index: 500;
  }
  .presets-dropdown::-webkit-scrollbar { width: 3px; }
  .presets-dropdown::-webkit-scrollbar-thumb { background: var(--border-bright); border-radius: 2px; }
  .preset-item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    cursor: pointer;
    transition: background 0.1s ease;
    border-bottom: 1px solid var(--border);
  }
  .preset-item:last-child { border-bottom: none; }
  .preset-item:hover { background: var(--bg-3); }
  .preset-name {
    font-family: var(--font-display);
    font-size: 11px;
    font-weight: 600;
    color: var(--text-0);
    white-space: nowrap;
    flex-shrink: 0;
  }
  .preset-target-tag {
    font-family: var(--font-display);
    font-size: 8px;
    font-weight: 700;
    padding: 1px 6px;
    border-radius: 3px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    flex-shrink: 0;
  }
  .preset-target-tag.auto { background: #1a1a0a; color: var(--amber); border: 1px solid #ffaa0033; }
  .preset-target-tag.both { background: var(--cyan-dim); color: var(--cyan); border: 1px solid #00d4ff33; }
  .preset-target-tag.atlas { background: #0a1a33; color: #7eb8ff; border: 1px solid #2196f322; }
  .preset-target-tag.pixel { background: #0a2a1a; color: #7effa0; border: 1px solid #4caf5022; }
  .preset-prompt {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--text-2);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
    min-width: 0;
  }
  .preset-delete {
    background: none;
    border: none;
    color: var(--text-3);
    cursor: pointer;
    font-size: 12px;
    padding: 0 2px;
    flex-shrink: 0;
    transition: color 0.15s ease;
    line-height: 1;
  }
  .preset-delete:hover { color: var(--red); }
  .preset-add {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 12px;
    cursor: pointer;
    transition: background 0.1s ease;
    border-top: 1px dashed var(--border-bright);
    color: var(--text-2);
    font-family: var(--font-display);
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.5px;
  }
  .preset-add:hover { background: var(--bg-3); color: var(--cyan); }
  .preset-add-icon {
    width: 16px; height: 16px;
    border-radius: 3px;
    border: 1px dashed var(--border-bright);
    display: grid;
    place-items: center;
    font-size: 12px;
    line-height: 1;
    flex-shrink: 0;
  }
  /* Chain builder */
  .action-btn.chain:hover { color: var(--cyan); border-color: #00d4ff44; background: #00d4ff10; }
  .action-btn.chain.active { color: var(--cyan); border-color: #00d4ff44; background: #00d4ff15; }
  .chain-panel {
    background: var(--bg-2);
    border-top: 1px solid var(--border);
    border-bottom: 1px solid var(--border-bright);
    padding: 10px 16px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    flex-shrink: 0;
  }
  .chain-header {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .chain-title {
    font-family: var(--font-display);
    font-size: 10px;
    font-weight: 700;
    color: var(--cyan);
    text-transform: uppercase;
    letter-spacing: 1px;
  }
  .chain-subtitle {
    font-family: var(--font-display);
    font-size: 9px;
    color: var(--text-3);
  }
  .chain-steps {
    display: flex;
    flex-direction: column;
    gap: 4px;
    max-height: 180px;
    overflow-y: auto;
  }
  .chain-steps::-webkit-scrollbar { width: 3px; }
  .chain-steps::-webkit-scrollbar-thumb { background: var(--border-bright); border-radius: 2px; }
  .chain-step {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 0;
  }
  .chain-step-num {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--text-3);
    width: 20px;
    text-align: center;
    flex-shrink: 0;
  }
  .chain-arrow {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--border-bright);
    flex-shrink: 0;
  }
  .chain-target {
    background: var(--bg-1);
    color: var(--text-0);
    border: 1px solid var(--border);
    padding: 4px 8px;
    border-radius: var(--radius);
    font-family: var(--font-display);
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    -webkit-appearance: none;
    appearance: none;
    cursor: pointer;
    flex-shrink: 0;
  }
  .chain-prompt {
    flex: 1;
    background: var(--bg-0);
    color: var(--text-0);
    border: 1px solid var(--border);
    padding: 4px 10px;
    border-radius: var(--radius);
    font-family: var(--font-mono);
    font-size: 11px;
    min-width: 0;
  }
  .chain-prompt:focus {
    outline: none;
    border-color: var(--cyan);
    box-shadow: 0 0 0 2px var(--cyan-dim);
  }
  .chain-prompt::placeholder { color: var(--text-3); }
  .chain-condition {
    background: var(--bg-1);
    color: var(--text-0);
    border: 1px solid var(--border);
    padding: 4px 6px;
    border-radius: var(--radius);
    font-family: var(--font-display);
    font-size: 9px;
    -webkit-appearance: none;
    appearance: none;
    cursor: pointer;
    flex-shrink: 0;
  }
  .chain-remove {
    background: none;
    border: none;
    color: var(--text-3);
    cursor: pointer;
    font-size: 14px;
    padding: 0 4px;
    flex-shrink: 0;
    line-height: 1;
    transition: color 0.15s ease;
  }
  .chain-remove:hover { color: var(--red); }
  .chain-actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .chain-add-btn {
    background: var(--bg-1);
    color: var(--text-2);
    border: 1px dashed var(--border-bright);
    padding: 4px 12px;
    border-radius: var(--radius);
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 600;
    cursor: pointer;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    transition: background 0.15s ease, color 0.15s ease;
  }
  .chain-add-btn:hover { background: var(--bg-3); color: var(--cyan); }
  .chain-cancel-btn {
    background: var(--bg-1);
    color: var(--text-2);
    border: 1px solid var(--border);
    padding: 4px 14px;
    border-radius: var(--radius);
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 600;
    cursor: pointer;
    text-transform: uppercase;
    transition: background 0.15s ease, color 0.15s ease;
  }
  .chain-cancel-btn:hover { background: var(--bg-3); color: var(--text-0); }
  .chain-send-btn {
    background: linear-gradient(180deg, #0088cc 0%, #006699 100%);
    color: #fff;
    border: 1px solid #0099dd;
    padding: 4px 16px;
    border-radius: var(--radius);
    font-family: var(--font-display);
    font-size: 10px;
    font-weight: 700;
    cursor: pointer;
    letter-spacing: 1px;
    text-transform: uppercase;
    transition: background 0.15s ease, box-shadow 0.15s ease;
    box-shadow: 0 2px 8px rgba(0,136,204,0.2);
  }
  .chain-send-btn:hover {
    background: linear-gradient(180deg, #0099dd 0%, #0077aa 100%);
    box-shadow: 0 2px 16px rgba(0,136,204,0.4);
  }
  .chain-send-btn:disabled { opacity: 0.3; cursor: not-allowed; }
  /* Template fill */
  .template-fill {
    position: absolute;
    bottom: 100%;
    left: 0;
    margin-bottom: 6px;
    background: var(--bg-1);
    border: 1px solid var(--cyan-dim);
    border-radius: 6px;
    padding: 10px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 320px;
    z-index: 500;
    box-shadow: 0 8px 32px rgba(0,0,0,0.5);
  }
  .template-title {
    font-family: var(--font-display);
    font-size: 10px;
    font-weight: 700;
    color: var(--cyan);
    text-transform: uppercase;
    letter-spacing: 1px;
  }
  .template-var {
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: 9px;
    color: var(--text-2);
    text-transform: capitalize;
  }
  .template-var input {
    background: var(--bg-0);
    border: 1px solid var(--border);
    border-radius: 4px;
    color: var(--text-0);
    padding: 4px 8px;
    font-size: 11px;
  }
  .template-var input:focus {
    border-color: var(--cyan);
    outline: none;
  }
  .template-actions {
    display: flex;
    gap: 6px;
    justify-content: flex-end;
    margin-top: 4px;
  }
  .action-btn.save { color: var(--cyan); border-color: #00d4ff44; background: #00d4ff10; }
  .action-btn.save:hover { background: #00d4ff20; }
</style>
