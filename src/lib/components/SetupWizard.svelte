<script lang="ts">
  import { onMount } from 'svelte';
  import { getDetectedLocal, saveJarvisConfig, testSshConnection, checkMachineConnections } from '../api';
  import { addToast } from '../stores/notifications';
  import { t, tr } from '$lib/i18n';
  import type { JarvisConfig, MachineConfigToml, MachineConnections } from '../types';

  let { onComplete }: { onComplete: () => void } = $props();

  const TOTAL_STEPS = 4;

  let step = $state(1);
  let prevStep = $state(1);
  let animating = $state(false);
  let localMachine = $state<MachineConfigToml | null>(null);
  let remoteMachines = $state<MachineConfigToml[]>([]);
  let connectionResults = $state<Record<string, MachineConnections>>({});
  let checking = $state(false);

  // New remote machine form
  let newName = $state('');
  let newHost = $state('');
  let newIP = $state('');
  let testingNew = $state(false);

  // Validation errors
  let nameError = $state('');
  let hostError = $state('');

  // Transition direction: 'forward' | 'backward'
  let direction = $state<'forward' | 'backward'>('forward');

  onMount(() => { detectLocal(); });

  async function detectLocal() {
    try {
      localMachine = await getDetectedLocal();
    } catch {
      localMachine = {
        id: 'local', name: 'LOCAL', host: 'local', os: 'unknown',
        role: '', enabled: true, tags: ['local'], repos: []
      };
    }
  }

  async function testNewSSH() {
    if (!newHost) return;
    testingNew = true;
    try {
      const result = await testSshConnection(newHost);
      if (result.status === 'ok') {
        addToast(t('wizard.sshOk') + ': ' + result.detail, 'success');
      } else {
        addToast(t('wizard.sshFailed') + ': ' + result.detail, 'error');
      }
    } catch (e) {
      addToast(t('common.error') + ': ' + String(e), 'error');
    }
    testingNew = false;
  }

  function validateAddRemote(): boolean {
    nameError = '';
    hostError = '';
    let valid = true;
    if (!newName.trim()) { nameError = t('wizard.nameRequired'); valid = false; }
    if (!newHost.trim()) { hostError = t('wizard.hostRequired'); valid = false; }
    return valid;
  }

  function addRemote() {
    if (!validateAddRemote()) return;
    remoteMachines = [...remoteMachines, {
      id: newName.toLowerCase().replace(/[^a-z0-9]/g, ''),
      name: newName.toUpperCase(),
      host: newHost,
      ip: newIP || undefined,
      os: 'linux',
      role: '',
      enabled: true,
      tags: ['remote'],
      repos: []
    }];
    newName = ''; newHost = ''; newIP = '';
    nameError = ''; hostError = '';
  }

  function removeRemote(idx: number) {
    remoteMachines = remoteMachines.filter((_, i) => i !== idx);
  }

  async function runChecks() {
    checking = true;
    connectionResults = {};
    // Save first so backend can find machines for check_machine_connections
    const tempConfig: JarvisConfig = {
      session: { id: '', rama: '', objetivo: '' },
      machines: [localMachine!, ...remoteMachines]
    };
    try {
      await saveJarvisConfig(tempConfig);
    } catch { /* continue anyway */ }

    const allMachines = [localMachine!, ...remoteMachines];
    for (const m of allMachines) {
      try {
        const result = await checkMachineConnections(m.id);
        connectionResults = { ...connectionResults, [m.id]: result };
      } catch { /* skip */ }
    }
    checking = false;
  }

  async function finish() {
    if (!localMachine) return;
    const config: JarvisConfig = {
      session: { id: '', rama: '', objetivo: '' },
      machines: [localMachine, ...remoteMachines]
    };
    try {
      await saveJarvisConfig(config);
      addToast(t('wizard.configuredSuccess'), 'success');
      onComplete();
    } catch (e) {
      addToast(t('common.error') + ': ' + String(e), 'error');
    }
  }

  function goTo(target: number) {
    if (target === step) return;
    direction = target > step ? 'forward' : 'backward';
    prevStep = step;
    animating = true;
    // Short delay lets the class apply before the transition fires
    requestAnimationFrame(() => {
      step = target;
      // Clear animating after transition completes (matches CSS duration)
      setTimeout(() => { animating = false; }, 250);
    });
  }
