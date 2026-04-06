<script lang="ts">
  import { session, config, atlasFeed, pixelFeed } from '../stores/session';
  import { runningCount } from '../stores/tasks';
  import { saveConfig, getRepoStatuses, getRepoBranches, switchBranch, listWorkspaces, saveWorkspace, switchWorkspace } from '../api';
  import { addToast, notificationHistory, unreadCount, markAllRead } from '../stores/notifications';
  import { handleError } from '../utils';
  import type { RepoStatus, Activity, SnapshotSummary } from '../types';
  import { appVisible } from '../stores/visibility';
  import { tr, t, toggleLocale, locale } from '$lib/i18n';
  import { onMount } from 'svelte';
  import { theme, toggleTheme } from '$lib/stores/theme';
  import Tooltip from './Tooltip.svelte';

  let showEditModal = $state(false);
  let editSessionId = $state('');
  let editRama = $state('');
  let editObjetivo = $state('');
  let saving = $state(false);
  let clockText = $state('');

  // Voice mode — opens in external browser (Tauri WebView doesn't support getUserMedia)
  let voiceState = $state<'idle' | 'active'>('idle');

  async function toggleVoice() {
    if (voiceState === 'active') {
      voiceState = 'idle';
      return;
    }
    try {
      // Check if voice server is running
      const resp = await fetch('http://localhost:3144/token');
      if (!resp.ok) throw new Error('Voice server no disponible');

      // Open voice UI in external browser (has mic access)
      // @ts-ignore — Tauri global
      window.__TAURI__?.shell?.open('http://localhost:3144') ??
        window.open('http://localhost:3144', '_blank');
      voiceState = 'active';
      addToast('Modo voz abierto en el navegador', 'success');
    } catch (err) {
      addToast('Error voz: ' + handleError(err), 'error');
    }
  }


  // Repo branch state
  let repoBack = $state<RepoStatus | null>(null);
  let repoFront = $state<RepoStatus | null>(null);
  let branchDropdown = $state<'back' | 'front' | null>(null);
  let branchList = $state<string[]>([]);
  let loadingBranches = $state(false);
  let switchingBranch = $state(false);

  // Load repo statuses on mount and periodically; also start clock
  onMount(() => {
    loadRepoStatuses();
    const repoInterval = setInterval(loadRepoStatuses, 30000);

    const formatHHMM = () => {
      const d = new Date();
      return d.getHours().toString().padStart(2, '0') + ':' + d.getMinutes().toString().padStart(2, '0');
    };
    clockText = formatHHMM();
    const clockInterval = setInterval(() => {
      if ($appVisible) clockText = formatHHMM();
    }, 60000);

    return () => {
      clearInterval(repoInterval);
      clearInterval(clockInterval);
    };
  });

  // Close dropdown on outside click
  $effect(() => {
    if (branchDropdown) {
      const handler = (e: MouseEvent) => {
        const target = e.target as HTMLElement;
        if (!target.closest('.branch-chip-wrapper')) {
          branchDropdown = null;
        }
      };
      document.addEventListener('click', handler);
      return () => document.removeEventListener('click', handler);
    }
  });

  async function loadRepoStatuses() {
    if (!$appVisible) return;
    try {
      const statuses = await getRepoStatuses();
      repoBack = statuses[0] ?? null;
      repoFront = statuses[1] ?? null;
    } catch {
      // silently fail
    }
  }

  async function openBranchDropdown(repo: 'back' | 'front', e: MouseEvent) {
    e.stopPropagation();
    if (branchDropdown === repo) {
      branchDropdown = null;
      return;
    }
    branchDropdown = repo;
    loadingBranches = true;
    branchList = [];
    try {
      branchList = await getRepoBranches(repo);
    } catch {
      branchList = [];
    } finally {
      loadingBranches = false;
    }
  }

  async function selectBranch(repo: 'back' | 'front', branch: string) {
    switchingBranch = true;
    try {
      const status = await switchBranch(repo, branch);
      if (repo === 'back') repoBack = status;
      else repoFront = status;
      addToast(`${repo.toUpperCase()} → ${branch}`, 'success');
    } catch (e) {
      addToast(t('common.error') + ': ' + handleError(e), 'error');
    } finally {
      switchingBranch = false;
      branchDropdown = null;
    }
  }

  function openEditor() {
    editSessionId = $config.sessionId || '';
    editRama = $config.rama || '';
    editObjetivo = $config.objetivo || '';
    showEditModal = true;
  }

  function closeEditor() {
    showEditModal = false;
  }

  async function saveEdit() {
    saving = true;
    try {
      await saveConfig({
        sessionId: editSessionId,
        rama: editRama,
        objetivo: editObjetivo
      });
      closeEditor();
    } catch (e) {
      addToast(t('common.error') + ': ' + handleError(e), 'error');
    } finally {
      saving = false;
    }
  }

  function handleEditKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') closeEditor();
  }

  function handleInputKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter') saveEdit();
  }

  function handleOverlayClick(e: MouseEvent) {
    if ((e.target as HTMLElement).classList.contains('edit-overlay')) closeEditor();
  }

  function handleEditableKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); openEditor(); }
  }

  // Workspace switcher state
  let workspaceOpen = $state(false);
  let workspaces = $state<SnapshotSummary[]>([]);
  let workspaceSaving = $state(false);
  let showSaveWorkspace = $state(false);
  let newWorkspaceName = $state('');
  let currentWorkspace = $state('');

  onMount(() => {
    loadWorkspaces();
  });

  // Close workspace dropdown on outside click
  $effect(() => {
    if (workspaceOpen) {
      const handler = (e: MouseEvent) => {
        const target = e.target as HTMLElement;
        if (!target.closest('.workspace-wrapper')) {
          workspaceOpen = false;
          showSaveWorkspace = false;
        }
      };
      document.addEventListener('click', handler);
      return () => document.removeEventListener('click', handler);
    }
  });

  async function loadWorkspaces() {
    try {
      workspaces = await listWorkspaces();
    } catch {
      // silently fail
    }
  }

  function openWorkspaceDropdown(e: MouseEvent) {
    e.stopPropagation();
    workspaceOpen = !workspaceOpen;
    if (workspaceOpen) loadWorkspaces();
    showSaveWorkspace = false;
  }

  async function handleSwitchWorkspace(name: string) {
    if (switching) return;
    switching = true;
    try {
      await switchWorkspace(name);
      workspaceOpen = false;
      window.location.reload();
    } catch (e) {
      addToast('Error cambiando workspace: ' + handleError(e), 'error');
    } finally {
      switching = false;
    }
  }

  async function handleSaveWorkspace() {
    const name = newWorkspaceName.trim();
    if (!name) return;
    workspaceSaving = true;
    try {
      await saveWorkspace(name);
      currentWorkspace = name;
      newWorkspaceName = '';
      showSaveWorkspace = false;
      await loadWorkspaces();
      addToast(`Workspace guardado: ${name}`, 'success');
    } catch (e) {
      addToast('Error guardando workspace: ' + handleError(e), 'error');
    } finally {
      workspaceSaving = false;
    }
  }

  function handleSaveWorkspaceKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter') handleSaveWorkspace();
    if (e.key === 'Escape') { showSaveWorkspace = false; newWorkspaceName = ''; }
  }

  let switching = $state(false);

  // Notification panel
  let notifOpen = $state(false);

  function toggleNotifPanel(e: MouseEvent) {
    e.stopPropagation();
    notifOpen = !notifOpen;
    if (notifOpen) markAllRead();
  }

  // Close notification panel on outside click
  $effect(() => {
    if (notifOpen) {
      const handler = (e: MouseEvent) => {
        const target = e.target as HTMLElement;
        if (!target.closest('.notif-wrapper')) {
          notifOpen = false;
        }
      };
      document.addEventListener('click', handler);
      return () => document.removeEventListener('click', handler);
    }
  });

  let pixelMode = $state(typeof localStorage !== 'undefined' ? localStorage.getItem('jarvis-pixel-mode') === 'true' : false);

  function togglePixelMode() {
    pixelMode = !pixelMode;
    localStorage.setItem('jarvis-pixel-mode', String(pixelMode));
    window.dispatchEvent(new CustomEvent('jarvis-pixel-toggle', { detail: pixelMode }));
  }

  /** Extract a short description from the latest activity item */
  function describeActivity(feed: Activity[]): string {
    for (let i = feed.length - 1; i >= 0; i--) {
      const a = feed[i];
      if (a.type === 'tool' && a.name) {
        const detail = a.detail ? `: ${a.detail}` : '';
        return `${a.name}${detail}`;
      }
      if (a.type === 'text' && a.content) {
        return a.content.substring(0, 80);
      }
    }
    return '';
  }

  let dynamicStatus = $derived.by(() => {
    const atlasRunning = $session.atlasRunning;
    const pixelRunning = $session.pixelRunning;
    if (!atlasRunning && !pixelRunning) return '';

    const parts: string[] = [];
    if (atlasRunning) {
      const desc = describeActivity($atlasFeed);
      if (desc) parts.push(`ATLAS: ${desc}`);
    }
    if (pixelRunning) {
      const desc = describeActivity($pixelFeed);
      if (desc) parts.push(`PIXEL: ${desc}`);
    }
    return parts.join('  |  ');
  });

  let headerStatusText = $derived(dynamicStatus || $session.objetivo || '');

  let displaySessionId = $derived(
    $session.sessionId
      ? $session.sessionId.replace(/^(parallel|longrun)-/, '').substring(0, 10)
      : '-'
  );

  let roundPips = $derived.by(() => {
    const total = parseInt($session.totalRounds) || 0;
    const rounds = $session.rounds || [];
    const result: { num: number; cls: string; label: string; text: string }[] = [];
    for (let i = 1; i <= total; i++) {
      const matching = rounds.filter((r) => r.file && r.file.includes('round-' + i + '-'));
      const done = matching.length >= 2 && matching.every((r) => r.done);
      const active = matching.length > 0 && !done;
      result.push({
        num: i,
        cls: done ? 'done' : active ? 'active' : 'pending',
        label: t('header.round', { n: i }) + (done ? ' ' + t('header.roundComplete') : active ? ' ' + t('header.roundActive') : ''),
        text: done ? '\u2713' : String(i)
      });
    }
    return result;
  });
