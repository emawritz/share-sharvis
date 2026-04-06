<script lang="ts">
  import { onMount } from 'svelte';
  import { get } from 'svelte/store';
  import { getSessions, getTeamMemories, getMemoryCategories, searchTeamMemories, pinMemory, deleteTeamMemory } from '../../api';
  import { atlasFeed, pixelFeed } from '../../stores/session';
  import type { SessionInfo, TeamMemory, Activity } from '../../types';

  // ── Sessions ──────────────────────────────────────────────────────────────
  let sessions = $state<SessionInfo[]>([]);
  let loading = $state(true);
  let error = $state('');
  let search = $state('');
  let confirmDelete = $state<string | null>(null);

  async function load() {
    loading = true;
    error = '';
    try {
      sessions = await getSessions();
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    load();
    const interval = setInterval(load, 5000);
    return () => clearInterval(interval);
  });

  // --- Date formatting ---
  function formatDate(created_at: number): string {
    const d = new Date(created_at * 1000);
    const now = new Date();
    const isToday =
      d.getFullYear() === now.getFullYear() &&
      d.getMonth() === now.getMonth() &&
      d.getDate() === now.getDate();
    const yesterday = new Date(now);
    yesterday.setDate(now.getDate() - 1);
    const isYesterday =
      d.getFullYear() === yesterday.getFullYear() &&
      d.getMonth() === yesterday.getMonth() &&
      d.getDate() === yesterday.getDate();

    const time = d.toLocaleTimeString('es-ES', { hour: '2-digit', minute: '2-digit' });
    if (isToday) return `Hoy ${time}`;
    if (isYesterday) return `Ayer ${time}`;
    return d.toLocaleDateString('es-ES', { month: 'short', day: 'numeric' }) + ' ' + time;
  }

  // --- Filtered list ---
  const filtered = $derived(
    search.trim() === ''
      ? sessions
      : sessions.filter((s) => {
          const q = search.toLowerCase();
          return (
            s.name.toLowerCase().includes(q) ||
            s.project.toLowerCase().includes(q) ||
            (s.machine ?? '').toLowerCase().includes(q)
          );
        })
  );

  // --- Stats ---
  const stats = $derived(() => {
    const total = sessions.length;
    const dayStart = new Date();
    dayStart.setHours(0, 0, 0, 0);
    const dayStartSecs = dayStart.getTime() / 1000;
    const today = sessions.filter((s) => s.created_at >= dayStartSecs).length;
    const avgMsgs =
      total > 0
        ? Math.round(sessions.reduce((acc, s) => acc + s.message_count, 0) / total)
        : 0;
    return { total, today, avgMsgs };
  });

  // --- Export single session ---
  function exportSession(session: SessionInfo) {
    const data = {
      name: session.name,
      project: session.project,
      machine: session.machine,
      message_count: session.message_count,
      task_count: session.task_count,
      active_task_id: session.active_task_id,
      created_at: new Date(session.created_at * 1000).toISOString(),
      exported_at: new Date().toISOString(),
    };
    downloadJson(data, `session-${session.name}-${Date.now()}.json`);
  }

  // --- Export current view (all filtered sessions) ---
  function exportCurrentView() {
    const data = {
      exported_at: new Date().toISOString(),
      filter: search || null,
      sessions: filtered.map((s) => ({
        name: s.name,
        project: s.project,
        machine: s.machine,
        message_count: s.message_count,
        task_count: s.task_count,
        active_task_id: s.active_task_id,
        created_at: new Date(s.created_at * 1000).toISOString(),
      })),
    };
    downloadJson(data, `sessions-export-${Date.now()}.json`);
  }

  function downloadJson(data: unknown, filename: string) {
    const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = filename;
    a.click();
    URL.revokeObjectURL(url);
  }

  // --- Delete (client-side removal; bridge has no delete endpoint) ---
  function requestDelete(name: string) {
    confirmDelete = name;
  }

  function cancelDelete() {
    confirmDelete = null;
  }

  function confirmDeleteSession() {
    if (confirmDelete !== null) {
      sessions = sessions.filter((s) => s.name !== confirmDelete);
      confirmDelete = null;
    }
  }

  // ── Team Memories ─────────────────────────────────────────────────────────
  let memoriesOpen = $state(true);
  let memories = $state<TeamMemory[]>([]);
  let memoriesLoading = $state(false);
  let memoriesError = $state('');
  let memCategories = $state<string[]>([]);
  let memCategoryFilter = $state<string | null>(null);
  let memSearch = $state('');
  let memSearchDebounce = $state<ReturnType<typeof setTimeout> | null>(null);

  async function loadMemories() {
    memoriesLoading = true;
    memoriesError = '';
    try {
      const [mems, cats] = await Promise.all([getTeamMemories(), getMemoryCategories()]);
      memories = mems;
      memCategories = cats;
    } catch (e) {
      memoriesError = String(e);
    } finally {
      memoriesLoading = false;
    }
  }

  onMount(() => {
    loadMemories();
  });

  // Derived: filtered memories (search + category)
  const filteredMemories = $derived(
    memories
      .filter((m) => {
        if (memCategoryFilter && m.category !== memCategoryFilter) return false;
        if (memSearch.trim()) {
          const q = memSearch.toLowerCase();
          return m.content.toLowerCase().includes(q) || (m.tags ?? []).some((t) => t.toLowerCase().includes(q));
        }
        return true;
      })
      .sort((a, b) => {
        // Pinned first
        if (a.pin && !b.pin) return -1;
        if (!a.pin && b.pin) return 1;
        return 0;
      })
  );

  function onMemSearchInput() {
    if (memSearchDebounce) clearTimeout(memSearchDebounce);
    memSearchDebounce = setTimeout(async () => {
      if (memSearch.trim()) {
        memoriesLoading = true;
        try {
          memories = await searchTeamMemories(memSearch.trim(), memCategoryFilter ?? undefined);
        } catch {
          // fall back to client-side filter
        } finally {
          memoriesLoading = false;
        }
      } else {
        await loadMemories();
      }
    }, 350);
  }

  async function togglePin(mem: TeamMemory) {
    const next = !mem.pin;
    try {
      await pinMemory(mem.id, next);
      memories = memories.map((m) => (m.id === mem.id ? { ...m, pin: next } : m));
    } catch (e) {
      memoriesError = String(e);
    }
  }

  async function handleDeleteMemory(id: number) {
    try {
      await deleteTeamMemory(id);
      memories = memories.filter((m) => m.id !== id);
    } catch (e) {
      memoriesError = String(e);
    }
  }

  // ── Recent Prompts ────────────────────────────────────────────────────────
  let promptsOpen = $state(true);

  // Derive last 10 prompt activities from both feeds (live stores)
  let atlasActivities = $state<Activity[]>([]);
  let pixelActivities = $state<Activity[]>([]);

  onMount(() => {
    // Subscribe to the Svelte stores from the session module
    const unsubAtlas = atlasFeed.subscribe((v) => { atlasActivities = v; });
    const unsubPixel = pixelFeed.subscribe((v) => { pixelActivities = v; });
    return () => { unsubAtlas(); unsubPixel(); };
  });

  const recentPrompts = $derived(
    [...atlasActivities, ...pixelActivities]
      .filter((a) => a.type === 'prompt' && a.content)
      .slice(-30)
      .reverse()
      .slice(0, 10)
  );
