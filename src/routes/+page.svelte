<script lang="ts">
  import Header from '$lib/components/Header.svelte';
  import AgentPanel from '$lib/components/AgentPanel.svelte';
  import ResizeHandle from '$lib/components/ResizeHandle.svelte';
  import CommandBar from '$lib/components/CommandBar.svelte';
  import StatusBar from '$lib/components/StatusBar.svelte';
  import SidePanel from '$lib/components/SidePanel.svelte';
  import DashboardTab from '$lib/components/tabs/DashboardTab.svelte';
  import CommitsTab from '$lib/components/tabs/CommitsTab.svelte';
  import TasksTab from '$lib/components/tabs/TasksTab.svelte';
  import MachinesTab from '$lib/components/tabs/MachinesTab.svelte';
  import PipelinesTab from '$lib/components/tabs/PipelinesTab.svelte';
  import GithubTab from '$lib/components/tabs/GithubTab.svelte';
  import LogsTab from '$lib/components/tabs/LogsTab.svelte';
  import DiffTab from '$lib/components/tabs/DiffTab.svelte';
  import CostsTab from '$lib/components/tabs/CostsTab.svelte';
  import TimelineTab from '$lib/components/tabs/TimelineTab.svelte';
  import EventsTab from '$lib/components/tabs/EventsTab.svelte';
  import CapabilitiesTab from '$lib/components/tabs/CapabilitiesTab.svelte';
  import SettingsTab from '$lib/components/tabs/SettingsTab.svelte';
  import CronTab from '$lib/components/tabs/CronTab.svelte';
  import WhatsAppTab from '$lib/components/tabs/WhatsAppTab.svelte';
  import DocsTab from '$lib/components/tabs/DocsTab.svelte';
  import ResearchTab from '$lib/components/tabs/ResearchTab.svelte';
  import SessionsTab from '$lib/components/tabs/SessionsTab.svelte';
  import PlanningModal from '$lib/components/PlanningModal.svelte';
  import MissionControl from '$lib/components/MissionControl.svelte';
  import PixelRooms from '$lib/components/PixelRooms.svelte';
  import SetupWizard from '$lib/components/SetupWizard.svelte';
  import LoadingScreen from '$lib/components/LoadingScreen.svelte';
  import ShortcutsOverlay from '$lib/components/ShortcutsOverlay.svelte';
  import SearchModal from '$lib/components/SearchModal.svelte';
  import ConfirmModal from '$lib/components/ConfirmModal.svelte';
  import { initSessionStore, destroySessionStore, atlasFeed, pixelFeed, session, atlasAgentInfo, pixelAgentInfo, clearFeed } from '$lib/stores/session';
  import { initTasksStore, destroyTasksStore, runningCount } from '$lib/stores/tasks';
  import { initPlanningStore, destroyPlanningStore, planningModalOpen } from '$lib/stores/planning';
  import { machines, refreshMachinesStore, markMachineOnline, markMachineOffline } from '$lib/stores/machines';
  import { isFirstLaunch, executeAction, listDir, sendTask } from '$lib/api';
  import type { DirEntry } from '$lib/api';
  import { listen } from '@tauri-apps/api/event';
  import { addToast } from '$lib/stores/notifications';
  import { sendNativeNotification } from '$lib/utils/notifications';
  import { t, tr } from '$lib/i18n';
  import { onMount, tick } from 'svelte';

  const TAB_ICONS: Record<string, string> = {
    Overview: '⊞', Commits: '⊙', Diff: '≠', GitHub: '⑂', Tareas: '▶', Sesiones: '◉', Pipelines: '⟿',
    Maquinas: '◈', Capacidades: '⬡', Timeline: '∿', Eventos: '≡', Logs: '▷', Costos: '$', Cron: '⏰', Ajustes: '⊕', WhatsApp: '💬', Docs: '📄', Research: '🔍',
  };
  const tabs = ['Overview', 'Commits', 'Diff', 'GitHub', 'Tareas', 'Sesiones', 'Pipelines', 'Maquinas', 'Capacidades', 'Timeline', 'Eventos', 'Logs', 'Costos', 'Cron', 'WhatsApp', 'Docs', 'Research'];
  let activeTab = $state('Overview');
  let showWizard = $state(false);
  let loadingVisible = $state(true);
  let loadingProgress = $state(0);
  let loadingStep = $state('Iniciando...');
  let settingsOpen = $state(false);
  let mcOpen = $state(typeof localStorage !== 'undefined' ? localStorage.getItem('jarvis-mc-open') !== 'false' : true);
  let shortcutsOpen = $state(false);
  let searchOpen = $state(false);
  let showKillAllConfirm = $state(false);
  let panelsOpen = $state(typeof localStorage !== 'undefined' ? localStorage.getItem('jarvis-panels-open') !== 'false' : true);

  function togglePanels() {
    panelsOpen = !panelsOpen;
    localStorage.setItem('jarvis-panels-open', String(panelsOpen));
  }

  // Chat panel
  let chatFeedEl: HTMLDivElement | undefined;
  let mainEl: HTMLElement | undefined;
  let chatHeightPct = $state(
    typeof localStorage !== 'undefined'
      ? parseFloat(localStorage.getItem('jarvis-chat-height') || '45')
      : 45
  );
  let draggingChat = $state(false);

  // Chat view mode: 'human' (readable) or 'tech' (raw everything)
  let chatMode = $state<'human' | 'tech'>(
    (typeof localStorage !== 'undefined' ? localStorage.getItem('jarvis-chat-mode') : null) as 'human' | 'tech' || 'human'
  );
  function toggleChatMode() {
    chatMode = chatMode === 'human' ? 'tech' : 'human';
    localStorage.setItem('jarvis-chat-mode', chatMode);
  }

  // Directory browser per machine
  interface DirBrowserState {
    open: boolean;
    currentPath: string;
    entries: DirEntry[];
    loading: boolean;
    selectedPath: string;
  }
  function defaultDirState(machine: 'atlas' | 'pixel'): DirBrowserState {
    const storedPath = typeof localStorage !== 'undefined'
      ? localStorage.getItem(`jarvis-dir-${machine}`) || ''
      : '';
    return { open: false, currentPath: '', entries: [], loading: false, selectedPath: storedPath };
  }
  let atlasDir = $state<DirBrowserState>(defaultDirState('atlas'));
  let pixelDir = $state<DirBrowserState>(defaultDirState('pixel'));

  function getDirState(machine: 'atlas' | 'pixel') { return machine === 'atlas' ? atlasDir : pixelDir; }

  async function openDirBrowser(machine: 'atlas' | 'pixel') {
    const state = getDirState(machine);
    const startPath = state.selectedPath || '~';
    state.open = true;
    await navigateDir(machine, startPath);
  }

  async function navigateDir(machine: 'atlas' | 'pixel', path: string) {
    const state = getDirState(machine);
    state.loading = true;
    state.currentPath = path;
    try {
      state.entries = await listDir(path);
    } catch (e) {
      state.entries = [];
      addToast('Error: ' + e, 'error');
    } finally {
      state.loading = false;
    }
  }

  function selectDir(machine: 'atlas' | 'pixel', path: string) {
    const state = getDirState(machine);
    state.selectedPath = path;
    state.open = false;
    localStorage.setItem(`jarvis-dir-${machine}`, path);
  }

  function dirDisplayName(path: string) {
    if (!path) return '—';
    const parts = path.split('/');
    return parts[parts.length - 1] || path;
  }

  function parentDir(path: string) {
    const parts = path.split('/');
    parts.pop();
    return parts.join('/') || '/';
  }

  // Parse text blocks to detect speaker changes for human mode
  interface ParsedBlock {
    speaker: 'atlas' | 'pixel' | 'system';
    text: string;
  }
  function parseBlocks(feed: typeof $atlasFeed, defaultSpeaker: 'atlas' | 'pixel'): ParsedBlock[] {
    const blocks: ParsedBlock[] = [];
    let current: ParsedBlock = { speaker: defaultSpeaker, text: '' };

    for (const item of feed) {
      if (item.type !== 'text' && item.type !== 'prompt') continue;
      const content = item.content || '';
      if (!content.trim()) continue;

      // Split by lines to detect speaker markers
      const lines = content.split('\n');
      for (const line of lines) {
        const trimmed = line.trim();
        if (/^#{1,3}\s*(ATLAS|atlas)\b/.test(trimmed) || /^##\s*ATLAS\s/.test(trimmed)) {
          if (current.text.trim()) blocks.push({ ...current });
          current = { speaker: 'atlas', text: '' };
        } else if (/^#{1,3}\s*(PIXEL|pixel)\b/.test(trimmed) || /^##\s*PIXEL\s/.test(trimmed)) {
          if (current.text.trim()) blocks.push({ ...current });
          current = { speaker: 'pixel', text: '' };
        } else if (trimmed === '---') {
          if (current.text.trim()) blocks.push({ ...current });
          current = { speaker: current.speaker, text: '' };
          continue;
        }
        current.text += line + '\n';
      }
    }
    if (current.text.trim()) blocks.push(current);
    return blocks;
  }

  let atlasBlocks = $derived(parseBlocks($atlasFeed, 'atlas'));
  let pixelBlocks = $derived(parseBlocks($pixelFeed, 'pixel'));

  function onChatDragStart(e: MouseEvent) {
    e.preventDefault();
    draggingChat = true;
    const onMove = (ev: MouseEvent) => {
      if (!mainEl) return;
      const rect = mainEl.getBoundingClientRect();
      const pct = ((rect.bottom - ev.clientY) / rect.height) * 100;
      chatHeightPct = Math.max(10, Math.min(80, pct));
    };
    const onUp = () => {
      draggingChat = false;
      localStorage.setItem('jarvis-chat-height', String(chatHeightPct));
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
    };
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
  }

  $effect(() => {
    $atlasFeed; $pixelFeed;
    if (chatFeedEl) tick().then(() => { if (chatFeedEl) chatFeedEl.scrollTop = chatFeedEl.scrollHeight; });
  });

  // Global keyboard shortcuts
  function handleGlobalKeydown(e: KeyboardEvent) {
    const mod = e.metaKey || e.ctrlKey;

    // Ctrl+/ or ? — Toggle shortcuts overlay
    if ((mod && e.key === '/') || (e.key === '?' && !mod && !(e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement || e.target instanceof HTMLSelectElement))) {
      e.preventDefault();
      shortcutsOpen = !shortcutsOpen;
      return;
    }

    // Ctrl+K / Cmd+K — Open global search (without Shift)
    if (mod && !e.shiftKey && e.key === 'k') {
      e.preventDefault();
      searchOpen = true;
      return;
    }

    // Escape — Close any open modal/overlay
    if (e.key === 'Escape') {
      if (searchOpen) { searchOpen = false; return; }
      if (shortcutsOpen) { shortcutsOpen = false; return; }
      if (showKillAllConfirm) { showKillAllConfirm = false; return; }
      // PlanningModal and CommandBar handle their own Escape
      return;
    }

    // Ctrl+P / Cmd+P — Open Planning Modal
    if (mod && e.key === 'p') {
      e.preventDefault();
      planningModalOpen.set(true);
      return;
    }

    // Ctrl+B / Cmd+B — Toggle bottom panels
    if (mod && e.key === 'b') {
      e.preventDefault();
      togglePanels();
      return;
    }

    // Ctrl+Shift+K / Cmd+Shift+K — Kill all (with confirmation)
    if (mod && e.shiftKey && e.key === 'K') {
      e.preventDefault();
      showKillAllConfirm = true;
      return;
    }

    // Ctrl+1..0 — Switch tabs (1=first tab, 0=10th tab)
    if (mod && !e.shiftKey && !e.altKey) {
      const digit = parseInt(e.key, 10);
      if (!isNaN(digit)) {
        const idx = digit === 0 ? 9 : digit - 1;
        if (idx < tabs.length) {
          e.preventDefault();
          activeTab = tabs[idx];
        }
        return;
      }
    }
  }

  async function handleSearchAction(action: string) {
    if (action === 'kill-all') {
      showKillAllConfirm = true;
    } else if (action === 'git-pull') {
      addToast(t('cmd.executingGitPull'), 'info');
      try {
        await executeAction('git-pull');
        addToast(t('cmd.gitPullDone'), 'success');
      } catch (e) {
        addToast('Error: ' + (typeof e === 'string' ? e : String(e)), 'error');
      }
    } else if (action === 'planning') {
      planningModalOpen.set(true);
    } else if (action === 'clear-history') {
      addToast(t('cmd.historyCleared'), 'info');
    } else if (action.startsWith('redispatch:')) {
      // Format: "redispatch:target:prompt"
      const parts = action.split(':');
      const target = parts[1] ?? '';
      const prompt = parts.slice(2).join(':');
      if (target && prompt) {
        try {
          await sendTask(target, prompt);
          addToast(t('cmd.taskSent').replace('{target}', target.toUpperCase()), 'success');
        } catch (e) {
          addToast('Error: ' + (typeof e === 'string' ? e : String(e)), 'error');
        }
      }
    }
  }

  async function confirmKillAll() {
    showKillAllConfirm = false;
    addToast(t('page.killingAgents'), 'info');
    try {
      await executeAction('kill-all');
      addToast(t('page.agentsStopped'), 'success');
    } catch (e) {
      addToast('Error: ' + (typeof e === 'string' ? e : String(e)), 'error');
    }
  }

  function toggleMC() {
    mcOpen = !mcOpen;
    localStorage.setItem('jarvis-mc-open', String(mcOpen));
  }

  let badges: Record<string, number> = $derived({
    Tareas: $runningCount
  });

  onMount(() => {
    // Sequential init with real progress tracking
    (async () => {
      loadingStep = 'Cargando sesión...';
      loadingProgress = 5;
      initSessionStore();
      await tick();
      loadingProgress = 25;

      loadingStep = 'Cargando tareas...';
      initTasksStore();
      await tick();
      loadingProgress = 50;

      loadingStep = 'Inicializando planificación...';
      initPlanningStore();
      await tick();
      loadingProgress = 65;

      loadingStep = 'Verificando máquinas...';
      // Don't block loading — refresh machines in background
      refreshMachinesStore().catch(() => {});
      loadingProgress = 85;

      loadingStep = 'Verificando primera ejecución...';
      const first = await isFirstLaunch().catch(() => false);
      showWizard = first;
      loadingProgress = 100;
      loadingStep = 'Listo';

      // Fade out, then hide
      await new Promise(r => setTimeout(r, 300));
      loadingVisible = false;
    })();

    if (typeof window !== 'undefined') {
      window.__jarvis_toggle_mc = toggleMC;
      window.__jarvis_open_settings = () => { settingsOpen = true; };
    }

    const unlistens: Array<() => void> = [];

    // Listen for task completion
    listen<{id: number; target: string; output: string}>('task-done', (event) => {
      sendNativeNotification('Task completed', `Task #${event.payload.id} finished on ${event.payload.target}`);
    }).then(fn => unlistens.push(fn));

    // Listen for automation rule alerts
    listen<{rule: string, message: string}>('rule-alert', (event) => {
      addToast(t('page.ruleAlert', {rule: event.payload.rule, message: event.payload.message}), 'info');
      sendNativeNotification('Rule alert: ' + event.payload.rule, event.payload.message);
    }).then(fn => unlistens.push(fn));

    listen<{message: string}>('repo-conflict', (event) => {
      addToast(t('page.conflict', {message: event.payload.message}), 'error');
      sendNativeNotification('Conflict detected', event.payload.message);
    }).then(fn => unlistens.push(fn));

    listen<{target: string, reason: string}>('auto-routed', (event) => {
      addToast(t('page.autoRouted', {target: event.payload.target, reason: event.payload.reason}), 'info');
    }).then(fn => unlistens.push(fn));

    listen<{url: string; error: string; attempts: number}>('webhook-failed', (event) => {
      addToast(`Webhook failed after ${event.payload.attempts} attempts: ${event.payload.url}`, 'error');
    }).then(fn => unlistens.push(fn));

    listen<{name: string}>('workspace-switched', (event) => {
      addToast(`Workspace switched to: ${event.payload.name}`, 'success');
    }).then(fn => unlistens.push(fn));

    listen<{id: string, name: string}>('machine-reconnected', (event) => {
      markMachineOnline(event.payload.id);
      addToast(`${event.payload.name} reconnected`, 'success');
    }).then(fn => unlistens.push(fn));

    listen<{id: string, name: string, attempts: number}>('machine-offline', (event) => {
      markMachineOffline(event.payload.id);
      addToast(`${event.payload.name} is offline`, 'error');
    }).then(fn => unlistens.push(fn));

    // Listen for tab switch events from CommandBar
    const onSwitchTab = (e: Event) => {
      const tab = (e as CustomEvent<string>).detail;
      if (tab) {
        activeTab = tab;
        settingsOpen = false;
        if (!panelsOpen) togglePanels();
      }
    };
    window.addEventListener('jarvis-switch-tab', onSwitchTab);

    return () => {
      window.removeEventListener('jarvis-switch-tab', onSwitchTab);
      destroySessionStore();
      destroyTasksStore();
      destroyPlanningStore();
      unlistens.forEach(fn => fn());
    };
  });