</script>

<header class="header">
  <div class="logo">
    <div class="logo-mark" aria-hidden="true">J</div>
    <h1>JARVIS</h1>
  </div>
  <div class="header-center" role="region" aria-label={$tr('header.sessionStatus')}>
    <span
      class="meta-chip session editable"
      title="{$tr('header.session')}: {$session.sessionId || '-'}"
      role="button"
      tabindex="0"
      aria-label={$tr('header.editSession')}
      onclick={openEditor}
      onkeydown={handleEditableKeydown}
    >{displaySessionId}</span>

    <!-- BACK branch chip -->
    <div class="branch-chip-wrapper">
      <button
        class="meta-chip branch-back"
        title="Backend: {repoBack?.branch || '...'}"
        disabled={loadingBranches}
        onclick={(e) => openBranchDropdown('back', e)}
      >
        <span class="branch-label">BACK</span>
        <span class="branch-name">{repoBack?.branch || '...'}</span>
        {#if repoBack && (repoBack.changed > 0 || repoBack.staged > 0)}
          <span class="branch-dirty" title="{repoBack.changed} changed, {repoBack.staged} staged">*</span>
        {/if}
      </button>
      {#if branchDropdown === 'back'}
        <div class="branch-dropdown">
          {#if loadingBranches}
            <div class="branch-loading">{$tr('header.loadingBranches')}</div>
          {:else if branchList.length === 0}
            <div class="branch-loading">{$tr('header.noBranches')}</div>
          {:else}
            {#each branchList as b}
              <button
                class="branch-option"
                class:current={b === repoBack?.branch}
                onclick={() => selectBranch('back', b)}
                disabled={switchingBranch}
              >{b}{b === repoBack?.branch ? ' ✓' : ''}</button>
            {/each}
          {/if}
        </div>
      {/if}
    </div>

    <!-- FRONT branch chip -->
    <div class="branch-chip-wrapper">
      <button
        class="meta-chip branch-front"
        title="Frontend: {repoFront?.branch || '...'}"
        disabled={loadingBranches}
        onclick={(e) => openBranchDropdown('front', e)}
      >
        <span class="branch-label">FRONT</span>
        <span class="branch-name">{repoFront?.branch || '...'}</span>
        {#if repoFront && (repoFront.changed > 0 || repoFront.staged > 0)}
          <span class="branch-dirty" title="{repoFront.changed} changed, {repoFront.staged} staged">*</span>
        {/if}
      </button>
      {#if branchDropdown === 'front'}
        <div class="branch-dropdown">
          {#if loadingBranches}
            <div class="branch-loading">{$tr('header.loadingBranches')}</div>
          {:else if branchList.length === 0}
            <div class="branch-loading">{$tr('header.noBranches')}</div>
          {:else}
            {#each branchList as b}
              <button
                class="branch-option"
                class:current={b === repoFront?.branch}
                onclick={() => selectBranch('front', b)}
                disabled={switchingBranch}
              >{b}{b === repoFront?.branch ? ' ✓' : ''}</button>
            {/each}
          {/if}
        </div>
      {/if}
    </div>

    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <span
      class="obj-text"
      class:editable={!dynamicStatus}
      class:live-status={!!dynamicStatus}
      title={dynamicStatus ? $tr('header.liveStatus', { objetivo: $session.objetivo || '-' }) : ($session.objetivo || $tr('header.noObjective'))}
      role={dynamicStatus ? 'status' : 'button'}
      tabindex={dynamicStatus ? -1 : 0}
      aria-label={dynamicStatus ? $tr('header.agentStatus') : $tr('header.editObjective')}
      onclick={dynamicStatus ? undefined : openEditor}
      onkeydown={dynamicStatus ? undefined : handleEditableKeydown}
    >{headerStatusText}{#if !headerStatusText}<span class="obj-placeholder">{$tr('header.noActiveObjective')}</span>{/if}</span>
    <div class="rounds" role="list">
      {#each roundPips as pip (pip.num)}
        <div class="round-pip {pip.cls}" role="listitem" title={pip.label}>{pip.text}</div>
      {/each}
    </div>
  </div>
  <div class="header-right">
    <!-- Workspace switcher -->
    <div class="workspace-wrapper">
      <button
        class="workspace-btn"
        onclick={openWorkspaceDropdown}
        title="Workspace switcher"
      >
        <span class="workspace-icon">&#9737;</span>
        <span class="workspace-name">{currentWorkspace || 'Workspace'}</span>
        <span class="workspace-caret">&#9662;</span>
      </button>
      {#if workspaceOpen}
        <div class="workspace-dropdown">
          {#if workspaces.length === 0}
            <div class="workspace-empty">Sin workspaces guardados</div>
          {:else}
            {#each workspaces as ws}
              <button
                class="workspace-item"
                class:ws-current={ws.name === currentWorkspace}
                onclick={() => handleSwitchWorkspace(ws.name)}
              >
                <span class="ws-name">{ws.name}</span>
                {#if ws.rama}
                  <span class="ws-meta">{ws.rama}</span>
                {/if}
                {#if ws.name === currentWorkspace}
                  <span class="ws-check">&#10003;</span>
                {/if}
              </button>
            {/each}
          {/if}
          <div class="workspace-divider"></div>
          {#if showSaveWorkspace}
            <div class="workspace-save-form">
              <input
                type="text"
                class="workspace-name-input"
                bind:value={newWorkspaceName}
                placeholder="nombre-workspace"
                onkeydown={handleSaveWorkspaceKeydown}
                spellcheck="false"
                autocomplete="off"
              />
              <button
                class="workspace-save-confirm"
                onclick={handleSaveWorkspace}
                disabled={workspaceSaving || !newWorkspaceName.trim()}
              >{workspaceSaving ? '...' : 'Guardar'}</button>
            </div>
          {:else}
            <button class="workspace-add-btn" onclick={(e) => { e.stopPropagation(); showSaveWorkspace = true; }}>
              + Guardar workspace actual
            </button>
          {/if}
        </div>
      {/if}
    </div>

    <div class="live-indicator" class:on={$session.active} role="status">
      <span class="live-dot" aria-hidden="true"></span>
      {$tr('header.live')}
    </div>
    {#if $runningCount > 0}
      <div class="task-spinner" title="{$runningCount} task{$runningCount > 1 ? 's' : ''} running" role="status" aria-label="{$runningCount} tasks running">
        <span class="spinner-ring" aria-hidden="true"></span>
        <span class="spinner-count">{$runningCount}</span>
      </div>
    {/if}
    <button
      class="voice-btn"
      class:voice-listening={voiceState === 'active'}
      onclick={toggleVoice}
      title={voiceState === 'idle' ? 'Modo voz (abre en navegador)' : 'Voz activa en navegador'}
    >
      {#if voiceState === 'active'}
        &#127908;
      {:else}
        &#127908;
      {/if}
    </button>
    <time class="clock" aria-label={$tr('header.lastUpdate')}>{clockText}</time>
    <Tooltip text={pixelMode ? $tr('header.normalView') : $tr('header.pixelView')} position="bottom">
      <button
        class="pixel-toggle"
        class:pixel-active={pixelMode}
        onclick={togglePixelMode}
      >
        {pixelMode ? '|||' : '8bit'}
      </button>
    </Tooltip>
    <button
      class="theme-toggle"
      onclick={toggleTheme}
      title={$theme === 'dark' ? $tr('theme.switchToLight') : $tr('theme.switchToDark')}
    >
      {$theme === 'dark' ? '\u2600' : '\u263E'}
    </button>
    <button class="lang-toggle" onclick={toggleLocale} title={$tr('lang.switchTo', { lang: $locale === 'es' ? $tr('lang.english') : $tr('lang.spanish') })}>
      {$locale === 'es' ? 'EN' : 'ES'}
    </button>
    <Tooltip text={$tr('header.missionControl')} position="bottom">
      <button class="mc-toggle" onclick={() => window.__jarvis_toggle_mc?.()}>
        MC
      </button>
    </Tooltip>
    <!-- Notification bell -->
    <div class="notif-wrapper">
      <button class="notif-btn" onclick={toggleNotifPanel} title="Notificaciones">
        &#128276;{#if $unreadCount > 0}<span class="notif-badge">{$unreadCount}</span>{/if}
      </button>
      {#if notifOpen}
        <div class="notif-dropdown">
          <div class="notif-header">
            <span class="notif-title">Notificaciones</span>
            <button class="notif-clear" onclick={() => { import('../stores/notifications').then(m => m.clearNotificationHistory()); }}>Limpiar</button>
          </div>
          {#if $notificationHistory.length === 0}
            <div class="notif-empty">Sin notificaciones</div>
          {:else}
            {#each [...$notificationHistory].reverse().slice(0, 10) as n (n.id)}
              <div class="notif-item notif-{n.type}">
                <span class="notif-msg">{n.message}</span>
                <span class="notif-time">{new Date(n.timestamp).toLocaleTimeString()}</span>
              </div>
            {/each}
          {/if}
        </div>
      {/if}
    </div>
    <Tooltip text="Settings (Ctrl+,)" position="bottom">
      <button class="settings-btn" onclick={() => window.__jarvis_open_settings?.()}>
        ⊕
      </button>
    </Tooltip>
  </div>
</header>

{#if showEditModal}
  <div
    class="edit-overlay"
    onclick={handleOverlayClick}
    onkeydown={handleEditKeydown}
    role="presentation"
  >
    <div class="edit-modal" role="dialog" aria-label={$tr('header.configureSession')}>
      <h3>{$tr('header.configureSession')}</h3>
      <div class="edit-field">
        <label for="edit-sessionId">{$tr('header.session')}</label>
        <input
          id="edit-sessionId"
          type="text"
          bind:value={editSessionId}
          onkeydown={handleInputKeydown}
          spellcheck="false"
          autocomplete="off"
        />
      </div>
      <div class="edit-field">
        <label for="edit-rama">{$tr('header.branch')}</label>
        <input
          id="edit-rama"
          type="text"
          bind:value={editRama}
          onkeydown={handleInputKeydown}
          spellcheck="false"
          autocomplete="off"
        />
      </div>
      <div class="edit-field">
        <label for="edit-objetivo">{$tr('header.objective')}</label>
        <textarea
          id="edit-objetivo"
          bind:value={editObjetivo}
          spellcheck="false"
          autocomplete="off"
        ></textarea>
      </div>
      <div class="edit-actions">
        <button class="btn-cancel" type="button" onclick={closeEditor}>{$tr('common.cancel')}</button>
        <button class="btn-save" type="button" onclick={saveEdit} disabled={saving}>
          {saving ? $tr('common.saving') : $tr('common.save')}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .header {
    background: var(--bg-1);
    border-bottom: 1px solid var(--border);
    padding: 0 20px;
    display: flex;
    align-items: stretch;
    flex-shrink: 0;
    position: relative;
    height: 48px;
    overflow: visible;
    z-index: 50;
  }
  .header::after {
    content: '';
    position: absolute;
    bottom: -1px;
    left: 0; right: 0;
    height: 1px;
    background: linear-gradient(90deg, transparent, var(--cyan-glow), transparent);
  }
  .logo {
    display: flex;
    align-items: center;
    gap: 10px;
    padding-right: 20px;
    border-right: 1px solid var(--border);
    flex-shrink: 0;
  }
  .logo-mark {
    width: 28px;
    height: 28px;
    border-radius: 6px;
    background: linear-gradient(135deg, var(--cyan) 0%, #0066ff 100%);
    display: grid;
    place-items: center;
    font-family: var(--font-display);
    font-weight: 700;
    font-size: 14px;
    color: var(--bg-0);
    box-shadow: 0 0 16px var(--cyan-dim);
  }
  .logo h1 {
    font-family: var(--font-display);
    font-size: 15px;
    font-weight: 700;
    letter-spacing: 4px;
    color: var(--text-0);
    text-transform: uppercase;
    line-height: 1;
  }
  .header-center {
    flex: 1;
    display: flex;
    align-items: center;
    padding: 0 20px;
    gap: 10px;
    min-width: 0;
    overflow: visible;
  }
  .obj-text {
    font-size: 12px;
    color: var(--text-1);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    flex: 1;
    min-width: 0;
  }
  .obj-text.live-status {
    color: var(--cyan);
    font-size: 11px;
    animation: statusPulse 3s ease-in-out infinite;
  }
  @keyframes statusPulse {
    0%, 100% { opacity: 0.8; }
    50% { opacity: 1; }
  }
  .obj-placeholder {
    font-style: italic;
    color: var(--text-3);
  }
  .meta-chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 3px 10px;
    border-radius: 4px;
    font-size: 10px;
    font-family: var(--font-display);
    font-weight: 600;
    letter-spacing: 0.5px;
    flex-shrink: 0;
    border: 1px solid;
    cursor: pointer;
    transition: background 0.15s ease;
  }
  .meta-chip.session {
    background: var(--cyan-dim);
    color: var(--cyan);
    border-color: #00d4ff33;
    font-variant-numeric: tabular-nums;
  }

  /* Branch chip shared */
  .branch-chip-wrapper {
    position: relative;
    flex-shrink: 0;
  }
  .meta-chip.branch-back,
  .meta-chip.branch-front {
    gap: 4px;
    padding: 3px 8px;
    background: transparent;
  }
  .meta-chip.branch-back {
    background: #6c3baa18;
    color: #c084fc;
    border-color: #6c3baa33;
  }
  .meta-chip.branch-back:hover {
    background: #6c3baa30;
  }
  .meta-chip.branch-front {
    background: #2563eb18;
    color: #60a5fa;
    border-color: #2563eb33;
  }
  .meta-chip.branch-front:hover {
    background: #2563eb30;
  }
  .branch-label {
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 1px;
    opacity: 0.7;
  }
  .branch-name {
    max-width: 120px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .branch-dirty {
    color: var(--amber);
    font-weight: 700;
    font-size: 12px;
    line-height: 1;
  }

  /* Branch dropdown */
  .branch-dropdown {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    min-width: 220px;
    max-height: 300px;
    overflow-y: auto;
    background: var(--bg-2);
    border: 1px solid var(--border-bright);
    border-radius: 6px;
    box-shadow: 0 8px 24px rgba(0,0,0,0.5);
    z-index: 100;
    padding: 4px;
  }
  .branch-loading {
    padding: 12px 16px;
    font-size: 11px;
    color: var(--text-2);
    text-align: center;
  }
  .branch-option {
    display: block;
    width: 100%;
    text-align: left;
    padding: 6px 12px;
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-1);
    background: transparent;
    border: none;
    border-radius: 4px;
    cursor: pointer;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .branch-option:hover {
    background: var(--bg-3);
    color: var(--text-0);
  }
  .branch-option.current {
    color: var(--green);
    font-weight: 600;
  }
  .branch-option:disabled {
    opacity: 0.5;
    cursor: wait;
  }

  .editable {
    cursor: pointer;
    position: relative;
    transition: background 0.15s ease;
  }
  .editable:hover { background: var(--bg-3); }
  .editable::after {
    content: '';
    display: inline-block;
    width: 10px; height: 10px;
    margin-left: 4px;
    background: url("data:image/svg+xml,%3Csvg viewBox='0 0 16 16' fill='none' stroke='%234a5a6a' stroke-width='1.5' xmlns='http://www.w3.org/2000/svg'%3E%3Cpath d='M11.5 1.5l3 3-9 9H2.5v-3z'/%3E%3C/svg%3E") center/contain no-repeat;
    opacity: 0;
    transition: opacity 0.15s ease;
    vertical-align: middle;
    flex-shrink: 0;
  }
  .editable:hover::after { opacity: 1; }
  .obj-text.editable::after {
    position: absolute;
    right: 0;
    top: 50%;
    transform: translateY(-50%);
  }
  .obj-text.editable { padding-right: 16px; }
  .rounds {
    display: flex;
    gap: 3px;
    align-items: center;
    flex-shrink: 0;
  }
  .round-pip {
    width: 18px;
    height: 18px;
    border-radius: 4px;
    display: grid;
    place-items: center;
    font-family: var(--font-display);
    font-size: 8px;
    font-weight: 700;
    border: 1px solid;
    letter-spacing: 0;
    transition: background 0.2s ease, color 0.2s ease;
  }
  .round-pip.done {
    background: var(--green-dim);
    color: var(--green);
    border-color: #00ff8844;
  }
  .round-pip.active {
    background: var(--amber-dim);
    color: var(--amber);
    border-color: #ffb80044;
    animation: blink 1.5s infinite;
  }
  .round-pip.pending {
    background: transparent;
    color: var(--text-3);
    border-color: var(--border);
  }
  .header-right {
    display: flex;
    align-items: center;
    gap: 14px;
    padding-left: 16px;
    border-left: 1px solid var(--border);
    flex-shrink: 0;
  }
  .live-indicator {
    display: none;
    align-items: center;
    gap: 5px;
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 2px;
    color: var(--red);
    text-transform: uppercase;
  }
  .live-indicator.on { display: flex; }
  .live-dot {
    width: 6px; height: 6px;
    border-radius: 50%;
    background: var(--red);
    box-shadow: 0 0 8px var(--red);
    animation: blink 1.5s infinite;
  }
  /* Voice button */
  .voice-btn {
    background: var(--bg-1);
    border: 1px solid var(--border);
    color: var(--text-2);
    padding: 3px 8px;
    border-radius: 12px;
    cursor: pointer;
    font-size: 14px;
    transition: all 0.2s;
    line-height: 1;
  }
  .voice-btn:hover { border-color: var(--border-bright); color: var(--text-0); }
  .voice-btn.voice-connecting {
    color: var(--cyan);
    border-color: rgba(0,212,255,0.3);
    animation: voice-pulse 1s ease-in-out infinite;
  }
  .voice-btn.voice-listening {
    color: var(--cyan);
    border-color: var(--cyan);
    background: rgba(0,212,255,0.1);
    box-shadow: 0 0 8px rgba(0,212,255,0.3);
    animation: voice-pulse 2s ease-in-out infinite;
  }
  .voice-btn.voice-speaking {
    color: #64ffcc;
    border-color: #64ffcc;
    background: rgba(100,255,200,0.1);
    box-shadow: 0 0 8px rgba(100,255,200,0.3);
    animation: voice-speak 0.6s ease-in-out infinite alternate;
  }
  @keyframes voice-pulse {
    0%, 100% { transform: scale(1); }
    50% { transform: scale(1.1); }
  }
  @keyframes voice-speak {
    from { transform: scale(1); }
    to { transform: scale(1.15); }
  }

  .clock {
    font-variant-numeric: tabular-nums;
    font-size: 10px;
    color: var(--text-2);
    letter-spacing: 0.5px;
    font-family: var(--font-mono);
  }

  /* Running task spinner */
  .task-spinner {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    position: relative;
    cursor: default;
  }
  .spinner-ring {
    display: block;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    border: 2px solid #00d4ff33;
    border-top-color: var(--cyan);
    animation: spin 0.8s linear infinite;
    flex-shrink: 0;
  }
  @keyframes spin {
    to { transform: rotate(360deg); }
  }
  .spinner-count {
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 700;
    color: var(--cyan);
    line-height: 1;
  }
  /* Edit modal */
  .edit-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0,0,0,0.6);
    z-index: 1000;
    display: flex;
    align-items: center;
    justify-content: center;
    backdrop-filter: blur(4px);
  }
  .edit-modal {
    background: var(--bg-2);
    border: 1px solid var(--border-bright);
    border-radius: 10px;
    padding: 20px 24px;
    width: 480px;
    max-width: 90vw;
    box-shadow: 0 16px 48px rgba(0,0,0,0.5), 0 0 0 1px var(--border);
  }
  .edit-modal h3 {
    font-family: var(--font-display);
    font-size: 13px;
    font-weight: 700;
    color: var(--text-0);
    letter-spacing: 1px;
    text-transform: uppercase;
    margin-bottom: 16px;
  }
  .edit-field {
    margin-bottom: 12px;
  }
  .edit-field label {
    display: block;
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 600;
    color: var(--text-2);
    text-transform: uppercase;
    letter-spacing: 1.5px;
    margin-bottom: 4px;
  }
  .edit-field input, .edit-field textarea {
    width: 100%;
    background: var(--bg-0);
    color: var(--text-0);
    border: 1px solid var(--border);
    padding: 8px 12px;
    border-radius: var(--radius);
    font-family: var(--font-mono);
    font-size: 12px;
    transition: border-color 0.15s ease;
  }
  .edit-field input:focus, .edit-field textarea:focus {
    outline: none;
    border-color: var(--cyan);
    box-shadow: 0 0 0 3px var(--cyan-dim);
  }
  .edit-field textarea { resize: vertical; min-height: 60px; }
  .edit-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 16px;
  }
  .edit-actions button {
    font-family: var(--font-display);
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 1px;
    padding: 8px 20px;
    border-radius: var(--radius);
    cursor: pointer;
    text-transform: uppercase;
    border: 1px solid;
    transition: background 0.15s ease;
  }
  .btn-cancel {
    background: transparent;
    color: var(--text-1);
    border-color: var(--border-bright);
  }
  .btn-cancel:hover { background: var(--bg-3); }
  .btn-save {
    background: linear-gradient(180deg, #0088cc 0%, #006699 100%);
    color: #fff;
    border-color: #0099dd;
    box-shadow: 0 2px 8px rgba(0,136,204,0.2);
  }
  .btn-save:hover { background: linear-gradient(180deg, #0099dd 0%, #0077aa 100%); }
  .pixel-toggle {
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 700;
    color: var(--text-2);
    background: var(--bg-3);
    border: 1px solid var(--border);
    border-radius: 3px;
    padding: 2px 8px;
    cursor: pointer;
    letter-spacing: 1px;
    transition: all 0.15s ease;
  }
  .pixel-toggle:hover { background: var(--bg-2); border-color: var(--border-bright); }
  .pixel-toggle.pixel-active {
    color: #4CAF50;
    border-color: #4CAF5044;
    background: #4CAF5018;
  }
  .lang-toggle {
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 700;
    color: var(--text-1);
    background: var(--bg-3);
    border: 1px solid var(--border);
    border-radius: 3px;
    padding: 2px 8px;
    cursor: pointer;
    letter-spacing: 1px;
    transition: all 0.15s ease;
  }
  .lang-toggle:hover { background: var(--bg-2); border-color: var(--border-bright); color: var(--text-0); }
  .theme-toggle {
    font-size: 13px;
    color: var(--text-1);
    background: var(--bg-3);
    border: 1px solid var(--border);
    border-radius: 3px;
    padding: 1px 7px;
    cursor: pointer;
    transition: all 0.15s ease;
    line-height: 1;
  }
  .theme-toggle:hover { background: var(--bg-2); border-color: var(--border-bright); color: var(--text-0); }
  .mc-toggle {
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 700;
    color: var(--cyan);
    background: var(--bg-3);
    border: 1px solid var(--border);
    border-radius: 3px;
    padding: 2px 8px;
    cursor: pointer;
    letter-spacing: 1px;
    transition: all 0.15s ease;
  }
  .mc-toggle:hover { background: var(--cyan-dim); border-color: #00d4ff44; }
  .settings-btn {
    font-family: var(--font-display);
    font-size: 13px;
    color: var(--cyan);
    background: var(--bg-3);
    border: 1px solid var(--border);
    border-radius: 3px;
    padding: 2px 7px;
    cursor: pointer;
    transition: all 0.15s ease;
    line-height: 1;
  }
  .settings-btn:hover { background: var(--cyan-dim); border-color: #00d4ff44; }

  /* Workspace switcher */
  .workspace-wrapper {
    position: relative;
    flex-shrink: 0;
  }
  .workspace-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 600;
    color: var(--text-2);
    background: var(--bg-3);
    border: 1px solid var(--border);
    border-radius: 3px;
    padding: 2px 8px;
    cursor: pointer;
    letter-spacing: 0.5px;
    transition: all 0.15s ease;
    max-width: 120px;
  }
  .workspace-btn:hover { background: var(--bg-2); border-color: var(--border-bright); color: var(--text-1); }
  .workspace-icon { font-size: 10px; opacity: 0.7; }
  .workspace-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 70px;
  }
  .workspace-caret { font-size: 7px; opacity: 0.6; flex-shrink: 0; }
  .workspace-dropdown {
    position: absolute;
    top: calc(100% + 4px);
    right: 0;
    min-width: 200px;
    max-width: 280px;
    background: var(--bg-2);
    border: 1px solid var(--border-bright);
    border-radius: 6px;
    box-shadow: 0 8px 24px rgba(0,0,0,0.5);
    z-index: 200;
    padding: 4px;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .workspace-empty {
    padding: 10px 12px;
    font-size: 10px;
    color: var(--text-3);
    font-style: italic;
    text-align: center;
  }
  .workspace-item {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    text-align: left;
    padding: 6px 10px;
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--text-1);
    background: transparent;
    border: none;
    border-radius: 4px;
    cursor: pointer;
    transition: background 0.1s ease;
  }
  .workspace-item:hover { background: var(--bg-3); color: var(--text-0); }
  .workspace-item.ws-current { color: var(--cyan); }
  .ws-name { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ws-meta {
    font-size: 9px;
    color: var(--text-3);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 60px;
    flex-shrink: 0;
  }
  .ws-check { color: var(--green); font-size: 10px; flex-shrink: 0; }
  .workspace-divider {
    height: 1px;
    background: var(--border);
    margin: 2px 4px;
  }
  .workspace-add-btn {
    width: 100%;
    text-align: left;
    padding: 6px 10px;
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 600;
    letter-spacing: 0.5px;
    color: var(--cyan);
    background: transparent;
    border: none;
    border-radius: 4px;
    cursor: pointer;
    transition: background 0.1s ease;
  }
  .workspace-add-btn:hover { background: var(--cyan-dim); }
  .workspace-save-form {
    display: flex;
    gap: 4px;
    padding: 4px 6px;
  }
  .workspace-name-input {
    flex: 1;
    background: var(--bg-0);
    color: var(--text-0);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 4px 8px;
    font-family: var(--font-mono);
    font-size: 10px;
    min-width: 0;
  }
  .workspace-name-input:focus {
    outline: none;
    border-color: var(--cyan);
    box-shadow: 0 0 0 2px var(--cyan-dim);
  }
  .workspace-save-confirm {
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 700;
    color: var(--bg-0);
    background: var(--cyan);
    border: none;
    border-radius: var(--radius);
    padding: 4px 10px;
    cursor: pointer;
    flex-shrink: 0;
    transition: opacity 0.15s ease;
  }
  .workspace-save-confirm:disabled { opacity: 0.5; cursor: default; }
  .workspace-save-confirm:not(:disabled):hover { opacity: 0.85; }

  /* ── Responsive ─────────────────────────────────────────── */
  @media (max-width: 900px) {
    /* Hide branch chips to reclaim space */
    .branch-chip-wrapper { display: none; }
    /* Shrink header center padding */
    .header-center { padding: 0 8px; gap: 6px; }
    /* Hide workspace name text, keep icon */
    .workspace-name { display: none; }
    .workspace-btn { max-width: unset; padding: 2px 6px; }
  }

  @media (max-width: 600px) {
    /* Keep only essential header-right items visible */
    .live-indicator { display: none !important; }
    .clock { display: none; }
    /* Tighten gaps */
    .header-right { gap: 6px; padding-left: 8px; }
    .header { padding: 0 8px; }
    /* Objective text: shorter ellipsis */
    .obj-text { max-width: 120px; }
  }

  /* ── Notification bell ──────────────────────────────────── */
  .notif-wrapper {
    position: relative;
    flex-shrink: 0;
  }
  .notif-btn {
    position: relative;
    font-size: 13px;
    color: var(--text-1);
    background: var(--bg-3);
    border: 1px solid var(--border);
    border-radius: 3px;
    padding: 1px 7px;
    cursor: pointer;
    transition: all 0.15s ease;
    line-height: 1;
    display: inline-flex;
    align-items: center;
    gap: 3px;
  }
  .notif-btn:hover { background: var(--bg-2); border-color: var(--border-bright); color: var(--text-0); }
  .notif-badge {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 14px;
    height: 14px;
    border-radius: 7px;
    background: var(--red, #ef4444);
    color: #fff;
    font-family: var(--font-display);
    font-size: 8px;
    font-weight: 700;
    padding: 0 3px;
    line-height: 1;
  }
  .notif-dropdown {
    position: absolute;
    top: calc(100% + 4px);
    right: 0;
    width: 300px;
    max-height: 360px;
    overflow-y: auto;
    background: var(--bg-2);
    border: 1px solid var(--border-bright);
    border-radius: 6px;
    box-shadow: 0 8px 24px rgba(0,0,0,0.5);
    z-index: 200;
    display: flex;
    flex-direction: column;
  }
  .notif-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 12px 6px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }
  .notif-title {
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 1.5px;
    text-transform: uppercase;
    color: var(--text-2);
  }
  .notif-clear {
    font-family: var(--font-display);
    font-size: 8px;
    font-weight: 600;
    color: var(--text-3);
    background: transparent;
    border: none;
    cursor: pointer;
    padding: 0;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    transition: color 0.1s ease;
  }
  .notif-clear:hover { color: var(--text-1); }
  .notif-empty {
    padding: 16px 12px;
    font-size: 11px;
    color: var(--text-3);
    font-style: italic;
    text-align: center;
  }
  .notif-item {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 7px 12px;
    border-bottom: 1px solid var(--border);
    border-left: 2px solid transparent;
  }
  .notif-item:last-child { border-bottom: none; }
  .notif-item.notif-success { border-left-color: var(--green, #22c55e); }
  .notif-item.notif-error   { border-left-color: var(--red, #ef4444); }
  .notif-item.notif-warning { border-left-color: var(--amber, #f59e0b); }
  .notif-item.notif-info    { border-left-color: var(--cyan, #00d4ff); }
  .notif-msg {
    flex: 1;
    font-size: 10px;
    color: var(--text-1);
    line-height: 1.4;
    word-break: break-word;
  }
  .notif-time {
    font-size: 9px;
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
    flex-shrink: 0;
    padding-top: 1px;
  }
</style>
