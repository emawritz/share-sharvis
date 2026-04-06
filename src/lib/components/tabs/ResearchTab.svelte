<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { sendTask, fetchTasks, onTaskDone, saveKnowledge, getKnowledge, searchKnowledge, starKnowledge, deleteKnowledge } from '$lib/api';
  import { addToast } from '$lib/stores/notifications';
  import type { Task, KnowledgeEntry } from '$lib/types';

  // ── Types ──────────────────────────────────────────────────────────────
  interface ResearchCard {
    id: string;
    title: string;
    status: 'analyzing' | 'done' | 'error';
    summary: string;
    sourceUrl: string;
    timestamp: number;
    fullOutput: string;
    taskId: number | null;
    knowledgeId: number | null;  // non-null if saved to DB
    starred: boolean;
  }

  // ── State ──────────────────────────────────────────────────────────────
  let inputValue = $state('');
  let searchQuery = $state('');
  let sending = $state(false);
  let cards = $state<ResearchCard[]>(loadCards());
  let expandedId = $state<string | null>(null);
  let pollTimer: ReturnType<typeof setInterval> | undefined;
  let unTaskDone: (() => void) | undefined;
  let searchTimer: ReturnType<typeof setTimeout> | undefined;

  // ── localStorage persistence ───────────────────────────────────────────
  const STORAGE_KEY = 'jarvis-research-cards';

  function loadCards(): ResearchCard[] {
    if (typeof localStorage === 'undefined') return [];
    try {
      const raw = localStorage.getItem(STORAGE_KEY);
      const parsed: ResearchCard[] = raw ? JSON.parse(raw) : [];
      // Ensure new fields exist on legacy cards
      return parsed.map(c => ({
        ...c,
        knowledgeId: c.knowledgeId ?? null,
        starred: c.starred ?? false,
      }));
    } catch {
      return [];
    }
  }

  function saveCards() {
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(cards));
    }
  }

  $effect(() => {
    // Save whenever cards change
    void cards.length;
    saveCards();
  });

  // ── Source type detection ─────────────────────────────────────────────
  function detectSourceType(url: string): string {
    if (!url) return 'research';
    if (/github\.com/i.test(url)) return 'github';
    if (/twitter\.com|x\.com/i.test(url)) return 'twitter';
    if (/^https?:\/\//i.test(url)) return 'web';
    return 'research';
  }

  // ── Knowledge DB integration ──────────────────────────────────────────
  async function saveCardToKnowledge(card: ResearchCard): Promise<number | null> {
    try {
      const sourceType = detectSourceType(card.sourceUrl);
      const tags = [sourceType];
      if (card.sourceUrl) tags.push('url');
      const id = await saveKnowledge(
        card.title,
        card.fullOutput || card.summary,
        card.sourceUrl || undefined,
        sourceType,
        tags,
      );
      return id;
    } catch (e) {
      console.error('Failed to save knowledge:', e);
      return null;
    }
  }

  async function loadKnowledgeEntries() {
    try {
      const entries = await getKnowledge(200);
      // Build a set of knowledgeIds already linked in cards
      const linkedIds = new Set(cards.filter(c => c.knowledgeId !== null).map(c => c.knowledgeId));
      // Add DB entries that don't have a corresponding card
      for (const entry of entries) {
        if (linkedIds.has(entry.id)) continue;
        // Check if there's already a card matching by title+timestamp (fuzzy match)
        const exists = cards.some(c => c.title === entry.title && c.knowledgeId === entry.id);
        if (exists) continue;
        cards = [...cards, {
          id: `kb-${entry.id}`,
          title: entry.title,
          status: 'done',
          summary: extractSummary(entry.content),
          sourceUrl: entry.sourceUrl || '',
          timestamp: new Date(entry.createdAt).getTime(),
          fullOutput: entry.content,
          taskId: null,
          knowledgeId: entry.id,
          starred: entry.starred,
        }];
      }
    } catch (e) {
      console.error('Failed to load knowledge:', e);
    }
  }

  async function handleSearch() {
    if (!searchQuery.trim()) {
      // Reload all
      cards = loadCards();
      await loadKnowledgeEntries();
      return;
    }
    try {
      const results = await searchKnowledge(searchQuery.trim(), 100);
      // Show only matching DB entries, plus any local analyzing cards
      const analyzingCards = cards.filter(c => c.status === 'analyzing');
      const dbCards: ResearchCard[] = results.map(entry => ({
        id: `kb-${entry.id}`,
        title: entry.title,
        status: 'done' as const,
        summary: extractSummary(entry.content),
        sourceUrl: entry.sourceUrl || '',
        timestamp: new Date(entry.createdAt).getTime(),
        fullOutput: entry.content,
        taskId: null,
        knowledgeId: entry.id,
        starred: entry.starred,
      }));
      cards = [...analyzingCards, ...dbCards];
    } catch (e) {
      console.error('Search failed:', e);
    }
  }

  function onSearchInput() {
    if (searchTimer) clearTimeout(searchTimer);
    searchTimer = setTimeout(() => handleSearch(), 300);
  }

  async function toggleStar(card: ResearchCard) {
    if (card.knowledgeId === null) return;
    try {
      await starKnowledge(card.knowledgeId);
      cards = cards.map(c =>
        c.id === card.id ? { ...c, starred: !c.starred } : c
      );
    } catch (e) {
      addToast('Error al marcar favorito', 'error');
    }
  }

  // ── Prompt building ────────────────────────────────────────────────────
  function isUrl(text: string): boolean {
    return /^https?:\/\//i.test(text.trim());
  }

  function buildPrompt(input: string): string {
    const trimmed = input.trim();
    if (!isUrl(trimmed)) {
      return `Investiga: ${trimmed}. Proporciona una respuesta detallada con fuentes si es posible.`;
    }
    const url = trimmed;
    if (/github\.com/i.test(url)) {
      return `Analiza este repositorio: ${url}. Evalua: tech stack, calidad del codigo, si es util, si se puede ejecutar localmente. Resume en español.`;
    }
    if (/twitter\.com|x\.com/i.test(url)) {
      return `Analiza este post/thread: ${url}. Resume el contenido y evalua si es relevante.`;
    }
    return `Investiga esta URL: ${url}. Resume el contenido principal.`;
  }

  function generateTitle(input: string): string {
    const trimmed = input.trim();
    if (isUrl(trimmed)) {
      try {
        const u = new URL(trimmed);
        const path = u.pathname.replace(/^\//, '').replace(/\/$/, '');
        if (/github\.com/i.test(trimmed)) return `GitHub: ${path}`;
        if (/twitter\.com|x\.com/i.test(trimmed)) return `X: ${path.substring(0, 60)}`;
        return `${u.hostname}/${path}`.substring(0, 80);
      } catch {
        return trimmed.substring(0, 80);
      }
    }
    return trimmed.length > 80 ? trimmed.substring(0, 77) + '...' : trimmed;
  }

  // ── Send research ──────────────────────────────────────────────────────
  async function investigate() {
    if (!inputValue.trim() || sending) return;
    sending = true;
    const input = inputValue.trim();
    const prompt = buildPrompt(input);
    const card: ResearchCard = {
      id: Date.now().toString(36) + Math.random().toString(36).substring(2, 6),
      title: generateTitle(input),
      status: 'analyzing',
      summary: '',
      sourceUrl: isUrl(input) ? input : '',
      timestamp: Date.now(),
      fullOutput: '',
      taskId: null,
      knowledgeId: null,
      starred: false,
    };

    cards = [card, ...cards];
    inputValue = '';

    try {
      const task = await sendTask('atlas', prompt);
      cards = cards.map(c => c.id === card.id ? { ...c, taskId: task.id } : c);
      addToast('Investigacion iniciada', 'success');
      startPolling();
    } catch (e) {
      const msg = typeof e === 'string' ? e : String(e);
      cards = cards.map(c => c.id === card.id ? { ...c, status: 'error' as const, summary: msg } : c);
      addToast('Error: ' + msg, 'error');
    } finally {
      sending = false;
    }
  }

  // ── Polling for task completion ────────────────────────────────────────
  function startPolling() {
    if (pollTimer) return;
    pollTimer = setInterval(pollTasks, 4000);
  }

  function stopPolling() {
    if (pollTimer) {
      clearInterval(pollTimer);
      pollTimer = undefined;
    }
  }

  async function onCardCompleted(card: ResearchCard) {
    if (card.knowledgeId !== null) return; // already saved
    const kId = await saveCardToKnowledge(card);
    if (kId !== null) {
      cards = cards.map(c => c.id === card.id ? { ...c, knowledgeId: kId } : c);
    }
  }

  async function pollTasks() {
    const pendingCards = cards.filter(c => c.status === 'analyzing' && c.taskId !== null);
    if (pendingCards.length === 0) {
      stopPolling();
      return;
    }

    try {
      const tasks = await fetchTasks();
      const taskMap = new Map<number, Task>();
      for (const t of tasks) {
        taskMap.set(t.id, t);
      }

      let changed = false;
      const completedCards: ResearchCard[] = [];
      cards = cards.map(c => {
        if (c.status !== 'analyzing' || c.taskId === null) return c;
        const task = taskMap.get(c.taskId);
        if (!task) return c;
        if (task.status === 'done' || task.status === 'completed') {
          changed = true;
          const output = task.output || '';
          const updated = {
            ...c,
            status: 'done' as const,
            summary: extractSummary(output),
            fullOutput: output,
          };
          completedCards.push(updated);
          return updated;
        }
        if (task.status === 'error' || task.status === 'failed') {
          changed = true;
          return {
            ...c,
            status: 'error' as const,
            summary: task.output || 'Task failed',
            fullOutput: task.output || '',
          };
        }
        return c;
      });

      if (changed) {
        saveCards();
        for (const cc of completedCards) {
          onCardCompleted(cc);
        }
      }
    } catch {
      // Ignore polling errors
    }
  }

  function extractSummary(output: string): string {
    if (!output) return 'Sin resultado';
    const slice = output.substring(0, 500);
    const lastDot = slice.lastIndexOf('.');
    if (lastDot > 100) return slice.substring(0, lastDot + 1);
    return slice + (output.length > 500 ? '...' : '');
  }

  // ── Task done event listener ───────────────────────────────────────────
  onMount(async () => {
    onTaskDone((data) => {
      const completedCards: ResearchCard[] = [];
      cards = cards.map(c => {
        if (c.taskId !== data.id || c.status !== 'analyzing') return c;
        const output = data.output || '';
        const hasError = output.toLowerCase().startsWith('error:') || output.toLowerCase().startsWith('fatal:');
        const updated = {
          ...c,
          status: hasError ? 'error' as const : 'done' as const,
          summary: extractSummary(output),
          fullOutput: output,
        };
        if (!hasError) completedCards.push(updated);
        return updated;
      });
      saveCards();
      for (const cc of completedCards) {
        onCardCompleted(cc);
      }
    }).then(fn => { unTaskDone = fn; });

    // If there are pending cards, start polling
    if (cards.some(c => c.status === 'analyzing' && c.taskId !== null)) {
      startPolling();
    }

    // Load knowledge entries from DB
    await loadKnowledgeEntries();
  });

  onDestroy(() => {
    unTaskDone?.();
    stopPolling();
    if (searchTimer) clearTimeout(searchTimer);
  });

  // ── Actions ────────────────────────────────────────────────────────────
  function toggleExpand(id: string) {
    expandedId = expandedId === id ? null : id;
  }

  async function removeCard(card: ResearchCard) {
    if (card.knowledgeId !== null) {
      try {
        await deleteKnowledge(card.knowledgeId);
      } catch (e) {
        console.error('Failed to delete from DB:', e);
      }
    }
    cards = cards.filter(c => c.id !== card.id);
  }

  function clearAll() {
    cards = [];
    expandedId = null;
  }

  function formatDate(ts: number): string {
    const d = new Date(ts);
    return d.toLocaleDateString('es', { day: '2-digit', month: 'short' }) + ' ' +
           d.toLocaleTimeString('es', { hour: '2-digit', minute: '2-digit' });
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      investigate();
    }
  }

  let pendingCount = $derived(cards.filter(c => c.status === 'analyzing').length);
  let sortedCards = $derived(
    [...cards].sort((a, b) => {
      // Starred first, then by timestamp desc
      if (a.starred !== b.starred) return a.starred ? -1 : 1;
      return b.timestamp - a.timestamp;
    })
  );
</script>

<div class="research-tab">
  <!-- Input area -->
  <div class="input-area">
    <div class="input-row">
      <input
        type="text"
        class="research-input"
        placeholder="URL o pregunta para investigar..."
        bind:value={inputValue}
        onkeydown={handleKeydown}
        disabled={sending}
      />
      <button
        class="btn-investigate"
        onclick={investigate}
        disabled={!inputValue.trim() || sending}
      >
        {sending ? 'Enviando...' : 'Investigar'}
      </button>
    </div>
    <div class="input-hints">
      <span class="hint">GitHub, Twitter/X, URLs, o preguntas libres</span>
      {#if cards.length > 0}
        <button class="btn-clear" onclick={clearAll}>Limpiar historial</button>
      {/if}
    </div>
  </div>

  <!-- Search bar -->
  <div class="search-bar">
    <input
      type="text"
      class="search-input"
      placeholder="Buscar en knowledge base..."
      bind:value={searchQuery}
      oninput={onSearchInput}
    />
    {#if searchQuery}
      <button class="search-clear" onclick={() => { searchQuery = ''; handleSearch(); }}>&times;</button>
    {/if}
  </div>

  <!-- Status bar -->
  {#if pendingCount > 0}
    <div class="status-bar">
      <span class="pulse"></span>
      {pendingCount} investigacion{pendingCount > 1 ? 'es' : ''} en curso...
    </div>
  {/if}

  <!-- Cards -->
  <div class="cards-container">
    {#if sortedCards.length === 0}
      <div class="empty-state">
        <div class="empty-icon">&#x1F50D;</div>
        <div class="empty-text">{searchQuery ? 'Sin resultados' : 'Sin investigaciones aun'}</div>
        <div class="empty-hint">{searchQuery ? 'Intenta con otros terminos' : 'Pega una URL o escribe una pregunta arriba'}</div>
      </div>
    {:else}
      {#each sortedCards as card (card.id)}
        <div class="research-card" class:analyzing={card.status === 'analyzing'} class:error={card.status === 'error'}>
          <div class="card-header">
            <div class="card-status">
              {#if card.status === 'analyzing'}
                <span class="status-dot analyzing-dot"></span>
              {:else if card.status === 'done'}
                <span class="status-dot done-dot"></span>
              {:else}
                <span class="status-dot error-dot"></span>
              {/if}
            </div>
            <div class="card-title" onclick={() => toggleExpand(card.id)} role="button" tabindex="0" onkeydown={(e) => { if (e.key === 'Enter') toggleExpand(card.id); }}>
              {card.title}
            </div>
            <div class="card-badges">
              {#if card.knowledgeId !== null}
                <span class="badge badge-saved">Guardado</span>
              {/if}
            </div>
            <div class="card-meta">
              <span class="card-time">{formatDate(card.timestamp)}</span>
              <button
                class="btn-star"
                class:starred={card.starred}
                onclick={() => toggleStar(card)}
                title={card.starred ? 'Quitar favorito' : 'Marcar favorito'}
                disabled={card.knowledgeId === null}
              >&#9733;</button>
              <button class="btn-remove" onclick={() => removeCard(card)} title="Eliminar">&times;</button>
            </div>
          </div>

          {#if card.sourceUrl}
            <div class="card-url">
              <a href={card.sourceUrl} target="_blank" rel="noopener noreferrer">{card.sourceUrl}</a>
            </div>
          {/if}

          <div class="card-summary">
            {#if card.status === 'analyzing'}
              <span class="analyzing-text">Analizando...</span>
            {:else}
              {card.summary}
            {/if}
          </div>

          {#if expandedId === card.id && card.fullOutput}
            <div class="card-full-output">
              <pre>{card.fullOutput}</pre>
            </div>
          {/if}

          {#if card.fullOutput}
            <button class="btn-expand" onclick={() => toggleExpand(card.id)}>
              {expandedId === card.id ? 'Colapsar' : 'Ver completo'}
            </button>
          {/if}
        </div>
      {/each}
    {/if}
  </div>
</div>

<style>
  .research-tab {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
    padding: 12px;
    gap: 10px;
  }

  /* Input area */
  .input-area {
    flex-shrink: 0;
  }
  .input-row {
    display: flex;
    gap: 8px;
  }
  .research-input {
    flex: 1;
    background: var(--bg-0);
    border: 1px solid var(--border);
    border-radius: 6px;
    color: var(--text-0);
    font-family: var(--font-mono);
    font-size: 12px;
    padding: 8px 12px;
    outline: none;
    transition: border-color 0.15s;
  }
  .research-input:focus {
    border-color: var(--cyan);
  }
  .research-input::placeholder {
    color: var(--text-2);
  }
  .btn-investigate {
    background: var(--cyan-dim);
    color: var(--cyan);
    border: 1px solid var(--cyan);
    border-radius: 6px;
    padding: 8px 16px;
    font-family: var(--font-mono);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    white-space: nowrap;
    transition: background 0.15s, opacity 0.15s;
  }
  .btn-investigate:hover:not(:disabled) {
    background: var(--cyan);
    color: var(--bg-0);
  }
  .btn-investigate:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  .input-hints {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-top: 4px;
    padding: 0 2px;
  }
  .hint {
    font-size: 10px;
    color: var(--text-2);
  }
  .btn-clear {
    background: none;
    border: none;
    color: var(--text-2);
    font-size: 10px;
    cursor: pointer;
    text-decoration: underline;
    font-family: var(--font-mono);
  }
  .btn-clear:hover {
    color: var(--red, #f44);
  }

  /* Search bar */
  .search-bar {
    flex-shrink: 0;
    position: relative;
  }
  .search-input {
    width: 100%;
    background: var(--bg-0);
    border: 1px solid var(--border);
    border-radius: 6px;
    color: var(--text-0);
    font-family: var(--font-mono);
    font-size: 11px;
    padding: 6px 28px 6px 10px;
    outline: none;
    transition: border-color 0.15s;
    box-sizing: border-box;
  }
  .search-input:focus {
    border-color: var(--cyan);
  }
  .search-input::placeholder {
    color: var(--text-2);
  }
  .search-clear {
    position: absolute;
    right: 6px;
    top: 50%;
    transform: translateY(-50%);
    background: none;
    border: none;
    color: var(--text-2);
    font-size: 14px;
    cursor: pointer;
    padding: 0 2px;
    line-height: 1;
  }
  .search-clear:hover {
    color: var(--text-0);
  }

  /* Status bar */
  .status-bar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    background: var(--cyan-dim);
    border-radius: 6px;
    font-size: 11px;
    color: var(--cyan);
    font-family: var(--font-mono);
    flex-shrink: 0;
  }
  .pulse {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--cyan);
    animation: pulse-anim 1.5s infinite;
  }
  @keyframes pulse-anim {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.3; }
  }

  /* Cards */
  .cards-container {
    flex: 1;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 48px 16px;
    color: var(--text-2);
  }
  .empty-icon {
    font-size: 32px;
    opacity: 0.5;
  }
  .empty-text {
    font-family: var(--font-display);
    font-size: 14px;
  }
  .empty-hint {
    font-size: 11px;
    font-family: var(--font-mono);
  }

  .research-card {
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 10px 12px;
    transition: border-color 0.15s;
  }
  .research-card.analyzing {
    border-color: var(--cyan);
  }
  .research-card.error {
    border-color: var(--red, #f44);
  }

  .card-header {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .card-status {
    flex-shrink: 0;
  }
  .status-dot {
    display: inline-block;
    width: 8px;
    height: 8px;
    border-radius: 50%;
  }
  .analyzing-dot {
    background: var(--cyan);
    animation: pulse-anim 1.5s infinite;
  }
  .done-dot {
    background: var(--green, #4c4);
  }
  .error-dot {
    background: var(--red, #f44);
  }

  .card-title {
    flex: 1;
    font-family: var(--font-display);
    font-size: 12px;
    font-weight: 600;
    color: var(--text-0);
    cursor: pointer;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .card-title:hover {
    color: var(--cyan);
  }

  .card-badges {
    display: flex;
    gap: 4px;
    flex-shrink: 0;
  }
  .badge {
    font-size: 8px;
    font-family: var(--font-mono);
    padding: 1px 5px;
    border-radius: 3px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }
  .badge-saved {
    background: rgba(76, 204, 76, 0.15);
    color: var(--green, #4c4);
    border: 1px solid rgba(76, 204, 76, 0.3);
  }

  .card-meta {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
  }
  .card-time {
    font-size: 9px;
    color: var(--text-2);
    font-family: var(--font-mono);
    white-space: nowrap;
  }
  .btn-star {
    background: none;
    border: none;
    color: var(--text-2);
    font-size: 14px;
    cursor: pointer;
    padding: 0 2px;
    line-height: 1;
    transition: color 0.15s;
  }
  .btn-star:disabled {
    opacity: 0.3;
    cursor: default;
  }
  .btn-star.starred {
    color: var(--yellow, #fa0);
  }
  .btn-star:hover:not(:disabled) {
    color: var(--yellow, #fa0);
  }
  .btn-remove {
    background: none;
    border: none;
    color: var(--text-2);
    font-size: 14px;
    cursor: pointer;
    padding: 0 2px;
    line-height: 1;
  }
  .btn-remove:hover {
    color: var(--red, #f44);
  }

  .card-url {
    margin-top: 4px;
    font-size: 10px;
    font-family: var(--font-mono);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .card-url a {
    color: var(--cyan);
    text-decoration: none;
  }
  .card-url a:hover {
    text-decoration: underline;
  }

  .card-summary {
    margin-top: 6px;
    font-size: 11px;
    color: var(--text-1);
    font-family: var(--font-mono);
    line-height: 1.5;
    white-space: pre-wrap;
    word-break: break-word;
  }
  .analyzing-text {
    color: var(--cyan);
    font-style: italic;
  }

  .card-full-output {
    margin-top: 8px;
    background: var(--bg-0);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 8px;
    max-height: 400px;
    overflow-y: auto;
  }
  .card-full-output pre {
    margin: 0;
    font-size: 10px;
    font-family: var(--font-mono);
    color: var(--text-1);
    white-space: pre-wrap;
    word-break: break-word;
  }

  .btn-expand {
    margin-top: 6px;
    background: none;
    border: none;
    color: var(--cyan);
    font-size: 10px;
    font-family: var(--font-mono);
    cursor: pointer;
    padding: 0;
    text-decoration: underline;
  }
  .btn-expand:hover {
    color: var(--text-0);
  }
</style>
