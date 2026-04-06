<script lang="ts">
  import { session } from '../../stores/session';
  import { t, tr } from '$lib/i18n';

  let commitsBack = $derived($session.commitsBack || []);
  let commitsFront = $derived($session.commitsFront || []);

  // Resize handle state: leftFlex is the fraction [0.15, 0.85] for the left panel
  let leftFlex = $state(0.5);
  let dragging = $state(false);
  let containerEl: HTMLElement | undefined = $state(undefined);

  // Expanded commit hashes (for details toggle)
  let expandedHashes = $state<Set<string>>(new Set());

  // Copied hash feedback
  let copiedHash = $state<string | null>(null);

  function onPointerDown(e: PointerEvent) {
    dragging = true;
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
    e.preventDefault();
  }

  function onPointerMove(e: PointerEvent) {
    if (!dragging || !containerEl) return;
    const rect = containerEl.getBoundingClientRect();
    const handleWidth = 5;
    const usable = rect.width - handleWidth;
    let ratio = (e.clientX - rect.left - handleWidth / 2) / usable;
    ratio = Math.max(0.15, Math.min(0.85, ratio));
    leftFlex = ratio;
  }

  function onPointerUp(_e: PointerEvent) {
    dragging = false;
  }

  function parseCommit(raw: string) {
    const i = raw.indexOf(' ');
    if (i === -1) return { hash: '', message: raw };
    return { hash: raw.substring(0, i), message: raw.substring(i + 1) };
  }

  function shortHash(hash: string): string {
    return hash.substring(0, 7);
  }

  async function copyHash(fullHash: string) {
    try {
      await navigator.clipboard.writeText(fullHash);
      copiedHash = fullHash;
      setTimeout(() => { copiedHash = null; }, 1500);
    } catch {}
  }

  function toggleExpand(hash: string) {
    const next = new Set(expandedHashes);
    if (next.has(hash)) {
      next.delete(hash);
    } else {
      next.add(hash);
    }
    expandedHashes = next;
  }

  function authorInitial(message: string): string {
    // Derive a pseudo-initial from the first meaningful word of the commit message
    const word = message.trim().replace(/^(feat|fix|chore|refactor|docs|style|test|perf|ci|build|revert)[:(]/i, '').trim();
    return (word[0] || '?').toUpperCase();
  }

  function avatarColor(letter: string): string {
    const palette = [
      '#1a6b6b', '#6b1a4a', '#2a4a6b', '#4a6b1a',
      '#6b4a1a', '#1a2a6b', '#6b1a1a', '#1a6b3a',
    ];
    return palette[(letter.charCodeAt(0) || 0) % palette.length];
  }

  function exportCommits() {
    const all = [
      ...commitsBack.map(r => { const c = parseCommit(r); return `${c.hash} backend ${c.message}`; }),
      ...commitsFront.map(r => { const c = parseCommit(r); return `${c.hash} frontend ${c.message}`; }),
    ];
    const text = all.join('\n');
    const date = new Date().toISOString().slice(0, 10);
    const blob = new Blob([text], { type: 'text/plain' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `jarvis-commits-${date}.txt`;
    a.click();
    URL.revokeObjectURL(url);
  }
</script>

<div class="commits-container" role="tabpanel">
  <div class="tab-header">
    <span class="tab-title">Commits</span>
    <button class="export-btn" onclick={exportCommits} title="Exportar todos los commits como .txt">
      <svg width="11" height="11" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.5">
        <path d="M6 1v7M3 5l3 3 3-3M1 10h10" stroke-linecap="round" stroke-linejoin="round"/>
      </svg>
      Exportar
    </button>
  </div>

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="bottom-bar" bind:this={containerEl} class:resizing={dragging}>
    <div class="commits" role="region" aria-label={$tr('commits.backend')} style="flex: {leftFlex}">
      <div class="section-title">
        {$tr('commits.backend')}
        {#if commitsBack.length > 0}
          <span class="count-badge">{commitsBack.length}</span>
        {/if}
      </div>
      {#if commitsBack.length === 0}
        <div class="empty-state">
          <span class="empty-icon">&#x2295;</span>
          <div class="empty-title">{$tr('commits.none')}</div>
          <div class="empty-hint">Run a task to generate commits</div>
        </div>
      {:else}
        {#each commitsBack as raw}
          {@const c = parseCommit(raw)}
          {@const initial = authorInitial(c.message)}
          {@const isExpanded = expandedHashes.has(c.hash)}
          <div class="commit-row" class:expanded={isExpanded}>
            <div class="commit-main" onclick={() => toggleExpand(c.hash)} role="button" tabindex="0"
              onkeydown={(e) => e.key === 'Enter' && toggleExpand(c.hash)}>
              <span class="author-avatar" style="background: {avatarColor(initial)}">{initial}</span>
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <!-- svelte-ignore a11y_click_events_have_key_events -->
              <span
                class="commit-hash"
                class:copied={copiedHash === c.hash}
                onclick={(e) => { e.stopPropagation(); copyHash(c.hash); }}
                title="Clic para copiar hash completo"
              >
                {shortHash(c.hash)}
              </span>
              <span class="commit-msg">{c.message}</span>
              <span class="expand-caret" class:open={isExpanded}>&#x25B8;</span>
            </div>
            {#if isExpanded}
              <div class="commit-detail">
                <div class="detail-row">
                  <span class="detail-label">Hash completo</span>
                  <span class="detail-value mono">{c.hash}</span>
                </div>
                <div class="detail-row">
                  <span class="detail-label">Mensaje</span>
                  <span class="detail-value">{c.message}</span>
                </div>
                <button class="copy-detail-btn" onclick={() => copyHash(c.hash)}>
                  {copiedHash === c.hash ? 'Copiado!' : 'Copiar hash'}
                </button>
              </div>
            {/if}
          </div>
        {/each}
      {/if}
    </div>
    <div
      class="resize-h-inner"
      onpointerdown={onPointerDown}
      onpointermove={onPointerMove}
      onpointerup={onPointerUp}
      role="separator"
      aria-orientation="vertical"
      tabindex="-1"
    ></div>
    <div class="commits" role="region" aria-label={$tr('commits.frontend')} style="flex: {1 - leftFlex}">
      <div class="section-title">
        {$tr('commits.frontend')}
        {#if commitsFront.length > 0}
          <span class="count-badge">{commitsFront.length}</span>
        {/if}
      </div>
      {#if commitsFront.length === 0}
        <div class="empty-state">
          <span class="empty-icon">&#x2295;</span>
          <div class="empty-title">{$tr('commits.none')}</div>
          <div class="empty-hint">Run a task to generate commits</div>
        </div>
      {:else}
        {#each commitsFront as raw}
          {@const c = parseCommit(raw)}
          {@const initial = authorInitial(c.message)}
          {@const isExpanded = expandedHashes.has(c.hash + '-front')}
          <div class="commit-row" class:expanded={isExpanded}>
            <div class="commit-main" onclick={() => toggleExpand(c.hash + '-front')} role="button" tabindex="0"
              onkeydown={(e) => e.key === 'Enter' && toggleExpand(c.hash + '-front')}>
              <span class="author-avatar" style="background: {avatarColor(initial)}">{initial}</span>
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <!-- svelte-ignore a11y_click_events_have_key_events -->
              <span
                class="commit-hash"
                class:copied={copiedHash === c.hash}
                onclick={(e) => { e.stopPropagation(); copyHash(c.hash); }}
                title="Clic para copiar hash completo"
              >
                {shortHash(c.hash)}
              </span>
              <span class="commit-msg">{c.message}</span>
              <span class="expand-caret" class:open={isExpanded}>&#x25B8;</span>
            </div>
            {#if isExpanded}
              <div class="commit-detail">
                <div class="detail-row">
                  <span class="detail-label">Hash completo</span>
                  <span class="detail-value mono">{c.hash}</span>
                </div>
                <div class="detail-row">
                  <span class="detail-label">Mensaje</span>
                  <span class="detail-value">{c.message}</span>
                </div>
                <button class="copy-detail-btn" onclick={() => copyHash(c.hash)}>
                  {copiedHash === c.hash ? 'Copiado!' : 'Copiar hash'}
                </button>
              </div>
            {/if}
          </div>
        {/each}
      {/if}
    </div>
  </div>
</div>

<style>
  .commits-container { display: flex; flex-direction: column; flex: 1; min-height: 0; overflow: hidden; }

  .tab-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 14px 4px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }
  .tab-title {
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 700;
    color: var(--text-3);
    text-transform: uppercase;
    letter-spacing: 2px;
  }
  .export-btn {
    display: flex;
    align-items: center;
    gap: 4px;
    background: transparent;
    border: 1px solid var(--border-bright);
    color: var(--text-2);
    font-size: 10px;
    padding: 3px 8px;
    border-radius: 4px;
    cursor: pointer;
    transition: color 0.15s, border-color 0.15s;
  }
  .export-btn:hover { color: var(--cyan); border-color: var(--cyan); }

  .bottom-bar { display: flex; flex: 1; min-height: 0; overflow: hidden; }
  .commits {
    background: var(--bg-1);
    padding: 8px 14px;
    overflow-y: auto;
    min-width: 0;
  }
  .resizing { user-select: none; }
  .commits::-webkit-scrollbar { width: 3px; }
  .commits::-webkit-scrollbar-thumb { background: var(--border-bright); border-radius: 2px; }
  .resize-h-inner {
    width: 5px;
    background: var(--border);
    cursor: col-resize;
    flex-shrink: 0;
    transition: background 0.15s ease;
  }
  .resize-h-inner:hover, .resizing .resize-h-inner { background: var(--cyan); }
  .section-title {
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 700;
    color: var(--text-2);
    text-transform: uppercase;
    letter-spacing: 2px;
    margin-bottom: 6px;
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .section-title::before {
    content: '';
    width: 3px;
    height: 3px;
    border-radius: 50%;
    background: var(--text-2);
  }
  .count-badge {
    font-size: 9px;
    background: var(--border-bright);
    color: var(--text-2);
    padding: 1px 5px;
    border-radius: 8px;
    font-weight: 700;
    letter-spacing: 0;
  }

  .commit-row {
    border-radius: 5px;
    margin-bottom: 2px;
    transition: background 0.1s;
  }
  .commit-row:hover { background: var(--bg-2); }
  .commit-row.expanded { background: var(--bg-2); }

  .commit-main {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 4px;
    cursor: pointer;
    font-size: 11px;
    font-variant-numeric: tabular-nums;
    min-width: 0;
  }

  .author-avatar {
    flex-shrink: 0;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 8px;
    font-weight: 700;
    color: rgba(255,255,255,0.85);
    font-family: var(--font-display);
  }

  .commit-hash {
    font-family: var(--font-mono, monospace);
    font-size: 10px;
    font-weight: 700;
    color: var(--cyan);
    background: rgba(0, 200, 200, 0.08);
    border: 1px solid rgba(0, 200, 200, 0.18);
    border-radius: 3px;
    padding: 1px 5px;
    cursor: pointer;
    flex-shrink: 0;
    transition: background 0.12s, color 0.12s;
    user-select: none;
  }
  .commit-hash:hover { background: rgba(0, 200, 200, 0.18); }
  .commit-hash.copied { color: var(--green, #4ade80); background: rgba(74, 222, 128, 0.12); border-color: rgba(74, 222, 128, 0.3); }

  .commit-msg {
    color: var(--text-1);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
    min-width: 0;
  }

  .expand-caret {
    flex-shrink: 0;
    color: var(--text-3);
    font-size: 9px;
    transition: transform 0.15s;
    display: inline-block;
  }
  .expand-caret.open { transform: rotate(90deg); }

  .commit-detail {
    padding: 6px 8px 8px 30px;
    border-top: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .detail-row {
    display: flex;
    gap: 8px;
    font-size: 10px;
    align-items: flex-start;
  }
  .detail-label {
    color: var(--text-3);
    flex-shrink: 0;
    min-width: 80px;
    text-transform: uppercase;
    font-size: 9px;
    letter-spacing: 0.5px;
    padding-top: 1px;
  }
  .detail-value {
    color: var(--text-1);
    word-break: break-all;
  }
  .detail-value.mono {
    font-family: var(--font-mono, monospace);
    font-size: 10px;
    color: var(--cyan);
  }
  .copy-detail-btn {
    align-self: flex-start;
    margin-top: 4px;
    background: transparent;
    border: 1px solid var(--border-bright);
    color: var(--text-2);
    font-size: 10px;
    padding: 2px 8px;
    border-radius: 3px;
    cursor: pointer;
    transition: color 0.12s, border-color 0.12s;
  }
  .copy-detail-btn:hover { color: var(--cyan); border-color: var(--cyan); }

  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 40px 20px;
    color: var(--text-3);
    text-align: center;
  }
  .empty-icon { font-size: 24px; opacity: 0.4; }
  .empty-title { font-size: 12px; font-weight: 600; color: var(--text-2); }
  .empty-hint { font-size: 10px; color: var(--text-3); }
</style>
