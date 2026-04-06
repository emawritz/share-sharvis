<script lang="ts">
  import { tick, onMount, onDestroy } from 'svelte';
  import { fade, scale } from 'svelte/transition';
  import { planningState, planningModalOpen } from '../stores/planning';
  import { machines } from '../stores/machines';
  import { startPlanning, approvePlan, addPlanningFeedback, cancelPlanning, retryFailedSteps, onPlanningChunk, getPlanningHistory, clearPlanningHistory, exportPlanningSession, getPlanningMetrics, duplicatePlanningSession } from '../api';
  import { addToast } from '../stores/notifications';
  import { handleError } from '../utils';
  import { t, tr } from '$lib/i18n';
  import type { PlanningHistoryEntry, PlanningMetrics } from '../types';
  import ConfirmModal from './ConfirmModal.svelte';
  import PlanningCanvas from './PlanningCanvas.svelte';

  let objetivo = $state('');
  let feedback = $state('');
  let sending = $state(false);
  let chatEl: HTMLDivElement | undefined = $state();
  let streamingEl: HTMLDivElement | undefined = $state();
  let showCloseConfirm = $state(false);
  let showClearConfirm = $state(false);
  let fullscreen = $state(false);
  let textareaEl: HTMLTextAreaElement | undefined = $state();
  let modalEl: HTMLDivElement | undefined = $state();
  let streamingOutput = $state('');

  // History tab state
  let activeTab = $state<'current' | 'history'>('current');
  let historyEntries = $state<PlanningHistoryEntry[]>([]);
  let historyMetrics = $state<PlanningMetrics | null>(null);
  let historyLoading = $state(false);

  const MAX_OBJETIVO = 500;
  let objetivoCount = $derived(objetivo.length);

  let planState = $derived($planningState);
  let isOpen = $derived($planningModalOpen);
  let phase = $derived(planState?.phase || 'idle');
  let messages = $derived(planState?.messages || []);
  let steps = $derived(planState?.planSteps || []);
  let activity = $derived(planState?.currentActivity || []);
  let elapsed = $derived(planState?.elapsedSecs || 0);
  let streamingText = $derived(planState?.streamingText || '');
  let isActive = $derived(['planning', 'executing'].includes(phase));
  let isCompact = $derived(!planState || phase === 'idle' || phase === 'cancelled');

  $effect(() => {
    messages;
    if (chatEl) {
      chatEl.scrollTop = chatEl.scrollHeight;
    }
  });

  let planningModalCleanup: (() => void) | null = null;
  onDestroy(() => planningModalCleanup?.());

  onMount(async () => {
    if (textareaEl && (!planState || phase === 'idle')) {
      textareaEl.focus();
    }

    const unlistenChunk = await onPlanningChunk(({ chunk }) => {
      streamingOutput += chunk;
      // Auto-scroll streaming area
      tick().then(() => {
        if (streamingEl) {
          streamingEl.scrollTop = streamingEl.scrollHeight;
        }
      });
    });

    planningModalCleanup = () => {
      unlistenChunk();
    };
  });

  $effect(() => {
    if (isOpen) {
      tick().then(() => {
        if (!modalEl) return;
        const focusable = modalEl.querySelectorAll<HTMLElement>(
          'button:not([disabled]), input:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])'
        );
        if (focusable.length > 0) focusable[0].focus();
      });
    }
  });

  async function handleStart() {
    const obj = objetivo.trim();
    if (!obj) return;
    sending = true;
    streamingOutput = ''; // clear streaming output for new planning round
    try {
      await startPlanning(obj);
      objetivo = '';
    } catch (e) {
      addToast('Error: ' + handleError(e), 'error');
    }
    sending = false;
  }

  async function handleApprove() {
    try {
      await approvePlan();
      addToast(t('planning.approved'), 'success');
    } catch (e) {
      addToast('Error: ' + handleError(e), 'error');
    }
  }

  async function handleFeedback() {
    const fb = feedback.trim();
    if (!fb) return;
    streamingOutput = ''; // clear streaming for new round
    try {
      await addPlanningFeedback(fb);
      feedback = '';
      addToast(t('planning.feedbackSent'), 'info');
    } catch (e) {
      addToast('Error: ' + handleError(e), 'error');
    }
  }

  function handleCancel() {
    fullscreen = false;
    cancelPlanning().catch(e => console.error('planning: cancel failed', e));
    planningState.set(null);
    planningModalOpen.set(false);
    addToast(t('planning.cancelled'), 'info');
  }

  async function handleRetry() {
    try {
      await retryFailedSteps();
      addToast(t('planning.retrying'), 'info');
    } catch (e) {
      addToast('Error: ' + handleError(e), 'error');
    }
  }

  function handleClearConversation() {
    showClearConfirm = false;
    streamingOutput = '';
    planningState.set(null);
  }

  function handleExportPlan() {
    if (!planState) return;
    const lines: string[] = [];
    lines.push(`# Plan: ${planState.objetivo}`);
    lines.push('');
    lines.push(`**Estado:** ${phase.toUpperCase()}`);
    if (elapsed > 0) lines.push(`**Tiempo:** ${formatElapsed(elapsed)}`);
    lines.push('');

    if (messages.length > 0) {
      lines.push('## Conversación');
      lines.push('');
      for (const msg of messages) {
        lines.push(`### ${msg.sender.toUpperCase()} (R${msg.round})`);
        lines.push('');
        lines.push(msg.content);
        lines.push('');
      }
    }

    if (steps.length > 0) {
      lines.push('## Plan de ejecución');
      lines.push('');
      for (const step of steps) {
        const icon = step.status === 'done' ? '✅' : step.status === 'error' ? '❌' : step.status === 'running' ? '▶' : '◯';
        lines.push(`- ${icon} **[${step.target.toUpperCase()}]** ${step.description}`);
      }
      lines.push('');
    }

    const md = lines.join('\n');
    const blob = new Blob([md], { type: 'text/markdown' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `plan-${Date.now()}.md`;
    a.click();
    URL.revokeObjectURL(url);
    addToast('Plan exportado', 'success');
  }

  function handleClose() {
    // Always just hide the modal — never cancel the running process
    fullscreen = false;
    planningModalOpen.set(false);
  }

  function confirmClose() {
    showCloseConfirm = false;
    fullscreen = false;
    cancelPlanning().catch(e => console.error('planning: cancel failed', e));
    planningState.set(null);
    planningModalOpen.set(false);
  }

  // Generic color palette for machine senders
  const MACHINE_COLORS = ['#7eb8ff', '#7effa0', '#f0a0ff', '#ffd07e', '#7ef0e0', '#ff9e9e'];
  function senderColor(sender: string): string {
    const ids = Object.keys($machines);
    const idx = ids.indexOf(sender);
    if (idx >= 0) return MACHINE_COLORS[idx % MACHINE_COLORS.length];
    return 'var(--cyan)';
  }

  function stepStatusIcon(status: string): string {
    if (status === 'done') return '\u2713';
    if (status === 'running') return '\u25B6';
    if (status === 'error') return '\u2717';
    return '\u25CB';
  }

  function stepStatusClass(status: string): string {
    if (status === 'done') return 'step-done';
    if (status === 'running') return 'step-running';
    if (status === 'error') return 'step-error';
    return 'step-pending';
  }

  function formatElapsed(secs: number): string {
    if (secs < 60) return `${secs}s`;
    const m = Math.floor(secs / 60);
    const s = secs % 60;
    return `${m}m ${s}s`;
  }

  const BADGE_MAP: Record<string, string> = {
    Bash: 'bash', Read: 'read', Edit: 'edit', Write: 'write', Grep: 'grep', Glob: 'grep', Agent: 'agent'
  };

  function badgeClass(name: string | undefined): string {
    if (!name) return 'other';
    return BADGE_MAP[name] || 'other';
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      handleClose();
      return;
    }
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      if (phase === 'review') {
        if (feedback.trim()) {
          handleFeedback();
        } else {
          handleApprove();
        }
      }
    }
    if (e.key === 'Tab' && modalEl) {
      const focusable = Array.from(
        modalEl.querySelectorAll<HTMLElement>(
          'button:not([disabled]), input:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])'
        )
      );
      if (focusable.length === 0) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (e.shiftKey) {
        if (document.activeElement === first || !modalEl.contains(document.activeElement)) {
          e.preventDefault();
          last.focus();
        }
      } else {
        if (document.activeElement === last || !modalEl.contains(document.activeElement)) {
          e.preventDefault();
          first.focus();
        }
      }
    }
  }

  function handleOverlayClick(e: MouseEvent) {
    if ((e.target as HTMLElement)?.classList?.contains('planning-overlay')) {
      // Only close on overlay click if no active process (don't cancel mid-planning)
      if (!isActive) handleClose();
    }
  }

  async function switchToHistory() {
    activeTab = 'history';
    historyLoading = true;
    try {
      const [entries, metrics] = await Promise.all([getPlanningHistory(), getPlanningMetrics()]);
      historyEntries = entries;
      historyMetrics = metrics;
    } catch (e) {
      addToast('Error loading history: ' + handleError(e), 'error');
    }
    historyLoading = false;
  }

  function switchToCurrent() {
    activeTab = 'current';
  }

  async function handleRerun(entry: PlanningHistoryEntry) {
    activeTab = 'current';
    objetivo = entry.prompt;
    await tick();
    if (textareaEl) textareaEl.focus();
  }

  async function handleDuplicate(entry: PlanningHistoryEntry) {
    try {
      await duplicatePlanningSession(entry);
      addToast('Session duplicated — prompt loaded', 'success');
      await handleRerun(entry);
    } catch (e) {
      addToast('Error: ' + handleError(e), 'error');
    }
  }

  async function handleExportHistory(entry: PlanningHistoryEntry) {
    try {
      const filename = `plan-${entry.id}`;
      const path = await exportPlanningSession(filename);
      addToast(`Exported to ${path}`, 'success');
    } catch (e) {
      addToast('Export failed: ' + handleError(e), 'error');
    }
  }

  async function handleClearHistory() {
    try {
      await clearPlanningHistory();
      historyEntries = [];
      historyMetrics = { totalSessions: 0, avgSteps: 0, successRate: 0 };
      addToast('History cleared', 'info');
    } catch (e) {
      addToast('Error: ' + handleError(e), 'error');
    }
  }

  function formatHistoryTimestamp(ts: number): string {
    return new Date(ts * 1000).toLocaleString();
  }