</script>

<svelte:window onkeydown={handleGlobalKeydown} />

<LoadingScreen progress={loadingProgress} step={loadingStep} visible={loadingVisible} />

{#if showWizard}
  <SetupWizard onComplete={() => { showWizard = false; window.location.reload(); }} />
{/if}

<Header />

<div class="app-body">
  <SidePanel bind:activePanel={activeTab} {badges} onTogglePanels={togglePanels} {panelsOpen} />
  <div class="content-area">
    <main class="main" id="main-content" aria-label="Paneles de agentes" bind:this={mainEl}>
      <PixelRooms />
      <!-- Two-column chat overlay aligned with orbs -->
      <div class="chat-overlay" style="height:{chatHeightPct}%">
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="chat-resize-handle" class:dragging={draggingChat} onmousedown={onChatDragStart}></div>
        <div class="chat-mode-bar">
          <button class="chat-action-btn mode" onclick={toggleChatMode}>{chatMode === 'human' ? 'Humano' : 'Tecnico'}</button>
        </div>
        <div class="chat-columns">
          <!-- ATLAS column -->
          <div class="chat-col">
            <div class="chat-col-header atlas">
              <span class="chat-dot" class:active={$session.atlasRunning}></span>
              <span class="chat-name atlas-name">ATLAS</span>
              <span class="chat-tag">Backend</span>
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <div class="repo-picker" class:open={atlasDir.open}>
                <!-- svelte-ignore a11y_click_events_have_key_events -->
                <span class="repo-current" onclick={() => openDirBrowser('atlas')}>
                  {atlasDir.selectedPath ? dirDisplayName(atlasDir.selectedPath) : 'dir'}
                  <span class="repo-chevron">▾</span>
                </span>
                {#if atlasDir.open}
                  <div class="repo-dropdown dir-browser">
                    <div class="dir-nav">
                      <!-- svelte-ignore a11y_click_events_have_key_events -->
                      <span class="dir-back" onclick={() => navigateDir('atlas', parentDir(atlasDir.currentPath))}>‹ arriba</span>
                      <span class="dir-current-path">{atlasDir.currentPath}</span>
                      <!-- svelte-ignore a11y_click_events_have_key_events -->
                      <span class="dir-select-here" onclick={() => selectDir('atlas', atlasDir.currentPath)}>✓ Usar</span>
                    </div>
                    {#if atlasDir.loading}
                      <div class="dir-loading">...</div>
                    {:else}
                      {#each atlasDir.entries as entry}
                        <!-- svelte-ignore a11y_click_events_have_key_events -->
                        <div class="repo-option dir-entry" onclick={() => navigateDir('atlas', entry.path)}>
                          <span class="dir-folder-icon">▷</span>
                          <span class="repo-opt-name">{entry.name}</span>
                        </div>
                      {/each}
                    {/if}
                  </div>
                {/if}
              </div>
              <span class="chat-col-actions">
                <button class="chat-action-btn run" onclick={() => { const input = document.getElementById('promptInput') as HTMLInputElement; if (input) { input.value = '@atlas '; input.focus(); } }}>Run</button>
                <button class="chat-action-btn stop" onclick={async () => { addToast('Stopping ATLAS...', 'info'); try { await executeAction('kill-atlas'); addToast('ATLAS stopped', 'success'); } catch(e) { addToast('Error: ' + e, 'error'); } }}>Stop</button>
                <button class="chat-action-btn clear" onclick={() => clearFeed('atlas')}>Clear</button>
              </span>
            </div>
            <div class="chat-feed" bind:this={chatFeedEl}>
              {#if chatMode === 'human'}
                {#each atlasBlocks as block}
                  <div class="cf-item cf-human">{block.text}</div>
                {/each}
              {:else}
                {#each $atlasFeed as item}
                  {#if item.type === 'prompt'}
                    <div class="cf-item cf-prompt">
                      <span class="cf-label">PROMPT</span>
                      <span class="cf-text">{item.content || ''}</span>
                    </div>
                  {:else if item.type === 'tool'}
                    <div class="cf-item cf-tool">
                      <span class="cf-badge">{item.name}</span>
                      <span class="cf-detail">{item.detail || ''}</span>
                    </div>
                  {:else}
                    <div class="cf-item cf-thought"><span>{item.content || ''}</span></div>
                  {/if}
                {/each}
              {/if}
              {#if $session.atlasRunning}
                <div class="cf-item cf-thinking"><span class="dots"><span></span><span></span><span></span></span></div>
              {/if}
            </div>
          </div>
          <!-- Vertical separator -->
          <div class="chat-col-sep"></div>
          <!-- PIXEL column -->
          <div class="chat-col">
            <div class="chat-col-header pixel">
              <span class="chat-dot" class:active={$session.pixelRunning}></span>
              <span class="chat-name pixel-name">PIXEL</span>
              <span class="chat-tag">Frontend</span>
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <div class="repo-picker" class:open={pixelDir.open}>
                <!-- svelte-ignore a11y_click_events_have_key_events -->
                <span class="repo-current" onclick={() => openDirBrowser('pixel')}>
                  {pixelDir.selectedPath ? dirDisplayName(pixelDir.selectedPath) : 'dir'}
                  <span class="repo-chevron">▾</span>
                </span>
                {#if pixelDir.open}
                  <div class="repo-dropdown dir-browser">
                    <div class="dir-nav">
                      <!-- svelte-ignore a11y_click_events_have_key_events -->
                      <span class="dir-back" onclick={() => navigateDir('pixel', parentDir(pixelDir.currentPath))}>‹ arriba</span>
                      <span class="dir-current-path">{pixelDir.currentPath}</span>
                      <!-- svelte-ignore a11y_click_events_have_key_events -->
                      <span class="dir-select-here" onclick={() => selectDir('pixel', pixelDir.currentPath)}>✓ Usar</span>
                    </div>
                    {#if pixelDir.loading}
                      <div class="dir-loading">...</div>
                    {:else}
                      {#each pixelDir.entries as entry}
                        <!-- svelte-ignore a11y_click_events_have_key_events -->
                        <div class="repo-option dir-entry" onclick={() => navigateDir('pixel', entry.path)}>
                          <span class="dir-folder-icon">▷</span>
                          <span class="repo-opt-name">{entry.name}</span>
                        </div>
                      {/each}
                    {/if}
                  </div>
                {/if}
              </div>
              <span class="chat-col-actions">
                <button class="chat-action-btn run" onclick={() => { const input = document.getElementById('promptInput') as HTMLInputElement; if (input) { input.value = '@pixel '; input.focus(); } }}>Run</button>
                <button class="chat-action-btn stop" onclick={async () => { addToast('Stopping PIXEL...', 'info'); try { await executeAction('kill-pixel'); addToast('PIXEL stopped', 'success'); } catch(e) { addToast('Error: ' + e, 'error'); } }}>Stop</button>
                <button class="chat-action-btn clear" onclick={() => clearFeed('pixel')}>Clear</button>
              </span>
            </div>
            <div class="chat-feed">
              {#if chatMode === 'human'}
                {#each pixelBlocks as block}
                  <div class="cf-item cf-human">{block.text}</div>
                {/each}
              {:else}
                {#each $pixelFeed as item}
                  {#if item.type === 'prompt'}
                    <div class="cf-item cf-prompt">
                      <span class="cf-label">PROMPT</span>
                      <span class="cf-text">{item.content || ''}</span>
                    </div>
                  {:else if item.type === 'tool'}
                    <div class="cf-item cf-tool">
                      <span class="cf-badge">{item.name}</span>
                      <span class="cf-detail">{item.detail || ''}</span>
                    </div>
                  {:else}
                    <div class="cf-item cf-thought"><span>{item.content || ''}</span></div>
                  {/if}
                {/each}
              {/if}
              {#if $session.pixelRunning}
                <div class="cf-item cf-thinking"><span class="dots"><span></span><span></span><span></span></span></div>
              {/if}
            </div>
          </div>
        </div>
      </div>
      {#if mcOpen}
        <MissionControl />
      {/if}
    </main>

    {#if panelsOpen}
      <ResizeHandle direction="vertical" />

      <div class="bottom-section" style="height:240px">
        <div class="tab-bar">
          {#each tabs as tab}
            <button
              class="tab-btn"
              class:active={activeTab === tab && !settingsOpen}
              onclick={() => { activeTab = tab; settingsOpen = false; }}
              title={tab}
            >
              <span class="tab-icon">{TAB_ICONS[tab] || '·'}</span>
              <span class="tab-label">{tab}</span>
              {#if badges[tab]}
                <span class="tab-badge">{badges[tab]}</span>
              {/if}
            </button>
          {/each}
        </div>
        <div class="tab-panels">
          {#if activeTab === 'Overview'}
            <DashboardTab onswitchTab={(tab) => { activeTab = tab; }} />
          {:else if activeTab === 'Commits'}
            <CommitsTab />
          {:else if activeTab === 'Tareas'}
            <TasksTab />
          {:else if activeTab === 'Sesiones'}
            <SessionsTab />
          {:else if activeTab === 'Maquinas'}
            <MachinesTab />
          {:else if activeTab === 'Capacidades'}
            <CapabilitiesTab />
          {:else if activeTab === 'Pipelines'}
            <PipelinesTab />
          {:else if activeTab === 'GitHub'}
            <GithubTab />
          {:else if activeTab === 'Diff'}
            <DiffTab />
          {:else if activeTab === 'Costos'}
            <CostsTab />
          {:else if activeTab === 'Timeline'}
            <TimelineTab />
          {:else if activeTab === 'Eventos'}
            <EventsTab />
          {:else if activeTab === 'Logs'}
            <LogsTab />
          {:else if activeTab === 'Cron'}
            <CronTab />
          {:else if activeTab === 'WhatsApp'}
            <WhatsAppTab />
          {:else if activeTab === 'Docs'}
            <DocsTab />
          {:else if activeTab === 'Research'}
            <ResearchTab />
          {/if}
        </div>
      </div>
    {/if}

    <CommandBar />
    <StatusBar />
  </div>
</div>
{#if settingsOpen}
  <div class="settings-page">
    <div class="settings-page-header">
      <button class="settings-back-btn" onclick={() => settingsOpen = false}>
        ‹ Volver
      </button>
      <span class="settings-page-title">⊕ Ajustes</span>
    </div>
    <div class="settings-page-body">
      <SettingsTab />
    </div>
  </div>
{/if}

<PlanningModal />
<SearchModal
  open={searchOpen}
  onClose={() => { searchOpen = false; }}
  onSwitchTab={(tab) => { activeTab = tab; }}
  onAction={handleSearchAction}
/>
<ShortcutsOverlay open={shortcutsOpen} onClose={() => { shortcutsOpen = false; }} />
<ConfirmModal
  open={showKillAllConfirm}
  title={$tr('page.killTitle')}
  message={$tr('page.killMsg')}
  confirmText={$tr('page.killConfirm')}
  cancelText={$tr('common.cancel')}
  variant="danger"
  onConfirm={confirmKillAll}
  onCancel={() => { showKillAllConfirm = false; }}
/>

<style>
  .app-body {
    display: flex;
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }
  .content-area {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }
  .main {
    display: flex;
    flex: 1;
    min-height: 0;
    position: relative;
  }

  /* --- Translucent chat overlay --- */
  .chat-overlay {
    position: absolute;
    bottom: 0;
    left: 0;
    right: 0;
    display: flex;
    flex-direction: column;
    z-index: 5;
    pointer-events: none;
  }
  .chat-resize-handle {
    height: 14px;
    cursor: ns-resize;
    pointer-events: auto;
    position: relative;
    flex-shrink: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(2, 8, 16, 0.5);
    border-top: 1px solid rgba(255, 255, 255, 0.06);
  }
  .chat-resize-handle::after {
    content: '';
    width: 48px;
    height: 3px;
    border-radius: 2px;
    background: rgba(255,255,255,0.10);
    transition: background 0.15s, width 0.15s;
  }
  .chat-resize-handle:hover::after,
  .chat-resize-handle.dragging::after {
    background: rgba(0, 212, 255, 0.5);
    width: 64px;
  }
  .chat-mode-bar {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 3px 12px;
    flex-shrink: 0;
    pointer-events: auto;
    background: rgba(2, 8, 16, 0.7);
    backdrop-filter: blur(12px);
    border-top: 1px solid rgba(255, 255, 255, 0.06);
  }
  .chat-columns {
    display: flex;
    flex: 1;
    min-height: 0;
    pointer-events: auto;
    background: rgba(2, 8, 16, 0.7);
    backdrop-filter: blur(12px);
  }
  .chat-col {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }
  .chat-col-sep {
    width: 1px;
    background: rgba(255, 255, 255, 0.06);
    flex-shrink: 0;
  }
  .chat-col-header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 14px;
    flex-shrink: 0;
    border-bottom: 1px solid rgba(255, 255, 255, 0.04);
  }
  .chat-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: #3a4a5a;
    flex-shrink: 0;
  }
  .chat-dot.active {
    background: var(--green);
    box-shadow: 0 0 6px var(--green);
    animation: pulse-glow 2s infinite;
  }
  .chat-name {
    font-family: var(--font-display);
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 2px;
  }
  .atlas-name { color: #7eb8ff; }
  .pixel-name { color: #7effa0; }
  .chat-tag {
    font-size: 8px;
    color: #4a5a6a;
    background: rgba(255,255,255,0.04);
    padding: 1px 6px;
    border-radius: 3px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }
  .chat-col-actions {
    margin-left: auto;
    display: flex;
    gap: 4px;
    align-items: center;
  }
  .repo-picker {
    position: relative;
    display: flex;
    align-items: center;
  }
  .repo-current {
    display: flex;
    align-items: center;
    gap: 4px;
    font-family: var(--font-display);
    font-size: 8px;
    font-weight: 500;
    color: #4a6a7a;
    cursor: pointer;
    padding: 1px 6px;
    border-radius: 3px;
    border: 1px solid transparent;
    transition: color 0.15s, border-color 0.15s, background 0.15s;
    white-space: nowrap;
    letter-spacing: 0.3px;
  }
  .repo-current:hover, .repo-picker.open .repo-current {
    color: var(--cyan);
    border-color: rgba(0, 212, 255, 0.2);
    background: rgba(0, 212, 255, 0.05);
  }
  .repo-chevron { font-size: 6px; opacity: 0.6; }
  .repo-dropdown {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    min-width: 180px;
    background: #0a1420;
    border: 1px solid rgba(0, 212, 255, 0.15);
    border-radius: 4px;
    box-shadow: 0 8px 24px rgba(0,0,0,0.5);
    z-index: 100;
    overflow: hidden;
  }
  .repo-option {
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 6px 10px;
    cursor: pointer;
    border-bottom: 1px solid rgba(255,255,255,0.04);
    transition: background 0.1s;
  }
  .repo-option:last-child { border-bottom: none; }
  .repo-option:hover { background: rgba(0, 212, 255, 0.06); }
  .repo-opt-name {
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 600;
    color: var(--cyan);
    letter-spacing: 0.3px;
  }
  .dir-browser { min-width: 240px; max-height: 240px; overflow-y: auto; }
  .dir-browser::-webkit-scrollbar { width: 2px; }
  .dir-browser::-webkit-scrollbar-thumb { background: rgba(255,255,255,0.1); }
  .dir-nav {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 8px;
    border-bottom: 1px solid rgba(255,255,255,0.06);
    background: rgba(0,212,255,0.04);
  }
  .dir-back {
    font-size: 9px;
    color: var(--cyan);
    cursor: pointer;
    flex-shrink: 0;
    opacity: 0.7;
  }
  .dir-back:hover { opacity: 1; }
  .dir-current-path {
    font-family: monospace;
    font-size: 7px;
    color: #3a5a6a;
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  .dir-select-here {
    font-size: 9px;
    color: var(--green, #4ade80);
    cursor: pointer;
    flex-shrink: 0;
    font-weight: 600;
  }
  .dir-select-here:hover { opacity: 0.8; }
  .dir-entry { flex-direction: row; align-items: center; gap: 6px; }
  .dir-folder-icon { font-size: 7px; color: #4a6a7a; flex-shrink: 0; }
  .dir-loading { padding: 8px 10px; color: #3a5a6a; font-size: 9px; }
  .chat-action-btn {
    font-family: var(--font-display);
    font-size: 8px;
    font-weight: 600;
    letter-spacing: 0.5px;
    text-transform: uppercase;
    color: var(--cyan);
    background: rgba(0, 212, 255, 0.06);
    border: 1px solid rgba(0, 212, 255, 0.15);
    border-radius: 3px;
    padding: 2px 8px;
    cursor: pointer;
    transition: background 0.15s, border-color 0.15s;
  }
  .chat-action-btn:hover {
    background: rgba(0, 212, 255, 0.12);
    border-color: rgba(0, 212, 255, 0.3);
  }
  .chat-action-btn.run {
    color: var(--green, #4ade80);
    background: rgba(74, 222, 128, 0.06);
    border-color: rgba(74, 222, 128, 0.2);
  }
  .chat-action-btn.run:hover {
    background: rgba(74, 222, 128, 0.12);
    border-color: rgba(74, 222, 128, 0.35);
  }
  .chat-action-btn.stop {
    color: var(--red, #f43f5e);
    background: rgba(244, 63, 94, 0.06);
    border-color: rgba(244, 63, 94, 0.2);
  }
  .chat-action-btn.stop:hover {
    background: rgba(244, 63, 94, 0.12);
    border-color: rgba(244, 63, 94, 0.35);
  }
  .chat-action-btn.clear {
    color: #a0a0b0;
    background: rgba(160, 160, 176, 0.05);
    border-color: rgba(160, 160, 176, 0.15);
  }
  .chat-action-btn.clear:hover {
    background: rgba(160, 160, 176, 0.1);
    border-color: rgba(160, 160, 176, 0.3);
  }
  .chat-feed {
    flex: 1;
    overflow-y: auto;
    padding: 4px 0;
    min-height: 0;
  }
  .chat-feed::-webkit-scrollbar { width: 2px; }
  .chat-feed::-webkit-scrollbar-thumb { background: rgba(255,255,255,0.1); border-radius: 1px; }
  .chat-feed::-webkit-scrollbar-track { background: transparent; }

  /* --- Tech mode items --- */
  .cf-item {
    padding: 2px 14px;
    font-size: 10px;
    line-height: 1.5;
    border-left: 2px solid transparent;
    display: flex;
    align-items: baseline;
    gap: 6px;
  }
  .cf-item:hover { background: rgba(255,255,255,0.02); }
  .cf-tool { border-left-color: rgba(255,255,255,0.06); }
  .cf-prompt {
    border-left-color: var(--cyan);
    background: rgba(0, 212, 255, 0.04);
    padding: 4px 14px;
    margin: 2px 8px;
    border-radius: 0 6px 6px 0;
  }
  .cf-label {
    font-family: var(--font-display);
    font-size: 7px;
    font-weight: 700;
    letter-spacing: 1px;
    color: var(--cyan);
    opacity: 0.6;
    flex-shrink: 0;
  }
  .cf-text {
    color: #c8d8e8;
    word-break: break-word;
  }
  .cf-badge {
    display: inline-block;
    font-family: var(--font-display);
    font-size: 8px;
    font-weight: 700;
    padding: 0px 4px;
    border-radius: 2px;
    text-transform: uppercase;
    letter-spacing: 0.3px;
    background: rgba(255,255,255,0.05);
    color: #6a8a9a;
    border: 1px solid rgba(255,255,255,0.06);
    flex-shrink: 0;
  }
  .cf-detail {
    color: #5a7a8a;
    font-size: 10px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }
  .cf-thought span {
    color: #5a7a8a;
    font-style: italic;
    font-size: 10px;
    word-break: break-word;
  }
  .cf-thinking {
    padding: 4px 14px;
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .dots {
    display: inline-flex;
    gap: 3px;
  }
  .dots span {
    width: 4px;
    height: 4px;
    border-radius: 50%;
    background: #3a5a6a;
    animation: dot-bounce 1.4s infinite ease-in-out both;
  }
  .dots span:nth-child(1) { animation-delay: 0s; }
  .dots span:nth-child(2) { animation-delay: 0.16s; }
  .dots span:nth-child(3) { animation-delay: 0.32s; }
  @keyframes dot-bounce {
    0%, 80%, 100% { transform: scale(0.6); opacity: 0.3; }
    40% { transform: scale(1); opacity: 0.8; }
  }

  /* --- Human mode items --- */
  .cf-human {
    padding: 4px 20px;
    border-left: none;
    font-size: 13px;
    line-height: 1.8;
    color: #c8d8e8;
    word-break: break-word;
    white-space: pre-wrap;
    display: block;
  }

  /* --- Tab bar --- */
  .tab-bar {
    display: flex;
    align-items: center;
    gap: 0;
    padding: 0 4px;
    background: var(--bg-1);
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
    overflow-x: auto;
    height: 28px;
  }
  .tab-bar::-webkit-scrollbar { height: 0; }
  .tab-btn {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 0 8px;
    height: 28px;
    background: none;
    border: none;
    border-bottom: 2px solid transparent;
    color: var(--text-3);
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 600;
    letter-spacing: 0.5px;
    cursor: pointer;
    white-space: nowrap;
    transition: color 0.15s, border-color 0.15s, background 0.15s;
  }
  .tab-btn:hover {
    color: var(--text-1);
    background: rgba(255,255,255,0.02);
  }
  .tab-btn.active {
    color: var(--cyan);
    border-bottom-color: var(--cyan);
  }
  .tab-icon {
    font-size: 10px;
    opacity: 0.7;
  }
  .tab-badge {
    font-size: 8px;
    background: var(--cyan);
    color: var(--bg-0);
    padding: 0 4px;
    border-radius: 8px;
    font-weight: 700;
    min-width: 14px;
    text-align: center;
  }

  .bottom-section {
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    border-top: 1px solid var(--border);
  }
  .tab-panels {
    flex: 1;
    min-height: 0;
    overflow: hidden;
    display: flex;
  }

  /* ── Settings full-page overlay ─────────────────────────── */
  .settings-page {
    position: fixed;
    inset: 0;
    z-index: 200;
    background: var(--bg-0);
    display: flex;
    flex-direction: column;
  }
  .settings-page-header {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 0 20px;
    height: 44px;
    border-bottom: 1px solid var(--border);
    background: var(--bg-1);
    flex-shrink: 0;
  }
  .settings-back-btn {
    font-family: var(--font-display);
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.5px;
    color: var(--cyan);
    background: rgba(0, 212, 255, 0.06);
    border: 1px solid rgba(0, 212, 255, 0.2);
    border-radius: 4px;
    padding: 4px 12px;
    cursor: pointer;
    transition: background 0.15s, border-color 0.15s;
  }
  .settings-back-btn:hover {
    background: rgba(0, 212, 255, 0.12);
    border-color: rgba(0, 212, 255, 0.4);
  }
  .settings-page-title {
    font-family: var(--font-display);
    font-size: 12px;
    font-weight: 700;
    letter-spacing: 1px;
    color: var(--text-1);
    text-transform: uppercase;
  }
  .settings-page-body {
    flex: 1;
    overflow-y: auto;
    min-height: 0;
  }

  /* ── Responsive: narrow window (< 900px) ───────────────── */
  @media (max-width: 900px) {
    /* Chat columns stack vertically */
    .chat-columns {
      flex-direction: column;
    }
    .chat-col-sep {
      width: 100%;
      height: 1px;
    }
    /* Tab bar scrolls horizontally, labels hidden to save space */
    .tab-label {
      display: none;
    }
    .tab-btn {
      padding: 0 6px;
    }
  }

  /* ── Responsive: very narrow (< 600px) ─────────────────── */
  @media (max-width: 600px) {
    .bottom-section {
      height: 180px !important;
    }
    .chat-overlay {
      height: 30% !important;
    }
  }
</style>
