<script lang="ts">
  import { tick, onMount } from 'svelte';
  import { machineFeed, machineAgentInfo, session, clearFeed } from '../stores/session';
  import { machines } from '../stores/machines';
  import { fetchAgentDetails, getAgentMessages, markMessagesRead } from '../api';
  import type { Activity, AgentDetail, AgentMessage } from '../types';
  import { appVisible } from '../stores/visibility';
  import { tr } from '$lib/i18n';
  import { formatAge } from '$lib/utils/time';

  let { target }: { target: string } = $props();

  let autoScroll = $state(true);
  let feedEl: HTMLDivElement | undefined = $state();
  let showDetails = $state(false);
  let agentDetails = $state<AgentDetail[]>([]);
  let loadingDetails = $state(false);
  let unreadCount = $state(0);
  let showMessages = $state(false);
  let messages = $state<AgentMessage[]>([]);
  // Local cleared offset: when user clicks "Clear", we snapshot the feed length
  // and only display items after that offset (purely local, no store mutation)
  let clearedAt = $state<number>(0);

  const BADGE_MAP: Record<string, string> = {
    Bash: 'bash', Read: 'read', Edit: 'edit', Write: 'write', Grep: 'grep', Glob: 'grep',
    Agent: 'agent', ToolSearch: 'toolsearch', Skill: 'skill', WebFetch: 'web', WebSearch: 'web',
    NotebookEdit: 'edit'
  };

  // Activity type → icon (Unicode / emoji-free symbols)
  const TYPE_ICON: Record<string, string> = {
    bash: '⚡', read: '👁', edit: '✏', write: '💾', grep: '🔍',
    agent: '🤖', toolsearch: '🔎', skill: '⚙', web: '🌐', other: '◆'
  };

  let rawFeed = $derived($machineFeed[target] ?? []);
  // Apply local clear: show only items added after the last clear
  let feed = $derived(rawFeed.slice(clearedAt));

  let machineInfo = $derived($machines[target]);
  let isRunning = $derived(($session as any)[`${target}Running`] ?? false);

  let agentName = $derived(machineInfo?.name ?? target);
  let agentTag = $derived(machineInfo?.role ?? target);
  let agentInfo = $derived($machineAgentInfo[target] ?? { agentCount: 0, skills: [] });

  onMount(() => {
    fetchUnreadCount();
    const interval = setInterval(fetchUnreadCount, 10000);
    return () => clearInterval(interval);
  });

  async function fetchUnreadCount() {
    if (!$appVisible) return;
    try {
      const msgs = await getAgentMessages(target, true);
      unreadCount = msgs.length;
    } catch { /* ignore */ }
  }

  $effect(() => {
    feed; // depend on feed
    if (!feedEl || !autoScroll) return;
    tick().then(() => {
      if (feedEl && autoScroll) feedEl.scrollTop = feedEl.scrollHeight;
    });
  });

  function toggleAutoScroll() {
    autoScroll = !autoScroll;
  }

  function clearLocalFeed() {
    clearedAt = rawFeed.length;
  }

  function badgeClass(name: string | undefined): string {
    if (!name) return 'other';
    return BADGE_MAP[name] || 'other';
  }

  function activityIcon(name: string | undefined): string {
    const cls = badgeClass(name);
    return TYPE_ICON[cls] || TYPE_ICON['other'];
  }

  function detailClass(name: string | undefined): string {
    if (!name) return 'cmd-ref';
    return ['Read', 'Edit', 'Write'].includes(name) ? 'file-ref' : 'cmd-ref';
  }

  /** Format a ts (epoch ms) as "Xm ago" / "Xs ago" / "just now" */
  function tsAgo(ts: number | undefined): string {
    if (!ts) return '';
    const diff = Math.floor((Date.now() - ts) / 1000);
    if (diff < 5) return 'just now';
    if (diff < 60) return `${diff}s ago`;
    const m = Math.floor(diff / 60);
    if (m < 60) return `${m}m ago`;
    const h = Math.floor(m / 60);
    return `${h}h ago`;
  }

  async function toggleMessages() {
    if (showMessages) {
      showMessages = false;
      return;
    }
    try {
      messages = await getAgentMessages(target);
      await markMessagesRead(target);
      unreadCount = 0;
    } catch { messages = []; }
    showMessages = true;
  }

  async function toggleDetails() {
    if (showDetails) {
      showDetails = false;
      return;
    }
    loadingDetails = true;
    try {
      agentDetails = await fetchAgentDetails(target);
    } catch {
      agentDetails = [];
    }
    loadingDetails = false;
    showDetails = true;
  }

  function shortSessionId(id: string): string {
    return id.length > 12 ? id.slice(0, 8) + '...' : id;
  }