</script>

<div class="wizard-overlay">
  <div class="wizard-card">
    <div class="wizard-header">
      <h2>{$tr('wizard.title')}</h2>
      <div class="wizard-header-right">
        <span class="step-label">{$tr('wizard.stepOf', { step: String(step), total: String(TOTAL_STEPS) })}</span>
        <div class="wizard-steps">
          {#each [1,2,3,4] as s}
            <div class="step-dot" class:active={step === s} class:done={step > s}>{step > s ? '\u2713' : s}</div>
          {/each}
        </div>
      </div>
    </div>

    <div class="wizard-content" class:slide-forward={animating && direction === 'forward'} class:slide-backward={animating && direction === 'backward'}>
      {#if step === 1}
        <div class="wizard-body">
          <h3>{$tr('wizard.localMachine')}</h3>
          <p>{$tr('wizard.detected')}</p>
          {#if localMachine}
            <div class="wizard-field">
              <label>{$tr('wizard.fieldName')} <input type="text" bind:value={localMachine.name} /></label>
            </div>
            <div class="wizard-field">
              <label>{$tr('wizard.fieldOS')} <input type="text" value={localMachine.os} disabled /></label>
            </div>
            <div class="wizard-field">
              <label>{$tr('wizard.fieldTailscaleIP')} <input type="text" bind:value={localMachine.ip} placeholder={$tr('wizard.ipAutoOrManual')} /></label>
            </div>
            <div class="wizard-field">
              <label>{$tr('wizard.fieldRole')} <input type="text" bind:value={localMachine.role} placeholder={$tr('wizard.rolePlaceholder')} /></label>
            </div>
            <div class="wizard-field">
              <label>{$tr('wizard.fieldTags')} <input type="text" value={localMachine.tags.join(', ')}
                oninput={(e) => { if (localMachine) localMachine.tags = (e.target as HTMLInputElement).value.split(',').map(tag => tag.trim()).filter(Boolean); }} /></label>
            </div>
          {/if}
        </div>
        <div class="wizard-footer">
          <button class="wizard-btn primary" onclick={() => goTo(2)}>{$tr('common.next')}</button>
        </div>

      {:else if step === 2}
        <div class="wizard-body">
          <h3>{$tr('wizard.remoteMachines')}</h3>
          <p>{$tr('wizard.remoteHint')}</p>
          {#each remoteMachines as rm, i}
            <div class="remote-item">
              <span>{rm.name}</span> <span class="remote-host">{rm.host}</span>
              <button class="jarvis-btn-remove" onclick={() => removeRemote(i)}>&#x2717;</button>
            </div>
          {/each}
          <div class="add-remote-form">
            <div class="field-wrap">
              <input type="text" bind:value={newName} placeholder={$tr('wizard.namePlaceholder')} class:input-error={!!nameError} oninput={() => nameError = ''} />
              {#if nameError}<span class="field-error">{nameError}</span>{/if}
            </div>
            <div class="field-wrap">
              <input type="text" bind:value={newHost} placeholder={$tr('wizard.hostPlaceholder')} class:input-error={!!hostError} oninput={() => hostError = ''} />
              {#if hostError}<span class="field-error">{hostError}</span>{/if}
            </div>
            <input type="text" bind:value={newIP} placeholder={$tr('wizard.ipPlaceholder')} />
            <button class="wizard-btn" onclick={testNewSSH} disabled={testingNew || !newHost}>{testingNew ? '...' : $tr('common.test')}</button>
            <button class="wizard-btn" onclick={addRemote}>{$tr('common.add')}</button>
          </div>
        </div>
        <div class="wizard-footer">
          <button class="wizard-btn" onclick={() => goTo(1)}>{$tr('common.back')}</button>
          <button class="wizard-btn secondary" onclick={() => goTo(3)}>{$tr('wizard.skipForNow')}</button>
          <button class="wizard-btn primary" onclick={() => { goTo(3); runChecks(); }}>
            {remoteMachines.length === 0 ? $tr('wizard.skipAndVerify') : $tr('common.next')}
          </button>
        </div>

      {:else if step === 3}
        <div class="wizard-body">
          <h3>{$tr('wizard.verifyConnections')}</h3>
          {#each [localMachine, ...remoteMachines].filter(Boolean) as m}
            {@const mc = connectionResults[m?.id || '']}
            <div class="check-machine">
              <h4>{m?.name}</h4>
              {#if !mc}
                <span class="checking-text">{$tr('settings.verifying')}</span>
              {:else}
                {#each mc.checks as check}
                  <div class="check-row">
                    <span class="check-name">{check.name}</span>
                    <span class="check-icon" style="color: {check.status === 'ok' ? 'var(--green)' : check.status === 'warning' ? '#ffb74d' : '#ef5350'}">
                      {check.status === 'ok' ? '\u2713' : check.status === 'warning' ? '\u26A0' : '\u2717'}
                    </span>
                    <span class="check-detail">{check.detail}</span>
                  </div>
                {/each}
              {/if}
            </div>
          {/each}
          {#if !checking}
            <button class="wizard-btn" onclick={runChecks}>{$tr('settings.retryAll')}</button>
          {/if}
        </div>
        <div class="wizard-footer">
          <button class="wizard-btn" onclick={() => goTo(2)}>{$tr('common.back')}</button>
          <button class="wizard-btn secondary" onclick={() => goTo(4)}>{$tr('wizard.skipForNow')}</button>
          <button class="wizard-btn primary" onclick={() => goTo(4)}>{$tr('common.next')}</button>
        </div>

      {:else}
        <div class="wizard-body done-body">
          <h3>{$tr('wizard.done')}</h3>
          <p>{$tr('wizard.configuredWith', { count: String(1 + remoteMachines.length), plural: remoteMachines.length > 0 ? 's' : '' })}</p>

          <!-- Summary -->
          <div class="summary-card">
            <div class="summary-title">{$tr('wizard.summaryTitle')}</div>
            <div class="summary-section">
              <span class="summary-label">{$tr('wizard.summaryLocalMachine')}</span>
              <span class="summary-value">{localMachine?.name ?? '—'}</span>
              {#if localMachine?.host}
                <span class="summary-sub">{localMachine.host}{localMachine.ip ? ' · ' + localMachine.ip : ''}</span>
              {/if}
            </div>
            <div class="summary-section">
              <span class="summary-label">{$tr('wizard.summaryRemoteMachines')}</span>
              {#if remoteMachines.length === 0}
                <span class="summary-none">{$tr('wizard.summaryNone')}</span>
              {:else}
                {#each remoteMachines as rm}
                  {@const mc = connectionResults[rm.id]}
                  {@const allOk = mc ? mc.checks.every(c => c.status === 'ok') : false}
                  <div class="summary-remote-row">
                    <span class="summary-value">{rm.name}</span>
                    <span class="summary-sub">{rm.host}{rm.ip ? ' · ' + rm.ip : ''}</span>
                    {#if mc}
                      <span class="summary-status" style="color: {allOk ? 'var(--green)' : '#ffb74d'}">{allOk ? '\u2713 ok' : '\u26A0 partial'}</span>
                    {/if}
                  </div>
                {/each}
              {/if}
            </div>
          </div>

          <p class="change-hint">{$tr('wizard.changeHint')}</p>
        </div>
        <div class="wizard-footer">
          <button class="wizard-btn" onclick={() => goTo(3)}>{$tr('common.back')}</button>
          <button class="wizard-btn primary" onclick={finish}>{$tr('wizard.startJarvis')}</button>
        </div>
      {/if}
    </div>
  </div>
</div>

<style>
  .wizard-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.85);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
  }
  .wizard-card {
    background: var(--bg-0);
    border: 1px solid var(--border);
    border-radius: 12px;
    width: 500px;
    max-height: 82vh;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    box-shadow: 0 0 40px rgba(0, 212, 255, 0.08);
  }
  .wizard-header {
    padding: 16px 20px;
    border-bottom: 1px solid var(--border);
    display: flex;
    align-items: center;
    justify-content: space-between;
    background: var(--bg-1);
    flex-shrink: 0;
  }
  .wizard-header h2 {
    font-family: var(--font-display);
    font-size: 16px;
    font-weight: 700;
    letter-spacing: 2px;
    text-transform: uppercase;
    color: var(--cyan);
    margin: 0;
  }
  .wizard-header-right {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 4px;
  }
  .step-label {
    font-size: 9px;
    font-family: var(--font-display);
    font-weight: 600;
    letter-spacing: 0.5px;
    color: var(--text-3);
    text-transform: uppercase;
  }
  .wizard-steps {
    display: flex;
    gap: 8px;
  }
  .step-dot {
    width: 24px;
    height: 24px;
    border-radius: 50%;
    background: var(--bg-3);
    border: 1px solid var(--border);
    display: grid;
    place-items: center;
    font-size: 10px;
    font-weight: 700;
    color: var(--text-3);
    transition: background 0.2s, border-color 0.2s, color 0.2s;
  }
  .step-dot.active {
    background: var(--cyan);
    border-color: var(--cyan);
    color: var(--bg-0);
  }
  .step-dot.done {
    background: var(--green);
    border-color: var(--green);
    color: var(--bg-0);
  }

  /* Step content container with transition */
  .wizard-content {
    display: flex;
    flex-direction: column;
    flex: 1;
    overflow: hidden;
    transition: opacity 0.22s ease, transform 0.22s ease;
    opacity: 1;
    transform: translateX(0);
  }
  .wizard-content.slide-forward {
    animation: slide-in-forward 0.22s ease forwards;
  }
  .wizard-content.slide-backward {
    animation: slide-in-backward 0.22s ease forwards;
  }

  @keyframes slide-in-forward {
    from { opacity: 0; transform: translateX(28px); }
    to   { opacity: 1; transform: translateX(0); }
  }
  @keyframes slide-in-backward {
    from { opacity: 0; transform: translateX(-28px); }
    to   { opacity: 1; transform: translateX(0); }
  }

  .wizard-body {
    padding: 16px 20px;
    overflow-y: auto;
    flex: 1;
  }
  .wizard-body h3 {
    font-size: 14px;
    font-weight: 700;
    color: var(--text-0);
    margin: 0 0 8px;
  }
  .wizard-body p {
    font-size: 12px;
    color: var(--text-2);
    margin: 0 0 12px;
  }
  .wizard-field {
    margin-bottom: 8px;
  }
  .wizard-field label {
    display: flex;
    flex-direction: column;
    font-size: 9px;
    color: var(--text-2);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    gap: 2px;
  }
  .wizard-field input {
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 4px;
    color: var(--text-0);
    padding: 6px 10px;
    font-size: 12px;
    font-family: var(--font-mono, monospace);
  }
  .wizard-field input:focus { border-color: var(--cyan); outline: none; }
  .wizard-field input:disabled { opacity: 0.5; }
  .wizard-footer {
    padding: 12px 20px;
    border-top: 1px solid var(--border);
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    background: var(--bg-1);
    flex-shrink: 0;
  }
  .wizard-btn {
    padding: 6px 16px;
    font-size: 11px;
    font-family: var(--font-display);
    font-weight: 600;
    letter-spacing: 0.5px;
    text-transform: uppercase;
    border: 1px solid var(--border);
    border-radius: 5px;
    background: var(--bg-2);
    color: var(--text-1);
    cursor: pointer;
    transition: background 0.15s, color 0.15s;
  }
  .wizard-btn:hover { background: var(--bg-3); color: var(--text-0); }
  .wizard-btn:disabled { opacity: 0.5; cursor: default; }
  .wizard-btn.primary {
    background: #00d4ff11;
    color: var(--cyan);
    border-color: #00d4ff33;
  }
  .wizard-btn.primary:hover { background: #00d4ff22; }
  .wizard-btn.secondary {
    background: transparent;
    color: var(--text-3);
    border-color: transparent;
    font-weight: 400;
  }
  .wizard-btn.secondary:hover { color: var(--text-1); background: var(--bg-2); border-color: var(--border); }

  .remote-item {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 10px;
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 5px;
    margin-bottom: 4px;
    font-size: 11px;
    color: var(--text-0);
  }
  .remote-host { color: var(--text-2); font-size: 10px; flex: 1; }
  .add-remote-form {
    display: flex;
    gap: 6px;
    align-items: flex-start;
    margin-top: 8px;
    flex-wrap: wrap;
  }
  .add-remote-form > input {
    flex: 1;
    min-width: 80px;
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 4px;
    color: var(--text-0);
    padding: 5px 8px;
    font-size: 11px;
    font-family: var(--font-mono, monospace);
  }
  .add-remote-form > input:focus { border-color: var(--cyan); outline: none; }
  .field-wrap {
    flex: 1;
    min-width: 80px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .field-wrap input {
    width: 100%;
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 4px;
    color: var(--text-0);
    padding: 5px 8px;
    font-size: 11px;
    font-family: var(--font-mono, monospace);
    box-sizing: border-box;
  }
  .field-wrap input:focus { border-color: var(--cyan); outline: none; }
  .field-wrap input.input-error { border-color: #ef5350; }
  .field-error {
    font-size: 9px;
    color: #ef5350;
    font-family: var(--font-mono, monospace);
  }

  .check-machine {
    margin-bottom: 12px;
  }
  .check-machine h4 {
    font-size: 12px;
    font-weight: 700;
    color: var(--text-0);
    margin: 0 0 6px;
  }
  .checking-text { font-size: 11px; color: var(--text-3); font-style: italic; }
  .check-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 2px 0;
  }
  .check-name {
    font-size: 10px;
    font-weight: 600;
    color: var(--text-2);
    text-transform: uppercase;
    min-width: 70px;
  }
  .check-icon { font-weight: 700; font-size: 12px; }
  .check-detail { font-size: 10px; color: var(--text-2); }

  /* Final step / summary */
  .done-body { text-align: center; padding: 24px 20px; }
  .change-hint { margin-top: 12px; }
  .summary-card {
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 12px 16px;
    text-align: left;
    margin: 12px 0;
  }
  .summary-title {
    font-size: 9px;
    font-family: var(--font-display);
    font-weight: 700;
    letter-spacing: 1px;
    text-transform: uppercase;
    color: var(--cyan);
    margin-bottom: 10px;
  }
  .summary-section {
    margin-bottom: 10px;
  }
  .summary-section:last-child { margin-bottom: 0; }
  .summary-label {
    display: block;
    font-size: 9px;
    font-weight: 600;
    color: var(--text-3);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    margin-bottom: 2px;
  }
  .summary-value {
    font-size: 12px;
    color: var(--text-0);
    font-family: var(--font-mono, monospace);
    font-weight: 600;
  }
  .summary-sub {
    display: block;
    font-size: 10px;
    color: var(--text-3);
    font-family: var(--font-mono, monospace);
    margin-top: 1px;
  }
  .summary-none {
    font-size: 11px;
    color: var(--text-3);
    font-style: italic;
  }
  .summary-remote-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 3px 0;
    flex-wrap: wrap;
  }
  .summary-status {
    font-size: 10px;
    font-weight: 700;
    font-family: var(--font-mono, monospace);
  }
</style>