</script>

{#if isOpen && fullscreen && planState && phase === 'planning'}
<!-- Fullscreen canvas mode -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="planning-fullscreen" onkeydown={handleKeydown} transition:fade={{ duration: 200 }}>
  <PlanningCanvas
    messages={messages}
    currentSpeaker={planState.currentSpeaker}
    currentRound={planState.currentRound}
    phase={phase}
    activity={activity}
    elapsed={elapsed}
  />
  <!-- Top bar overlay -->
  <div class="pf-topbar">
    <div class="pf-topbar-left">
      <span class="pm-title">PING-PONG PLANNING</span>
      <span class="pm-phase planning">PLANNING</span>
      <span class="pf-round">Ronda {planState?.currentRound ?? 0} — <span style="color:{senderColor(planState?.currentSpeaker ?? '')}">{(planState?.currentSpeaker ?? '').toUpperCase()}</span> pensando... <span class="pm-elapsed-header">{formatElapsed(elapsed)}</span></span>
    </div>
    <div class="pf-topbar-right">
      <button class="pf-btn-exit" onclick={() => fullscreen = false}>SALIR PANTALLA COMPLETA</button>
      <button class="pm-btn cancel" onclick={handleCancel}>{$tr('common.cancel')}</button>
    </div>
  </div>
  <!-- Objetivo overlay -->
  <div class="pf-objetivo">
    <span class="pm-objetivo-label">OBJETIVO</span>
    {planState.objetivo}
  </div>
</div>
{:else if isOpen}
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="planning-overlay" role="dialog" aria-modal="true" aria-label="Planning Mode" tabindex="-1" onclick={handleOverlayClick} onkeydown={handleKeydown} transition:fade={{ duration: 150 }}>
  <div class="planning-modal" class:compact={isCompact} bind:this={modalEl} in:scale={{ duration: 200, start: 0.95 }} out:fade={{ duration: 100 }}>
    <!-- Header -->
    <div class="pm-header">
      <div class="pm-header-left">
        <span class="pm-title">{$tr('planning.title')}</span>
        {#if planState && activeTab === 'current'}
          <span class="pm-phase {phase}">{phase.toUpperCase()}</span>
          {#if phase === 'planning'}
            <span class="pm-speaker">
              {$tr('planning.round')} {planState.currentRound} &mdash;
              <span style="color:{senderColor(planState.currentSpeaker)}">{planState.currentSpeaker.toUpperCase()}</span> {$tr('planning.thinking')}
              <span class="pm-elapsed-header">{formatElapsed(elapsed)}</span>
            </span>
          {/if}
        {/if}
      </div>
      <div class="pm-header-center">
        <button class="pm-tab-btn" class:active={activeTab === 'current'} onclick={switchToCurrent}>Current</button>
        <button class="pm-tab-btn" class:active={activeTab === 'history'} onclick={switchToHistory}>History</button>
      </div>
      <div class="pm-header-actions">
        {#if planState && !isActive && activeTab === 'current'}
          <button class="pm-header-btn export" onclick={handleExportPlan} title="Exportar plan como Markdown">↓ Exportar</button>
          <button class="pm-header-btn clear" onclick={() => showClearConfirm = true} title="Limpiar conversación">✕ Limpiar</button>
        {/if}
        {#if activeTab === 'history' && historyEntries.length > 0}
          <button class="pm-header-btn clear" onclick={handleClearHistory} title="Clear all history">✕ Clear All</button>
        {/if}
        <button class="pm-close" onclick={handleClose} title={isActive ? 'Minimizar (el proceso sigue en background)' : 'Cerrar'}>{isActive ? '−' : '×'}</button>
      </div>
    </div>

    {#if activeTab === 'history'}
      <!-- History view -->
      <div class="pm-history">
        {#if historyLoading}
          <div class="pm-history-empty">Loading...</div>
        {:else if historyEntries.length === 0}
          <div class="pm-history-empty">No planning sessions in history yet.</div>
        {:else}
          {#if historyMetrics}
            <div class="pm-metrics">
              <div class="pm-metric">
                <span class="pm-metric-value">{historyMetrics.totalSessions}</span>
                <span class="pm-metric-label">Sessions</span>
              </div>
              <div class="pm-metric">
                <span class="pm-metric-value">{historyMetrics.avgSteps}</span>
                <span class="pm-metric-label">Avg Steps</span>
              </div>
              <div class="pm-metric">
                <span class="pm-metric-value">{historyMetrics.successRate}%</span>
                <span class="pm-metric-label">Success Rate</span>
              </div>
            </div>
          {/if}
          <div class="pm-history-list">
            {#each historyEntries as entry (entry.id)}
              <div class="pm-history-entry">
                <div class="pm-history-meta">
                  <span class="pm-history-ts">{formatHistoryTimestamp(entry.timestamp)}</span>
                  {#if entry.machine}
                    <span class="pm-history-machine">{entry.machine.toUpperCase()}</span>
                  {/if}
                </div>
                <div class="pm-history-prompt">{entry.prompt.substring(0, 80)}{entry.prompt.length > 80 ? '…' : ''}</div>
                <div class="pm-history-actions">
                  <button class="pm-hist-btn rerun" onclick={() => handleRerun(entry)}>↩ Re-run</button>
                  <button class="pm-hist-btn duplicate" onclick={() => handleDuplicate(entry)}>⧉ Duplicate</button>
                  <button class="pm-hist-btn export" onclick={() => handleExportHistory(entry)}>↓ Export</button>
                </div>
              </div>
            {/each}
          </div>
        {/if}
      </div>
    {:else if !planState || phase === 'idle' || phase === 'cancelled'}
      <!-- Compact start form -->
      <div class="pm-start">
        <p class="pm-start-desc">{$tr('planning.startDesc')}</p>
        <div class="pm-textarea-wrap">
          <textarea
            class="pm-start-input"
            placeholder={$tr('planning.placeholder')}
            bind:value={objetivo}
            rows="3"
            maxlength={MAX_OBJETIVO}
            bind:this={textareaEl}
          ></textarea>
          <span class="pm-char-counter" class:warn={objetivoCount > MAX_OBJETIVO * 0.8}>
            {objetivoCount}/{MAX_OBJETIVO} caracteres
          </span>
        </div>
        <button
          class="pm-start-btn"
          onclick={handleStart}
          disabled={sending || !objetivo.trim()}
        >
          {#if sending}
            <span class="pm-loading-dots">
              <span></span><span></span><span></span>
            </span>
            {$tr('planning.starting')}
          {:else}
            {$tr('planning.startPlanning')}
          {/if}
        </button>
      </div>
    {:else}
      <!-- Main content -->
      <div class="pm-body">
        {#if phase === 'planning'}
          <!-- Compact conversation + fullscreen button -->
          <div class="pm-conversation">
            <div class="pm-objetivo">
              <span class="pm-objetivo-label">{$tr('planning.objective')}</span>
              {planState.objetivo}
            </div>
            <div class="pm-chat" bind:this={chatEl}>
              {#each messages as msg}
                <div class="pm-msg {msg.sender}">
                  <div class="pm-msg-header">
                    <span class="pm-msg-sender" style="color:{senderColor(msg.sender)}">{msg.sender.toUpperCase()}</span>
                    <span class="pm-msg-round">R{msg.round}</span>
                  </div>
                  <div class="pm-msg-content">{msg.content}</div>
                </div>
              {/each}
              <div class="pm-thinking-block">
                <div class="pm-thinking">
                  <span class="pm-thinking-dot"></span>
                  <span style="color:{senderColor(planState.currentSpeaker)}">{planState.currentSpeaker.toUpperCase()}</span> {$tr('planning.thinking')}
                  <span class="pm-loading-dots pm-thinking-pulse">
                    <span></span><span></span><span></span>
                  </span>
                  <span class="pm-elapsed">{formatElapsed(elapsed)}</span>
                </div>
                {#if streamingOutput}
                  <div class="pm-streaming-text" bind:this={streamingEl}>{streamingOutput}</div>
                {:else if streamingText}
                  <div class="pm-streaming-text">{streamingText}</div>
                {/if}
              </div>
            </div>
            <button class="pm-fullscreen-btn" onclick={() => fullscreen = true}>VER EN PANTALLA COMPLETA</button>
          </div>
        {:else}
          <!-- Standard layout for review/executing/done phases -->
          <div class="pm-conversation">
            <div class="pm-objetivo">
              <span class="pm-objetivo-label">{$tr('planning.objective')}</span>
              {planState.objetivo}
            </div>
            <div class="pm-chat" bind:this={chatEl}>
              {#each messages as msg}
                <div class="pm-msg {msg.sender}">
                  <div class="pm-msg-header">
                    <span class="pm-msg-sender" style="color:{senderColor(msg.sender)}">{msg.sender.toUpperCase()}</span>
                    <span class="pm-msg-round">R{msg.round}</span>
                  </div>
                  <div class="pm-msg-content">{msg.content}</div>
                </div>
              {/each}
            </div>
          </div>
          <div class="pm-plan-panel">
            {#if planState.repoBack || planState.repoFront}
              <div class="pm-repos">
                {#if planState.repoBack}
                  {@const r = planState.repoBack}
                  <div class="pm-repo">
                    <div class="pm-repo-header">
                      <span class="pm-repo-label atlas">BACK</span>
                      <span class="pm-repo-branch">{r.branch}</span>
                      {#if r.ahead > 0}<span class="pm-repo-ahead">&uarr;{r.ahead}</span>{/if}
                      {#if r.behind > 0}<span class="pm-repo-behind">&darr;{r.behind}</span>{/if}
                    </div>
                    <div class="pm-repo-stats">
                      {#if r.changed > 0}<span class="pm-stat modified">{r.changed}M</span>{/if}
                      {#if r.staged > 0}<span class="pm-stat staged">{r.staged}S</span>{/if}
                      {#if r.untracked > 0}<span class="pm-stat untracked">{r.untracked}?</span>{/if}
                      {#if r.changed === 0 && r.staged === 0 && r.untracked === 0}<span class="pm-stat clean">clean</span>{/if}
                    </div>
                    {#if r.lastCommit}<div class="pm-repo-commit">{r.lastCommit}</div>{/if}
                  </div>
                {/if}
                {#if planState.repoFront}
                  {@const r = planState.repoFront}
                  <div class="pm-repo">
                    <div class="pm-repo-header">
                      <span class="pm-repo-label pixel">FRONT</span>
                      <span class="pm-repo-branch">{r.branch}</span>
                      {#if r.ahead > 0}<span class="pm-repo-ahead">&uarr;{r.ahead}</span>{/if}
                      {#if r.behind > 0}<span class="pm-repo-behind">&darr;{r.behind}</span>{/if}
                    </div>
                    <div class="pm-repo-stats">
                      {#if r.changed > 0}<span class="pm-stat modified">{r.changed}M</span>{/if}
                      {#if r.staged > 0}<span class="pm-stat staged">{r.staged}S</span>{/if}
                      {#if r.untracked > 0}<span class="pm-stat untracked">{r.untracked}?</span>{/if}
                      {#if r.changed === 0 && r.staged === 0 && r.untracked === 0}<span class="pm-stat clean">clean</span>{/if}
                    </div>
                    {#if r.lastCommit}<div class="pm-repo-commit">{r.lastCommit}</div>{/if}
                  </div>
                {/if}
              </div>
            {/if}
            {#if steps.length > 0}
              <h3 class="pm-plan-title">{$tr('planning.planTitle')} ({steps.filter(s => s.status === 'done').length}/{steps.length} {$tr('planning.steps')})</h3>
              <div class="pm-steps">
                {#each steps as step}
                  <div class="pm-step {stepStatusClass(step.status)}">
                    <span class="pm-step-icon">{stepStatusIcon(step.status)}</span>
                    <span class="pm-step-target {step.target}">{step.target.toUpperCase()}</span>
                    <span class="pm-step-desc">{step.description}</span>
                  </div>
                {/each}
              </div>
              {#if phase === 'executing' && activity.length > 0}
                <div class="pm-exec-activity">
                  <h4 class="pm-exec-activity-title">{$tr('planning.liveActivity')} <span class="pm-elapsed">{formatElapsed(elapsed)}</span></h4>
                  {#each activity as item}
                    <div class="pm-activity-item">
                      {#if item.type === 'tool'}
                        <span class="badge {badgeClass(item.name)}">{item.name}</span>
                        <span class="pm-activity-detail">{item.detail || ''}</span>
                      {:else}
                        <span class="pm-activity-text">{(item.content || '').split('\n').slice(0, 1).join('').substring(0, 150)}</span>
                      {/if}
                    </div>
                  {/each}
                </div>
              {/if}
            {:else}
              <div class="pm-plan-empty">{$tr('planning.noPlanYet')}</div>
            {/if}
          </div>
        {/if}
      </div>

      <!-- Footer -->
      <div class="pm-footer">
        {#if phase === 'review'}
          <input
            class="pm-feedback-input"
            type="text"
            placeholder={$tr('planning.feedbackPlaceholder')}
            bind:value={feedback}
            onkeydown={handleKeydown}
          />
          <button class="pm-btn approve" onclick={handleApprove}>{$tr('planning.runPlan')}</button>
          <button class="pm-btn feedback" onclick={handleFeedback} disabled={!feedback.trim()}>{$tr('common.send')}</button>
          <button class="pm-btn cancel" onclick={handleCancel}>{$tr('common.cancel')}</button>
        {:else if phase === 'executing'}
          <div class="pm-exec-status">
            {$tr('planning.executing')} {steps.filter(s => s.status === 'done').length}/{steps.length} {$tr('planning.completed')}
            <span class="pm-elapsed">{formatElapsed(elapsed)}</span>
          </div>
          <button class="pm-btn cancel" onclick={handleCancel}>{$tr('common.cancel')}</button>
        {:else if phase === 'done' || phase === 'done-with-errors'}
          <div class="pm-exec-status">
            {phase === 'done' ? $tr('planning.planExecuted') : $tr('planning.executedWithErrors')}
            {#if elapsed > 0}<span class="pm-elapsed">{$tr('planning.inTime')} {formatElapsed(elapsed)}</span>{/if}
          </div>
          <button class="pm-btn" onclick={handleClose}>{$tr('common.close')}</button>
          {#if phase === 'done-with-errors'}
            <button class="pm-btn approve" onclick={handleRetry}>{$tr('planning.retryFailed')}</button>
          {/if}
          <button class="pm-btn" onclick={() => { planningState.set(null); }}>{$tr('planning.newPlan')}</button>
        {:else if phase === 'planning'}
          <div class="pm-exec-status">{$tr('planning.discussing')} {$tr('planning.round').toLowerCase()} {planState.currentRound}</div>
          <button class="pm-btn cancel" onclick={handleCancel}>{$tr('common.cancel')}</button>
        {/if}
      </div>
    {/if}
  </div>
</div>
{/if}

<ConfirmModal
  open={showCloseConfirm}
  title={t('planning.activeProcess')}
  message={t('planning.activeProcessMsg')}
  confirmText={t('planning.cancelProcess')}
  cancelText={t('planning.goBack')}
  variant="danger"
  onConfirm={confirmClose}
  onCancel={() => showCloseConfirm = false}
/>

<ConfirmModal
  open={showClearConfirm}
  title="Limpiar conversación"
  message="¿Seguro que quieres borrar toda la conversación y el plan actual? Esta acción no se puede deshacer."
  confirmText="Sí, limpiar"
  cancelText="Cancelar"
  variant="danger"
  onConfirm={handleClearConversation}
  onCancel={() => showClearConfirm = false}
/>

<style>
  .planning-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.85);
    z-index: 1000;
    display: grid;
    place-items: center;
    backdrop-filter: blur(4px);
  }
  .planning-modal {
    width: 92vw;
    height: 88vh;
    background: var(--bg-0);
    border: 1px solid var(--border-bright);
    border-radius: 12px;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    box-shadow: 0 24px 80px rgba(0, 0, 0, 0.6);
  }
  /* Compact modal for start form only */
  .planning-modal.compact {
    width: min(500px, 85vw);
    height: auto;
    max-height: 80vh;
    background: rgba(4, 12, 24, 0.85);
    border: 1px solid rgba(0, 212, 255, 0.15);
    box-shadow: 0 16px 60px rgba(0, 0, 0, 0.5), 0 0 40px rgba(0, 212, 255, 0.05);
    backdrop-filter: blur(20px);
  }
  .pm-header {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 20px;
    border-bottom: 1px solid var(--border);
    background: var(--bg-1);
    flex-shrink: 0;
  }
  .pm-header-left { display: flex; align-items: center; gap: 12px; }
  .pm-title {
    font-family: var(--font-display);
    font-size: 13px;
    font-weight: 700;
    letter-spacing: 2px;
    color: var(--cyan);
  }
  .pm-phase {
    font-size: 9px;
    font-weight: 600;
    padding: 2px 8px;
    border-radius: 3px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }
  .pm-phase.planning { background: #ffb74d22; color: #ffb74d; border: 1px solid #ffb74d33; }
  .pm-phase.review { background: #00d4ff22; color: var(--cyan); border: 1px solid #00d4ff33; }
  .pm-phase.executing { background: #7effa022; color: #7effa0; border: 1px solid #7effa033; }
  .pm-phase.done { background: #66bb6a22; color: #66bb6a; border: 1px solid #66bb6a33; }
  .pm-phase.cancelled { background: #ef535022; color: #ef5350; border: 1px solid #ef535033; }
  .pm-phase.done-with-errors { background: #ef535022; color: #ef5350; border: 1px solid #ef535033; }
  .pm-speaker { font-size: 11px; color: var(--text-2); }
  .pm-close {
    background: none;
    border: none;
    color: var(--text-3);
    font-size: 20px;
    cursor: pointer;
    width: 32px; height: 32px;
    display: grid;
    place-items: center;
    border-radius: 6px;
    transition: background 0.15s, color 0.15s;
  }
  .pm-close:hover { background: var(--bg-3); color: var(--text-0); }

  .pm-start {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 16px;
    padding: 40px;
  }
  .pm-start-desc { color: #8899aa; font-size: 13px; max-width: 500px; text-align: center; text-shadow: 0 1px 4px rgba(0,0,0,0.8); }
  .pm-start-input {
    width: 100%;
    max-width: 600px;
    background: rgba(4, 12, 24, 0.8);
    color: var(--text-0);
    border: 1px solid rgba(255,255,255,0.1);
    border-radius: 8px;
    padding: 14px;
    font-family: var(--font-mono);
    font-size: 13px;
    resize: vertical;
    backdrop-filter: blur(12px);
  }
  .pm-start-input:focus { outline: none; border-color: var(--cyan); box-shadow: 0 0 0 3px rgba(0,212,255,0.15); }
  .pm-start-btn {
    background: linear-gradient(180deg, #0088cc 0%, #006699 100%);
    color: #fff;
    border: 1px solid #0099dd;
    padding: 12px 32px;
    border-radius: 6px;
    font-family: var(--font-display);
    font-size: 12px;
    font-weight: 700;
    cursor: pointer;
    letter-spacing: 2px;
    text-transform: uppercase;
    transition: background 0.15s, box-shadow 0.15s;
  }
  .pm-start-btn:hover { background: linear-gradient(180deg, #0099dd 0%, #0077aa 100%); box-shadow: 0 2px 16px rgba(0,136,204,0.4); }
  .pm-start-btn:disabled { opacity: 0.4; cursor: not-allowed; }

  /* Body */
  .pm-body {
    flex: 1;
    display: flex;
    min-height: 0;
    overflow: hidden;
  }

  /* Canvas mode (planning phase) */
  .pm-canvas-area {
    flex: 1;
    position: relative;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
  }
  .pm-canvas-objetivo {
    position: absolute;
    top: 12px;
    left: 50%;
    transform: translateX(-50%);
    background: rgba(2, 8, 16, 0.75);
    border: 1px solid rgba(255,255,255,0.06);
    border-radius: 8px;
    padding: 8px 16px;
    font-size: 12px;
    color: var(--text-1);
    max-width: 60%;
    text-align: center;
    backdrop-filter: blur(8px);
    z-index: 5;
    pointer-events: none;
  }
  .pm-canvas-chat {
    position: absolute;
    bottom: 60px;
    left: 50%;
    transform: translateX(-50%);
    width: 55%;
    max-height: 35%;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px;
    z-index: 5;
    mask-image: linear-gradient(transparent 0%, black 20%, black 100%);
    -webkit-mask-image: linear-gradient(transparent 0%, black 20%, black 100%);
  }
  .pm-canvas-chat::-webkit-scrollbar { width: 3px; }
  .pm-canvas-chat::-webkit-scrollbar-thumb { background: rgba(255,255,255,0.1); border-radius: 2px; }
  .pm-canvas-msg {
    background: rgba(2, 8, 16, 0.65);
    border: 1px solid rgba(255,255,255,0.05);
    border-radius: 6px;
    padding: 8px 12px;
    backdrop-filter: blur(6px);
  }
  .pm-canvas-msg.atlas { border-left: 2px solid #00BCD4; }
  .pm-canvas-msg.pixel { border-left: 2px solid #4CAF50; }
  .pm-canvas-sender {
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 1px;
    margin-right: 6px;
  }
  .pm-canvas-round {
    font-size: 8px;
    color: var(--text-3);
  }
  .pm-canvas-content {
    font-size: 11px;
    color: #8899aa;
    line-height: 1.5;
    white-space: pre-wrap;
    word-break: break-word;
    margin-top: 4px;
    max-height: 80px;
    overflow: hidden;
  }

  .pm-conversation {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    border-right: 1px solid var(--border);
  }
  .pm-objetivo {
    padding: 10px 16px;
    border-bottom: 1px solid var(--border);
    font-size: 12px;
    color: var(--text-1);
    background: var(--bg-1);
    flex-shrink: 0;
  }
  .pm-objetivo-label {
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 700;
    color: var(--cyan);
    letter-spacing: 1px;
    margin-right: 8px;
  }
  .pm-chat {
    flex: 1;
    overflow-y: auto;
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .pm-chat::-webkit-scrollbar { width: 4px; }
  .pm-chat::-webkit-scrollbar-thumb { background: var(--border-bright); border-radius: 2px; }
  .pm-msg {
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 10px 14px;
  }
  .pm-msg.atlas { border-left: 3px solid #7eb8ff; }
  .pm-msg.pixel { border-left: 3px solid #7effa0; }
  .pm-msg.user { border-left: 3px solid var(--cyan); }
  .pm-msg-header { display: flex; align-items: center; gap: 8px; margin-bottom: 6px; }
  .pm-msg-sender {
    font-family: var(--font-display);
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 1px;
  }
  .pm-msg-round { font-size: 9px; color: var(--text-3); }
  .pm-msg-content {
    font-size: 12px;
    color: var(--text-1);
    line-height: 1.6;
    white-space: pre-wrap;
    word-break: break-word;
  }
  .pm-thinking {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px;
    font-size: 11px;
    color: var(--text-2);
  }
  .pm-thinking-dot {
    width: 8px; height: 8px;
    border-radius: 50%;
    background: var(--amber);
    animation: pulse-glow 1.5s infinite;
  }
  .pm-thinking-block {
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 10px 14px;
    border-left: 3px solid var(--amber);
  }
  .pm-streaming-text {
    font-size: 10px;
    color: var(--text-2);
    line-height: 1.6;
    white-space: pre-wrap;
    word-break: break-word;
    margin-top: 8px;
    max-height: 120px;
    overflow-y: auto;
    font-family: var(--font-mono);
    opacity: 0.8;
    border-top: 1px solid var(--border);
    padding-top: 8px;
  }
  .pm-elapsed {
    font-size: 10px;
    color: var(--text-3);
    margin-left: auto;
    font-family: var(--font-mono);
  }
  .pm-elapsed-header {
    font-size: 10px;
    color: var(--text-3);
    font-family: var(--font-mono);
    margin-left: 4px;
  }
  .pm-activity-feed {
    margin-top: 8px;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .pm-activity-item {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 2px 0;
    line-height: 1.5;
  }
  .pm-activity-detail {
    color: #5cc4b8;
    font-size: 10px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .pm-activity-text {
    color: var(--text-2);
    font-size: 10px;
    font-style: italic;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* Reuse badge styles from AgentPanel */
  .badge {
    display: inline-block;
    font-family: var(--font-display);
    font-size: 8px;
    font-weight: 700;
    padding: 1px 5px;
    border-radius: 3px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    flex-shrink: 0;
  }
  .badge.bash { background: #33221a; color: #ffb74d; border: 1px solid #ff980022; }
  .badge.read { background: #0a1a33; color: #64b5f6; border: 1px solid #2196f322; }
  .badge.edit { background: #0a2a1a; color: #66bb6a; border: 1px solid #4caf5022; }
  .badge.write { background: #0a2a1a; color: #81c784; border: 1px solid #4caf5022; }
  .badge.grep { background: #1a0a2a; color: #ba68c8; border: 1px solid #9c27b022; }
  .badge.agent { background: #2a0a0a; color: #ef5350; border: 1px solid #f4433622; }
  .badge.other { background: var(--bg-3); color: var(--text-2); border: 1px solid var(--border); }

  /* Repo status */
  .pm-repos {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-bottom: 10px;
    padding-bottom: 10px;
    border-bottom: 1px solid var(--border);
  }
  .pm-repo {
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 8px 10px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .pm-repo-header {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .pm-repo-label {
    font-family: var(--font-display);
    font-size: 8px;
    font-weight: 700;
    padding: 1px 6px;
    border-radius: 3px;
    letter-spacing: 0.5px;
    text-transform: uppercase;
    flex-shrink: 0;
  }
  .pm-repo-label.atlas { background: #0a1a33; color: #7eb8ff; border: 1px solid #2196f322; }
  .pm-repo-label.pixel { background: #0a2a1a; color: #7effa0; border: 1px solid #4caf5022; }
  .pm-repo-branch {
    font-family: var(--font-mono);
    color: var(--cyan);
    font-size: 10px;
    font-weight: 600;
  }
  .pm-repo-ahead { font-size: 9px; color: var(--green); font-weight: 600; }
  .pm-repo-behind { font-size: 9px; color: var(--amber); font-weight: 600; }
  .pm-repo-stats {
    display: flex;
    gap: 6px;
  }
  .pm-stat {
    font-family: var(--font-mono);
    font-size: 9px;
    font-weight: 600;
    padding: 0 4px;
    border-radius: 2px;
  }
  .pm-stat.modified { color: #ffb74d; background: #ffb74d11; }
  .pm-stat.staged { color: #66bb6a; background: #66bb6a11; }
  .pm-stat.untracked { color: #ba68c8; background: #ba68c811; }
  .pm-stat.clean { color: var(--text-3); font-style: italic; font-weight: 400; }
  .pm-repo-commit {
    font-family: var(--font-mono);
    font-size: 9px;
    color: var(--text-3);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* Execution activity */
  .pm-exec-activity {
    margin-top: 12px;
    padding-top: 10px;
    border-top: 1px solid var(--border);
  }
  .pm-exec-activity-title {
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 1px;
    color: var(--amber);
    text-transform: uppercase;
    margin-bottom: 8px;
    display: flex;
    align-items: center;
    gap: 8px;
  }

  /* Plan panel */
  .pm-plan-panel {
    width: 380px;
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    overflow-y: auto;
    padding: 12px;
  }
  .pm-plan-title {
    font-family: var(--font-display);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 1px;
    color: var(--text-0);
    text-transform: uppercase;
    margin-bottom: 10px;
  }
  .pm-steps { display: flex; flex-direction: column; gap: 6px; }
  .pm-step {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 8px 10px;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: var(--bg-1);
    transition: border-color 0.2s, background 0.2s;
  }
  .pm-step.step-running { border-color: #ffb74d44; background: #ffb74d08; }
  .pm-step.step-done { border-color: #66bb6a44; background: #66bb6a08; }
  .pm-step.step-error { border-color: #ef535044; background: #ef535008; }
  .pm-step-icon { font-size: 12px; flex-shrink: 0; width: 16px; text-align: center; }
  .step-done .pm-step-icon { color: #66bb6a; }
  .step-running .pm-step-icon { color: #ffb74d; animation: pulse-glow 1.5s infinite; }
  .step-error .pm-step-icon { color: #ef5350; }
  .step-pending .pm-step-icon { color: var(--text-3); }
  .pm-step-target {
    font-family: var(--font-display);
    font-size: 8px;
    font-weight: 700;
    padding: 1px 6px;
    border-radius: 3px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    flex-shrink: 0;
    margin-top: 1px;
  }
  .pm-step-target.atlas { background: #0a1a33; color: #7eb8ff; border: 1px solid #2196f322; }
  .pm-step-target.pixel { background: #0a2a1a; color: #7effa0; border: 1px solid #4caf5022; }
  .pm-step-desc { font-size: 11px; color: var(--text-1); line-height: 1.5; }
  .pm-plan-empty {
    color: var(--text-3);
    font-size: 12px;
    text-align: center;
    padding: 40px 20px;
    font-style: italic;
  }

  /* Footer */
  .pm-footer {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 20px;
    border-top: 1px solid var(--border);
    background: var(--bg-1);
    flex-shrink: 0;
  }
  .pm-feedback-input {
    flex: 1;
    background: var(--bg-0);
    color: var(--text-0);
    border: 1px solid var(--border);
    padding: 9px 14px;
    border-radius: 6px;
    font-family: var(--font-mono);
    font-size: 12px;
  }
  .pm-feedback-input:focus { outline: none; border-color: var(--cyan); box-shadow: 0 0 0 3px var(--cyan-dim); }
  .pm-btn {
    padding: 9px 20px;
    border-radius: 6px;
    font-family: var(--font-display);
    font-size: 10px;
    font-weight: 700;
    cursor: pointer;
    letter-spacing: 1px;
    text-transform: uppercase;
    transition: background 0.15s, box-shadow 0.15s;
    border: 1px solid var(--border);
    background: var(--bg-2);
    color: var(--text-1);
    white-space: nowrap;
  }
  .pm-btn:hover { background: var(--bg-3); color: var(--text-0); }
  .pm-btn:disabled { opacity: 0.4; cursor: not-allowed; }
  .pm-btn.approve {
    background: linear-gradient(180deg, #0088cc 0%, #006699 100%);
    color: #fff;
    border: 1px solid #0099dd;
  }
  .pm-btn.approve:hover { background: linear-gradient(180deg, #0099dd 0%, #0077aa 100%); }
  .pm-btn.cancel { color: var(--red); border-color: #ff335533; }
  .pm-btn.cancel:hover { background: #ff335510; }
  .pm-btn.feedback { color: var(--amber); border-color: #ffb74d33; }
  .pm-btn.feedback:hover { background: #ffb74d10; }
  .pm-exec-status {
    flex: 1;
    font-size: 11px;
    color: var(--text-2);
  }

  /* Fullscreen button in compact dialog */
  .pm-fullscreen-btn {
    display: block;
    width: 100%;
    padding: 10px;
    background: linear-gradient(180deg, rgba(0,136,204,0.15) 0%, rgba(0,102,153,0.15) 100%);
    border: 1px solid rgba(0,212,255,0.2);
    border-radius: 6px;
    color: var(--cyan);
    font-family: var(--font-display);
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 2px;
    cursor: pointer;
    transition: background 0.15s, border-color 0.15s;
    flex-shrink: 0;
  }
  .pm-fullscreen-btn:hover {
    background: linear-gradient(180deg, rgba(0,136,204,0.25) 0%, rgba(0,102,153,0.25) 100%);
    border-color: rgba(0,212,255,0.4);
  }

  /* Fullscreen planning mode */
  .planning-fullscreen {
    position: fixed;
    inset: 0;
    z-index: 1000;
    background: #020810;
  }
  .pf-topbar {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 20px;
    background: rgba(2, 8, 16, 0.7);
    backdrop-filter: blur(12px);
    border-bottom: 1px solid rgba(255,255,255,0.05);
    z-index: 10;
  }
  .pf-topbar-left {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .pf-topbar-right {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .pf-round {
    font-size: 11px;
    color: var(--text-2);
  }
  .pf-btn-exit {
    padding: 6px 14px;
    background: rgba(255,255,255,0.05);
    border: 1px solid rgba(255,255,255,0.1);
    border-radius: 4px;
    color: var(--text-2);
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 600;
    letter-spacing: 1px;
    cursor: pointer;
    transition: background 0.15s;
  }
  .pf-btn-exit:hover {
    background: rgba(255,255,255,0.1);
    color: var(--text-0);
  }
  .pf-objetivo {
    position: absolute;
    bottom: 20px;
    left: 50%;
    transform: translateX(-50%);
    background: rgba(2, 8, 16, 0.75);
    border: 1px solid rgba(255,255,255,0.06);
    border-radius: 8px;
    padding: 10px 20px;
    font-size: 12px;
    color: var(--text-1);
    max-width: 60%;
    text-align: center;
    backdrop-filter: blur(8px);
    z-index: 10;
  }

  /* Header action buttons */
  .pm-header-actions {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .pm-header-btn {
    padding: 4px 10px;
    border-radius: 4px;
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.5px;
    cursor: pointer;
    transition: background 0.15s, color 0.15s;
    border: 1px solid var(--border);
    background: var(--bg-2);
    color: var(--text-2);
  }
  .pm-header-btn:hover { background: var(--bg-3); color: var(--text-0); }
  .pm-header-btn.export { color: var(--cyan); border-color: rgba(0,212,255,0.25); }
  .pm-header-btn.export:hover { background: rgba(0,212,255,0.08); }
  .pm-header-btn.clear { color: var(--red); border-color: rgba(255,51,85,0.25); }
  .pm-header-btn.clear:hover { background: rgba(255,51,85,0.08); }

  /* Textarea wrapper with char counter */
  .pm-textarea-wrap {
    width: 100%;
    max-width: 600px;
    position: relative;
    display: flex;
    flex-direction: column;
  }
  .pm-char-counter {
    align-self: flex-end;
    font-size: 10px;
    color: var(--text-3);
    font-family: var(--font-mono);
    margin-top: 4px;
    transition: color 0.2s;
  }
  .pm-char-counter.warn { color: #ffb74d; }

  /* Loading dots */
  .pm-loading-dots {
    display: inline-flex;
    align-items: center;
    gap: 3px;
  }
  .pm-loading-dots span {
    width: 4px;
    height: 4px;
    border-radius: 50%;
    background: currentColor;
    animation: pm-dot-bounce 1.2s infinite ease-in-out;
  }
  .pm-loading-dots span:nth-child(1) { animation-delay: 0s; }
  .pm-loading-dots span:nth-child(2) { animation-delay: 0.2s; }
  .pm-loading-dots span:nth-child(3) { animation-delay: 0.4s; }
  .pm-thinking-pulse {
    color: var(--amber);
  }
  @keyframes pm-dot-bounce {
    0%, 80%, 100% { transform: scale(0.6); opacity: 0.4; }
    40% { transform: scale(1); opacity: 1; }
  }

  /* Tab switcher */
  .pm-header-center {
    display: flex;
    align-items: center;
    gap: 4px;
    position: absolute;
    left: 50%;
    transform: translateX(-50%);
  }
  .pm-tab-btn {
    padding: 4px 14px;
    border-radius: 4px;
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 1px;
    cursor: pointer;
    border: 1px solid var(--border);
    background: var(--bg-2);
    color: var(--text-3);
    text-transform: uppercase;
    transition: background 0.15s, color 0.15s, border-color 0.15s;
  }
  .pm-tab-btn:hover { background: var(--bg-3); color: var(--text-1); }
  .pm-tab-btn.active {
    background: rgba(0, 212, 255, 0.1);
    color: var(--cyan);
    border-color: rgba(0, 212, 255, 0.3);
  }

  /* History view */
  .pm-history {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .pm-history-empty {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-3);
    font-size: 13px;
    font-style: italic;
  }
  .pm-metrics {
    display: flex;
    gap: 0;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }
  .pm-metric {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 14px 8px;
    border-right: 1px solid var(--border);
  }
  .pm-metric:last-child { border-right: none; }
  .pm-metric-value {
    font-family: var(--font-display);
    font-size: 22px;
    font-weight: 700;
    color: var(--cyan);
    line-height: 1;
  }
  .pm-metric-label {
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 600;
    letter-spacing: 1px;
    color: var(--text-3);
    text-transform: uppercase;
    margin-top: 4px;
  }
  .pm-history-list {
    flex: 1;
    overflow-y: auto;
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .pm-history-list::-webkit-scrollbar { width: 4px; }
  .pm-history-list::-webkit-scrollbar-thumb { background: var(--border-bright); border-radius: 2px; }
  .pm-history-entry {
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 10px 14px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    transition: border-color 0.15s;
  }
  .pm-history-entry:hover { border-color: rgba(0, 212, 255, 0.2); }
  .pm-history-meta {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .pm-history-ts {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--text-3);
  }
  .pm-history-machine {
    font-family: var(--font-display);
    font-size: 8px;
    font-weight: 700;
    padding: 1px 6px;
    border-radius: 3px;
    letter-spacing: 0.5px;
    background: #0a1a33;
    color: #7eb8ff;
    border: 1px solid #2196f322;
  }
  .pm-history-prompt {
    font-size: 12px;
    color: var(--text-1);
    line-height: 1.5;
    font-family: var(--font-mono);
  }
  .pm-history-actions {
    display: flex;
    gap: 6px;
    margin-top: 2px;
  }
  .pm-hist-btn {
    padding: 4px 10px;
    border-radius: 4px;
    font-family: var(--font-display);
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.5px;
    cursor: pointer;
    border: 1px solid var(--border);
    background: var(--bg-2);
    color: var(--text-2);
    text-transform: uppercase;
    transition: background 0.15s, color 0.15s, border-color 0.15s;
  }
  .pm-hist-btn:hover { background: var(--bg-3); color: var(--text-0); }
  .pm-hist-btn.rerun { color: var(--cyan); border-color: rgba(0,212,255,0.25); }
  .pm-hist-btn.rerun:hover { background: rgba(0,212,255,0.08); }
  .pm-hist-btn.duplicate { color: #ba68c8; border-color: rgba(186,104,200,0.25); }
  .pm-hist-btn.duplicate:hover { background: rgba(186,104,200,0.08); }
  .pm-hist-btn.export { color: #7eb8ff; border-color: rgba(126,184,255,0.25); }
  .pm-hist-btn.export:hover { background: rgba(126,184,255,0.08); }
</style>