</script>

<section class="agent-panel {target}" aria-label={agentName.toUpperCase()}>
  <div class="agent-header">
    <div class="agent-dot" class:active={isRunning} role="status" aria-label="{agentName} {isRunning ? $tr('agent.active') : $tr('agent.inactive')}"></div>
    <h2 class="agent-name">{agentName}</h2>
    <span class="agent-tag">{agentTag}</span>
    {#if agentInfo.agentCount > 0}
      <button class="agent-count" onclick={toggleDetails} title={$tr('agent.viewDetails')}>
        {agentInfo.agentCount} {$tr('agent.agents')}
      </button>
    {/if}
    {#if unreadCount > 0}
      <button class="msg-badge" onclick={toggleMessages} title={$tr('agent.unreadMessages')}>
        {unreadCount} {$tr('agent.msg')}
      </button>
    {/if}
    <div class="feed-controls">
      {#if feed.length > 0}
        <button
          class="feed-clear"
          aria-label="Clear feed"
          title="Clear feed"
          onclick={clearLocalFeed}
        >✕</button>
      {/if}
      <button
        class="feed-toggle"
        class:active={autoScroll}
        aria-label={$tr('agent.autoScroll')}
        title={$tr('agent.autoScroll')}
        onclick={toggleAutoScroll}
      >&#8595;</button>
    </div>
  </div>
  {#if agentInfo.skills.length > 0}
    <div class="skills-bar">
      {#each agentInfo.skills as skill}
        <span class="skill-pill">{skill}</span>
      {/each}
    </div>
  {/if}
  {#if showDetails}
    <div class="agent-details">
      {#if loadingDetails}
        <div class="detail-loading">{$tr('common.loading')}</div>
      {:else if agentDetails.length === 0}
        <div class="detail-empty">{$tr('agent.noActiveAgents')}</div>
      {:else}
        {#each agentDetails as agent, i}
          <div class="detail-card">
            <div class="detail-header">
              <span class="detail-index">#{i + 1}</span>
              <span class="detail-sid" title={agent.sessionId}>{shortSessionId(agent.sessionId)}</span>
              <span class="detail-age">{formatAge(agent.secondsAgo)}</span>
            </div>
            {#if agent.lastTool}
              <div class="detail-activity">
                <span class="badge {badgeClass(agent.lastTool)}">{agent.lastTool}</span>
                <span class={detailClass(agent.lastTool)}>{agent.lastDetail || ''}</span>
              </div>
            {/if}
            {#if agent.lastText}
              <div class="detail-text">{agent.lastText}</div>
            {/if}
          </div>
        {/each}
      {/if}
    </div>
  {/if}
  {#if showMessages}
    <div class="messages-panel">
      {#if messages.length === 0}
        <div class="detail-empty">{$tr('agent.noRecentMessages')}</div>
      {:else}
        {#each messages.slice(-20) as msg}
          <div class="msg-item">
            <span class="msg-from">{msg.from}</span>
            <span class="msg-cat">{msg.category}</span>
            <span class="msg-content">{msg.content}</span>
          </div>
        {/each}
      {/if}
    </div>
  {/if}
  <div class="feed" bind:this={feedEl} role="log" aria-label={$tr('agent.activity', { name: agentName })} aria-live="polite">
    {#if feed.length === 0 && !isRunning}
      <div class="idle-placeholder">
        <span class="idle-dot"></span>
        <span class="idle-label">Idle...</span>
      </div>
    {/if}
    {#each feed as item}
      {#if item.type === 'prompt'}
        <div class="feed-item prompt-bubble">
          <div class="prompt-label">{$tr('agent.prompt')}</div>
          <div class="prompt-text">{item.content || ''}</div>
        </div>
      {:else}
        <div class="feed-item {item.type}" class:error={item.name === 'error'}>
          {#if item.type === 'tool'}
            <span class="activity-icon" aria-hidden="true">{activityIcon(item.name)}</span>
            <span class="badge {badgeClass(item.name)}">{item.name}</span>
            <span class={detailClass(item.name)}>{item.detail || ''}</span>
            {#if item.ts}
              <span class="ts-ago">{tsAgo(item.ts)}</span>
            {/if}
          {:else if item.name === 'error'}
            <span class="activity-icon" aria-hidden="true">⚠</span>
            <span class="badge error-badge">{$tr('agent.errorLabel')}</span>
            <span class="error-text">{item.content || ''}</span>
            {#if item.ts}
              <span class="ts-ago">{tsAgo(item.ts)}</span>
            {/if}
          {:else}
            <span class="thought">{item.content || ''}</span>
            {#if item.ts}
              <span class="ts-ago">{tsAgo(item.ts)}</span>
            {/if}
          {/if}
        </div>
      {/if}
    {/each}
    {#if isRunning}
      <div class="feed-item thinking">
        <span class="thinking-dots">
          <span></span><span></span><span></span>
        </span>
        <span class="thinking-label">{$tr('agent.thinking')}</span>
      </div>
    {/if}
  </div>
</section>

<style>
  .agent-panel {
    background: var(--bg-0);
    display: flex;
    flex-direction: column;
    overflow: hidden;
    min-width: 0;
    position: relative;
  }
  .agent-header {
    padding: 8px 16px;
    display: flex;
    align-items: center;
    gap: 10px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
    background: var(--bg-1);
  }
  .agent-dot {
    width: 8px; height: 8px;
    border-radius: 50%;
    flex-shrink: 0;
    background: var(--text-3);
    transition: background 0.3s ease, box-shadow 0.3s ease;
  }
  .agent-dot.active {
    background: var(--green);
    box-shadow: 0 0 8px var(--green);
    animation: pulse-glow 2s infinite;
  }
  .agent-name {
    font-family: var(--font-display);
    font-size: 13px;
    font-weight: 700;
    letter-spacing: 2px;
    text-transform: uppercase;
  }
  .atlas .agent-name { color: #7eb8ff; }
  .pixel .agent-name { color: #7effa0; }
  .agent-tag {
    font-size: 9px;
    color: var(--text-2);
    background: var(--bg-3);
    padding: 2px 8px;
    border-radius: 3px;
    text-transform: uppercase;
    letter-spacing: 1px;
    font-weight: 600;
  }
  .agent-count {
    font-family: var(--font-display);
    font-size: 10px;
    color: var(--cyan);
    background: #00d4ff11;
    padding: 2px 8px;
    border-radius: 3px;
    border: 1px solid #00d4ff22;
    font-weight: 600;
    letter-spacing: 0.5px;
    cursor: pointer;
    transition: background 0.15s ease, border-color 0.15s ease;
  }
  .agent-count:hover { background: #00d4ff22; border-color: #00d4ff44; }
  .agent-details {
    border-bottom: 1px solid var(--border);
    background: var(--bg-1);
    padding: 6px 10px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    flex-shrink: 0;
    max-height: 200px;
    overflow-y: auto;
  }
  .detail-loading, .detail-empty {
    font-size: 10px;
    color: var(--text-3);
    text-align: center;
    padding: 4px;
  }
  .detail-card {
    background: var(--bg-0);
    border: 1px solid var(--border);
    border-radius: 5px;
    padding: 6px 10px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .detail-header {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .detail-index {
    font-family: var(--font-display);
    font-size: 10px;
    font-weight: 700;
    color: var(--cyan);
  }
  .detail-sid {
    font-size: 9px;
    color: var(--text-3);
    font-family: var(--font-mono, monospace);
  }
  .detail-age {
    font-size: 9px;
    color: var(--text-3);
    margin-left: auto;
  }
  .detail-activity {
    display: flex;
    align-items: center;
    gap: 4px;
    overflow: hidden;
  }
  .detail-activity .file-ref,
  .detail-activity .cmd-ref {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .detail-text {
    font-size: 10px;
    color: var(--text-2);
    font-style: italic;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .skills-bar {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    padding: 4px 14px;
    border-bottom: 1px solid var(--border);
    background: var(--bg-1);
    flex-shrink: 0;
  }
  .skill-pill {
    font-size: 9px;
    color: var(--text-2);
    background: var(--bg-3);
    padding: 1px 6px;
    border-radius: 3px;
    border: 1px solid var(--border);
    letter-spacing: 0.3px;
  }
  .feed {
    flex: 1;
    overflow-y: auto;
    padding: 4px 0;
    min-height: 0;
    max-height: 100%;
  }
  .feed::-webkit-scrollbar { width: 3px; }
  .feed::-webkit-scrollbar-thumb { background: var(--border-bright); border-radius: 2px; }
  .feed::-webkit-scrollbar-track { background: transparent; }
  .feed-item {
    padding: 3px 14px;
    line-height: 1.65;
    border-left: 2px solid transparent;
    display: flex;
    align-items: baseline;
    flex-wrap: wrap;
    gap: 0 4px;
  }
  .feed-item:hover { background: #ffffff06; }
  .feed-item.tool { border-left-color: var(--border-bright); }
  .feed-item.text { padding: 4px 14px; border-left-color: #2a4060; background: #0f1620; }
  .badge {
    display: inline-block;
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 700;
    padding: 1px 6px;
    border-radius: 3px;
    margin-right: 6px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    vertical-align: middle;
  }
  .badge.bash { background: #33221a; color: #ffb74d; border: 1px solid #ff980022; }
  .badge.read { background: #0a1a33; color: #64b5f6; border: 1px solid #2196f322; }
  .badge.edit { background: #0a2a1a; color: #66bb6a; border: 1px solid #4caf5022; }
  .badge.write { background: #0a2a1a; color: #81c784; border: 1px solid #4caf5022; }
  .badge.grep { background: #1a0a2a; color: #ba68c8; border: 1px solid #9c27b022; }
  .badge.agent { background: #2a0a0a; color: var(--red); border: 1px solid #f4433622; }
  .badge.toolsearch { background: #1a1a0a; color: #ffd54f; border: 1px solid #ffc10722; }
  .badge.skill { background: #0a1a2a; color: #4fc3f7; border: 1px solid #03a9f422; }
  .badge.web { background: #1a0a1a; color: #ce93d8; border: 1px solid #ab47bc22; }
  .badge.other { background: var(--bg-3); color: var(--text-2); border: 1px solid var(--border); }
  .badge.error-badge { background: #2a0a0a; color: var(--red); border: 1px solid #f4433644; }
  .file-ref { color: #5cc4b8; font-size: 11px; }
  .cmd-ref { color: var(--text-1); font-size: 11px; }
  .thought { color: #c8d9ea; font-size: 11.5px; white-space: pre-wrap; word-break: break-word; flex: 1; }
  .error-text { color: var(--red); font-size: 11px; word-break: break-word; flex: 1; }
  .feed-item.error { border-left-color: #f44336; background: #f4433608; }

  /* Activity type icon */
  .activity-icon {
    font-size: 10px;
    opacity: 0.7;
    margin-right: 2px;
    flex-shrink: 0;
  }

  /* Timestamp diff */
  .ts-ago {
    font-size: 9px;
    color: var(--text-3);
    margin-left: auto;
    white-space: nowrap;
    flex-shrink: 0;
    opacity: 0.75;
  }

  /* Idle placeholder */
  .idle-placeholder {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 32px 14px;
    color: var(--text-3);
  }
  .idle-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--text-3);
    animation: idle-pulse 2.4s ease-in-out infinite;
    flex-shrink: 0;
  }
  .idle-label {
    font-size: 11px;
    font-style: italic;
    color: var(--text-3);
    letter-spacing: 0.5px;
  }
  @keyframes idle-pulse {
    0%, 100% { opacity: 0.25; transform: scale(0.8); }
    50% { opacity: 0.85; transform: scale(1.15); }
  }

  /* Prompt bubble */
  .feed-item.prompt-bubble {
    border-left-color: var(--cyan);
    background: #00d4ff08;
    padding: 8px 14px;
    margin: 4px 8px 4px 14px;
    border-radius: 0 8px 8px 0;
    border: 1px solid #00d4ff1a;
    border-left: 3px solid var(--cyan);
    display: block;
  }
  .prompt-label {
    font-family: var(--font-display);
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 1.5px;
    color: var(--cyan);
    margin-bottom: 4px;
    opacity: 0.7;
  }
  .prompt-text {
    font-size: 12px;
    color: var(--text-0);
    white-space: pre-wrap;
    word-break: break-word;
    line-height: 1.5;
  }

  /* Thinking indicator */
  .feed-item.thinking {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 14px;
    border-left-color: transparent;
  }
  .thinking-dots {
    display: flex;
    gap: 3px;
    align-items: center;
  }
  .thinking-dots span {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--text-3);
    animation: thinking-bounce 1.4s infinite ease-in-out both;
  }
  .thinking-dots span:nth-child(1) { animation-delay: 0s; }
  .thinking-dots span:nth-child(2) { animation-delay: 0.16s; }
  .thinking-dots span:nth-child(3) { animation-delay: 0.32s; }
  .thinking-label {
    font-size: 10px;
    color: var(--text-3);
    font-style: italic;
  }
  @keyframes thinking-bounce {
    0%, 80%, 100% { transform: scale(0.6); opacity: 0.4; }
    40% { transform: scale(1); opacity: 1; }
  }
  .feed-controls {
    display: flex;
    gap: 4px;
    margin-left: auto;
    align-items: center;
  }
  .feed-clear {
    background: none;
    border: 1px solid var(--border);
    color: var(--text-3);
    width: 22px; height: 22px;
    border-radius: 4px;
    cursor: pointer;
    display: grid;
    place-items: center;
    font-size: 9px;
    transition: background 0.15s ease, color 0.15s ease, border-color 0.15s ease;
  }
  .feed-clear:hover { background: #f4433612; color: var(--red); border-color: #f4433644; }
  .feed-toggle {
    background: none;
    border: 1px solid var(--border);
    color: var(--text-2);
    width: 22px; height: 22px;
    border-radius: 4px;
    cursor: pointer;
    display: grid;
    place-items: center;
    font-size: 10px;
    transition: background 0.15s ease, color 0.15s ease;
  }
  .feed-toggle:hover { background: var(--bg-3); color: var(--text-0); }
  .feed-toggle.active { background: var(--cyan-dim); color: var(--cyan); border-color: #00d4ff44; }
  .msg-badge {
    font-family: var(--font-display);
    font-size: 10px;
    color: var(--amber);
    background: #ffb74d11;
    padding: 2px 8px;
    border-radius: 3px;
    border: 1px solid #ffb74d22;
    font-weight: 600;
    cursor: pointer;
    animation: pulse-glow 2s infinite;
  }
  .msg-badge:hover { background: #ffb74d22; border-color: #ffb74d44; }
  .messages-panel {
    border-bottom: 1px solid var(--border);
    background: var(--bg-1);
    padding: 6px 10px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    flex-shrink: 0;
    max-height: 150px;
    overflow-y: auto;
  }
  .msg-item {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 10px;
    padding: 2px 0;
  }
  .msg-from {
    font-weight: 700;
    color: var(--cyan);
    font-family: var(--font-display);
    font-size: 9px;
    text-transform: uppercase;
  }
  .msg-cat {
    font-size: 8px;
    color: var(--text-3);
    background: var(--bg-3);
    padding: 1px 4px;
    border-radius: 2px;
  }
  .msg-content {
    color: var(--text-1);
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