</script>

<div class="sessions-tab">
  <!-- Header -->
  <div class="sessions-header">
    <h2>Sesiones Claude</h2>
    <div class="header-actions">
      {#if sessions.length > 0}
        <button class="export-view-btn" onclick={exportCurrentView} title="Exportar vista actual">
          Exportar vista
        </button>
      {/if}
      <button class="refresh-btn" onclick={load} disabled={loading}>
        {loading ? '...' : '↻'}
      </button>
    </div>
  </div>

  <!-- Search -->
  <div class="search-row">
    <div class="search-wrap">
      <span class="search-icon">⌕</span>
      <input
        class="search-input"
        type="text"
        placeholder="Buscar por nombre, proyecto o máquina…"
        bind:value={search}
      />
      {#if search}
        <button class="clear-btn" onclick={() => (search = '')} aria-label="Limpiar">✕</button>
      {/if}
    </div>
  </div>

  <!-- Stats row -->
  {#if sessions.length > 0}
    <div class="stats-row">
      <div class="stat-chip">
        <span class="stat-val">{stats().total}</span>
        <span class="stat-label">total</span>
      </div>
      <div class="stat-chip">
        <span class="stat-val">{stats().today}</span>
        <span class="stat-label">hoy</span>
      </div>
      <div class="stat-chip">
        <span class="stat-val">{stats().avgMsgs}</span>
        <span class="stat-label">msgs promedio</span>
      </div>
    </div>
  {/if}

  {#if error}
    <div class="error">{error}</div>
  {:else if loading && sessions.length === 0}
    <div class="empty">Cargando sesiones...</div>
  {:else if sessions.length === 0}
    <div class="empty-state">
      <div class="empty-icon">🤖</div>
      <div class="empty-title">Sin sesiones</div>
      <div class="empty-sub">Empieza una conversación con JARVIS para ver sesiones aquí.</div>
    </div>
  {:else if filtered.length === 0}
    <div class="empty-state">
      <div class="empty-icon">🔍</div>
      <div class="empty-title">Sin resultados</div>
      <div class="empty-sub">Ninguna sesión coincide con "{search}".</div>
    </div>
  {:else}
    <div class="sessions-list">
      {#each filtered as session (session.name)}
        <div class="session-card">
          <div class="session-icon">🤖</div>
          <div class="session-info">
            <div class="session-top">
              <span class="session-name">{session.name}</span>
              <span class="msg-badge">{session.message_count} msgs</span>
            </div>
            <div class="session-sub">
              {session.project}{session.machine ? ` · ${session.machine.toUpperCase()}` : ''}
            </div>
            <div class="session-meta">
              {#if session.active_task_id !== null}
                <span class="meta-item active">Tarea #{session.active_task_id} activa</span>
              {:else}
                <span class="meta-item idle">Inactiva</span>
              {/if}
              {#if session.task_count > 0}
                <span class="meta-item">{session.task_count} tareas</span>
              {/if}
              <span class="meta-item date">{formatDate(session.created_at)}</span>
            </div>
          </div>
          <div class="session-actions">
            <button
              class="action-btn export-btn"
              onclick={() => exportSession(session)}
              title="Exportar sesión"
            >
              Exportar
            </button>
            {#if confirmDelete === session.name}
              <button class="action-btn confirm-del-btn" onclick={confirmDeleteSession}>
                ¿Confirmar?
              </button>
              <button class="action-btn cancel-btn" onclick={cancelDelete}>Cancelar</button>
            {:else}
              <button
                class="action-btn del-btn"
                onclick={() => requestDelete(session.name)}
                title="Eliminar sesión"
              >
                Eliminar
              </button>
            {/if}
          </div>
          <div class="session-status" class:running={session.active_task_id !== null}></div>
        </div>
      {/each}
    </div>
  {/if}

  <!-- ── Team Memories ────────────────────────────────────────────────── -->
  <div class="section-card">
    <button
      class="section-toggle"
      onclick={() => (memoriesOpen = !memoriesOpen)}
      aria-expanded={memoriesOpen}
    >
      <span class="section-title">Team Memories</span>
      <span class="section-count">{memories.length}</span>
      <span class="toggle-arrow">{memoriesOpen ? '▾' : '▸'}</span>
    </button>

    {#if memoriesOpen}
      <div class="section-body">
        <!-- Category filter chips -->
        {#if memCategories.length > 0}
          <div class="chip-row">
            <button
              class="chip"
              class:chip-active={memCategoryFilter === null}
              onclick={() => { memCategoryFilter = null; loadMemories(); }}
            >
              Todas
            </button>
            {#each memCategories as cat (cat)}
              <button
                class="chip"
                class:chip-active={memCategoryFilter === cat}
                onclick={() => { memCategoryFilter = cat; loadMemories(); }}
              >
                {cat}
              </button>
            {/each}
          </div>
        {/if}

        <!-- Search -->
        <div class="search-wrap mem-search">
          <span class="search-icon">⌕</span>
          <input
            class="search-input"
            type="text"
            placeholder="Buscar memorias…"
            bind:value={memSearch}
            oninput={onMemSearchInput}
          />
          {#if memSearch}
            <button class="clear-btn" onclick={() => { memSearch = ''; loadMemories(); }} aria-label="Limpiar">✕</button>
          {/if}
        </div>

        {#if memoriesError}
          <div class="error">{memoriesError}</div>
        {:else if memoriesLoading}
          <div class="empty">Cargando memorias...</div>
        {:else if filteredMemories.length === 0}
          <div class="empty">Sin memorias{memCategoryFilter ? ` en "${memCategoryFilter}"` : ''}.</div>
        {:else}
          <div class="memories-list">
            {#each filteredMemories as mem (mem.id)}
              <div class="memory-card" class:memory-pinned={mem.pin}>
                <div class="memory-body">
                  <div class="memory-content">{mem.content}</div>
                  <div class="memory-meta">
                    {#if mem.category}
                      <span class="mem-cat-badge">{mem.category}</span>
                    {/if}
                    {#each (mem.tags ?? []) as tag (tag)}
                      <span class="mem-tag">#{tag}</span>
                    {/each}
                    <span class="mem-from">{mem.from}</span>
                  </div>
                </div>
                <div class="memory-actions">
                  <button
                    class="mem-btn pin-btn"
                    class:pin-active={mem.pin}
                    onclick={() => togglePin(mem)}
                    title={mem.pin ? 'Desfijar' : 'Fijar'}
                  >
                    📌
                  </button>
                  <button
                    class="mem-btn mem-del-btn"
                    onclick={() => handleDeleteMemory(mem.id)}
                    title="Eliminar memoria"
                  >
                    ✕
                  </button>
                </div>
              </div>
            {/each}
          </div>
        {/if}
      </div>
    {/if}
  </div>

  <!-- ── Recent Prompts ──────────────────────────────────────────────── -->
  <div class="section-card">
    <button
      class="section-toggle"
      onclick={() => (promptsOpen = !promptsOpen)}
      aria-expanded={promptsOpen}
    >
      <span class="section-title">Prompts Recientes</span>
      <span class="section-count">{recentPrompts.length}</span>
      <span class="toggle-arrow">{promptsOpen ? '▾' : '▸'}</span>
    </button>

    {#if promptsOpen}
      <div class="section-body">
        {#if recentPrompts.length === 0}
          <div class="empty">Sin prompts recientes. Los prompts aparecen cuando los agentes están activos.</div>
        {:else}
          <div class="prompts-list">
            {#each recentPrompts as prompt, i (i)}
              <div class="prompt-card">
                <div class="prompt-text">{prompt.content}</div>
              </div>
            {/each}
          </div>
        {/if}
      </div>
    {/if}
  </div>
</div>

<style>
  .sessions-tab {
    padding: 16px;
    height: 100%;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .sessions-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .header-actions {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  h2 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
    color: var(--text-primary, #e2e8f0);
  }

  .refresh-btn {
    background: none;
    border: 1px solid var(--border, #374151);
    color: var(--text-secondary, #9ca3af);
    border-radius: 4px;
    padding: 2px 8px;
    cursor: pointer;
    font-size: 14px;
  }

  .refresh-btn:hover:not(:disabled) {
    background: var(--hover, #1f2937);
  }

  .export-view-btn {
    font-size: 11px;
    border-radius: 4px;
    padding: 3px 8px;
    cursor: pointer;
    background: rgba(59, 130, 246, 0.1);
    border: 1px solid rgba(59, 130, 246, 0.3);
    color: #60a5fa;
    white-space: nowrap;
  }

  .export-view-btn:hover {
    background: rgba(59, 130, 246, 0.2);
  }

  /* Search */
  .search-row {
    display: flex;
  }

  .search-wrap {
    position: relative;
    flex: 1;
    display: flex;
    align-items: center;
  }

  .search-icon {
    position: absolute;
    left: 8px;
    color: var(--text-secondary, #9ca3af);
    font-size: 15px;
    pointer-events: none;
    line-height: 1;
  }

  .search-input {
    width: 100%;
    background: var(--card-bg, #111827);
    border: 1px solid var(--border, #374151);
    border-radius: 6px;
    color: var(--text-primary, #e2e8f0);
    font-size: 12px;
    padding: 6px 28px 6px 28px;
    outline: none;
    box-sizing: border-box;
  }

  .search-input:focus {
    border-color: #3b82f6;
  }

  .search-input::placeholder {
    color: var(--text-secondary, #6b7280);
  }

  .clear-btn {
    position: absolute;
    right: 6px;
    background: none;
    border: none;
    color: var(--text-secondary, #9ca3af);
    cursor: pointer;
    font-size: 11px;
    padding: 2px 4px;
    line-height: 1;
  }

  .clear-btn:hover {
    color: var(--text-primary, #e2e8f0);
  }

  /* Stats row */
  .stats-row {
    display: flex;
    gap: 8px;
  }

  .stat-chip {
    display: flex;
    align-items: baseline;
    gap: 4px;
    background: var(--card-bg, #111827);
    border: 1px solid var(--border, #374151);
    border-radius: 6px;
    padding: 4px 10px;
  }

  .stat-val {
    font-size: 14px;
    font-weight: 700;
    color: var(--text-primary, #e2e8f0);
  }

  .stat-label {
    font-size: 10px;
    color: var(--text-secondary, #9ca3af);
  }

  /* Empty states */
  .empty {
    color: var(--text-secondary, #9ca3af);
    font-size: 13px;
    text-align: center;
    padding: 32px 16px;
  }

  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding: 40px 16px;
    color: var(--text-secondary, #9ca3af);
    text-align: center;
  }

  .empty-icon {
    font-size: 28px;
    margin-bottom: 4px;
  }

  .empty-title {
    font-size: 14px;
    font-weight: 600;
    color: var(--text-primary, #e2e8f0);
  }

  .empty-sub {
    font-size: 12px;
    color: var(--text-secondary, #9ca3af);
    max-width: 240px;
  }

  .error {
    color: #f87171;
    font-size: 12px;
    padding: 8px;
    background: rgba(239, 68, 68, 0.1);
    border-radius: 4px;
  }

  /* Session list */
  .sessions-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .session-card {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px;
    background: var(--card-bg, #111827);
    border: 1px solid var(--border, #374151);
    border-radius: 8px;
  }

  .session-icon {
    font-size: 20px;
    flex-shrink: 0;
  }

  .session-info {
    flex: 1;
    min-width: 0;
  }

  .session-top {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 2px;
  }

  .session-name {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary, #e2e8f0);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .msg-badge {
    flex-shrink: 0;
    font-size: 10px;
    font-weight: 600;
    color: #3b82f6;
    background: rgba(59, 130, 246, 0.15);
    border-radius: 10px;
    padding: 1px 7px;
  }

  .session-sub {
    font-size: 11px;
    color: var(--text-secondary, #9ca3af);
    margin-bottom: 4px;
  }

  .session-meta {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }

  .meta-item {
    font-size: 11px;
    color: var(--text-secondary, #9ca3af);
    background: var(--tag-bg, #1f2937);
    padding: 2px 6px;
    border-radius: 3px;
  }

  .meta-item.active {
    color: #34d399;
    background: rgba(52, 211, 153, 0.1);
  }

  .meta-item.idle {
    color: #6b7280;
  }

  .meta-item.date {
    color: #60a5fa;
    background: rgba(96, 165, 250, 0.08);
  }

  /* Action buttons */
  .session-actions {
    display: flex;
    flex-direction: column;
    gap: 4px;
    flex-shrink: 0;
  }

  .action-btn {
    font-size: 11px;
    border-radius: 4px;
    padding: 3px 8px;
    cursor: pointer;
    border: 1px solid transparent;
    white-space: nowrap;
  }

  .export-btn {
    background: rgba(59, 130, 246, 0.1);
    border-color: rgba(59, 130, 246, 0.3);
    color: #60a5fa;
  }

  .export-btn:hover {
    background: rgba(59, 130, 246, 0.2);
  }

  .del-btn {
    background: rgba(239, 68, 68, 0.08);
    border-color: rgba(239, 68, 68, 0.25);
    color: #f87171;
  }

  .del-btn:hover {
    background: rgba(239, 68, 68, 0.18);
  }

  .confirm-del-btn {
    background: rgba(239, 68, 68, 0.2);
    border-color: #ef4444;
    color: #fca5a5;
    font-weight: 600;
  }

  .confirm-del-btn:hover {
    background: rgba(239, 68, 68, 0.35);
  }

  .cancel-btn {
    background: var(--tag-bg, #1f2937);
    border-color: var(--border, #374151);
    color: var(--text-secondary, #9ca3af);
  }

  .cancel-btn:hover {
    background: var(--hover, #374151);
  }

  /* Status dot */
  .session-status {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: #6b7280;
    flex-shrink: 0;
  }

  .session-status.running {
    background: #34d399;
    box-shadow: 0 0 6px rgba(52, 211, 153, 0.5);
  }

  /* ── Collapsible section cards ─────────────────────────────────────── */
  .section-card {
    background: var(--card-bg, #111827);
    border: 1px solid var(--border, #374151);
    border-radius: 8px;
    overflow: hidden;
  }

  .section-toggle {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 12px;
    background: none;
    border: none;
    cursor: pointer;
    text-align: left;
    color: var(--text-primary, #e2e8f0);
  }

  .section-toggle:hover {
    background: var(--hover, #1f2937);
  }

  .section-title {
    font-size: 13px;
    font-weight: 600;
    flex: 1;
  }

  .section-count {
    font-size: 11px;
    color: var(--text-secondary, #9ca3af);
    background: var(--tag-bg, #1f2937);
    border-radius: 10px;
    padding: 1px 6px;
  }

  .toggle-arrow {
    font-size: 11px;
    color: var(--text-secondary, #9ca3af);
  }

  .section-body {
    padding: 0 12px 12px 12px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  /* Category chips */
  .chip-row {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .chip {
    font-size: 11px;
    padding: 3px 10px;
    border-radius: 12px;
    border: 1px solid var(--border, #374151);
    background: var(--tag-bg, #1f2937);
    color: var(--text-secondary, #9ca3af);
    cursor: pointer;
  }

  .chip:hover {
    background: var(--hover, #374151);
    color: var(--text-primary, #e2e8f0);
  }

  .chip-active {
    background: rgba(59, 130, 246, 0.2);
    border-color: #3b82f6;
    color: #60a5fa;
  }

  .mem-search {
    margin-top: 2px;
  }

  /* Memory list */
  .memories-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .memory-card {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 10px;
    border: 1px solid var(--border, #374151);
    border-radius: 6px;
    background: var(--tag-bg, #1f2937);
    transition: border-color 0.15s, background 0.15s;
  }

  .memory-card.memory-pinned {
    border-color: #b45309;
    background: rgba(180, 83, 9, 0.08);
  }

  .memory-body {
    flex: 1;
    min-width: 0;
  }

  .memory-content {
    font-size: 12px;
    color: var(--text-primary, #e2e8f0);
    line-height: 1.5;
    white-space: pre-wrap;
    word-break: break-word;
  }

  .memory-meta {
    display: flex;
    gap: 5px;
    flex-wrap: wrap;
    margin-top: 6px;
  }

  .mem-cat-badge {
    font-size: 10px;
    background: rgba(139, 92, 246, 0.15);
    color: #a78bfa;
    border-radius: 3px;
    padding: 1px 5px;
  }

  .mem-tag {
    font-size: 10px;
    color: var(--text-secondary, #9ca3af);
  }

  .mem-from {
    font-size: 10px;
    color: #4b5563;
    margin-left: auto;
  }

  .memory-actions {
    display: flex;
    flex-direction: column;
    gap: 4px;
    flex-shrink: 0;
  }

  .mem-btn {
    background: none;
    border: none;
    cursor: pointer;
    font-size: 13px;
    padding: 2px 4px;
    border-radius: 3px;
    opacity: 0.5;
    line-height: 1;
  }

  .mem-btn:hover {
    opacity: 1;
    background: var(--hover, #374151);
  }

  .pin-btn.pin-active {
    opacity: 1;
    filter: sepia(1) saturate(3) hue-rotate(5deg);
  }

  .mem-del-btn:hover {
    color: #f87171;
  }

  /* Recent Prompts */
  .prompts-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .prompt-card {
    padding: 8px 10px;
    border: 1px solid var(--border, #374151);
    border-radius: 6px;
    background: var(--tag-bg, #1f2937);
  }

  .prompt-text {
    font-size: 12px;
    color: var(--text-primary, #e2e8f0);
    line-height: 1.5;
    white-space: pre-wrap;
    word-break: break-word;
    /* clamp to 4 lines */
    display: -webkit-box;
    -webkit-line-clamp: 4;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
</style>
