<script lang="ts">
  import { onMount } from 'svelte';
  import { fetchMachineCapabilities, fetchSingleMachineCapabilities, getTopTools } from '$lib/api';
  import { handleError } from '$lib/utils';
  import { addToast } from '$lib/stores/notifications';
  import type { MachineCapabilities, ToolStat } from '$lib/types';
  import { t, tr } from '$lib/i18n';
  import Skeleton from '$lib/components/Skeleton.svelte';

  // ---------------------------------------------------------------------------
  // Machine capabilities state
  // ---------------------------------------------------------------------------

  let capabilities = $state<MachineCapabilities[]>([]);
  let loadingIds = $state<Set<string>>(new Set());
  let globalLoading = $state(false);
  let selectedMachineId = $state<string | null>(null);
  let expandedAgent = $state<string | null>(null);

  let visibleCaps = $derived(
    selectedMachineId
      ? capabilities.filter(c => c.machineId === selectedMachineId)
      : capabilities,
  );

  let machines = $derived(capabilities.map(c => ({ id: c.machineId, name: c.machineName })));

  // ---------------------------------------------------------------------------
  // JARVIS Voice Agent server status
  // ---------------------------------------------------------------------------

  const VOICE_PORT = 3144;

  let voiceServerStatus = $state<'unknown' | 'online' | 'offline'>('unknown');
  let voiceServerChecking = $state(false);

  async function checkVoiceServer() {
    voiceServerChecking = true;
    try {
      const resp = await fetch(`http://localhost:${VOICE_PORT}/sessions`, { signal: AbortSignal.timeout(4000) });
      voiceServerStatus = resp.ok ? 'online' : 'offline';
    } catch {
      voiceServerStatus = 'offline';
    } finally {
      voiceServerChecking = false;
    }
  }

  // ---------------------------------------------------------------------------
  // Tool catalog — hard-coded from livekit-agent/plugins/
  // ---------------------------------------------------------------------------

  interface ToolDef {
    name: string;
    description: string;
  }

  interface PluginGroup {
    id: string;
    label: string;
    color: string;
    tools: ToolDef[];
  }

  const TOOL_CATALOG: PluginGroup[] = [
    {
      id: 'docker',
      label: 'DOCKER',
      color: '#2496ED',
      tools: [
        { name: 'docker_ps', description: 'Lista containers Docker corriendo en una máquina' },
        { name: 'docker_logs', description: 'Obtiene logs recientes de un container' },
        { name: 'docker_restart', description: 'Reinicia un container Docker' },
        { name: 'docker_stats', description: 'Uso de CPU/RAM de containers en una máquina' },
        { name: 'docker_compose_up', description: 'Inicia servicios Docker Compose' },
        { name: 'docker_status_summary', description: 'Resumen rápido de Docker en ATLAS y PIXEL' },
        { name: 'get_docker_stats', description: 'CPU, memoria y red de containers locales' },
        { name: 'docker_exec', description: 'Ejecuta un comando dentro de un container' },
        { name: 'get_docker_images', description: 'Lista imágenes Docker locales con tamaño' },
        { name: 'docker_pull', description: 'Descarga una imagen Docker del registro' },
        { name: 'get_docker_networks', description: 'Lista redes Docker con driver y scope' },
      ],
    },
    {
      id: 'browser',
      label: 'BROWSER',
      color: '#F4A400',
      tools: [
        { name: 'browser_navigate', description: 'Abre una URL en el browser headless' },
        { name: 'browser_get_content', description: 'Obtiene el texto visible de la página actual' },
        { name: 'browser_click', description: 'Hace click en un elemento por selector o texto' },
        { name: 'browser_type_text', description: 'Escribe texto en un campo del formulario' },
        { name: 'browser_screenshot', description: 'Captura pantalla de la página actual' },
        { name: 'browser_close', description: 'Cierra el browser headless y libera recursos' },
        { name: 'browser_current_url', description: 'Obtiene la URL actual del browser' },
        { name: 'take_screenshot_url', description: 'Captura pantalla de una URL pública' },
        { name: 'extract_links', description: 'Extrae hasta 20 links de una página web' },
        { name: 'search_web', description: 'Busca en la web con DuckDuckGo (top 5 resultados)' },
      ],
    },
    {
      id: 'utilities',
      label: 'UTILITIES',
      color: '#00FF88',
      tools: [
        { name: 'get_weather', description: 'Clima actual de una ciudad (wttr.in)' },
        { name: 'set_reminder', description: 'Programa un recordatorio con alerta de sonido' },
        { name: 'list_reminders', description: 'Lista recordatorios activos con tiempo restante' },
        { name: 'cancel_reminder', description: 'Cancela un recordatorio activo por posición' },
        { name: 'get_system_uptime', description: 'Tiempo de actividad del sistema en formato legible' },
        { name: 'speak_time', description: 'Hora y fecha actual en español natural' },
      ],
    },
    {
      id: 'monitoring',
      label: 'MONITORING',
      color: '#FF4081',
      tools: [
        { name: 'get_monitoring_status', description: 'Estado de salud de máquinas y alertas activas' },
        { name: 'check_ci_now', description: 'Estado actual de GitHub Actions CI para un repo' },
      ],
    },
    {
      id: 'system',
      label: 'SYSTEM',
      color: '#C084FC',
      tools: [
        { name: 'capture_screen', description: 'Captura pantalla de un monitor y la describe con visión' },
        { name: 'analyze_screen', description: 'Captura y analiza la pantalla con Claude Vision' },
        { name: 'open_app', description: 'Abre cualquier aplicación en el Mac' },
        { name: 'mouse_click', description: 'Hace click en coordenadas específicas de pantalla' },
        { name: 'type_text', description: 'Escribe texto usando el teclado' },
        { name: 'press_key', description: 'Presiona una tecla o atajo de teclado' },
        { name: 'get_mouse_position', description: 'Posición actual del cursor del mouse' },
        { name: 'get_active_window', description: 'Nombre y título de la ventana activa en macOS' },
        { name: 'get_clipboard', description: 'Lee el contenido de texto del portapapeles' },
        { name: 'set_clipboard', description: 'Escribe texto en el portapapeles de macOS' },
      ],
    },
    {
      id: 'projects',
      label: 'PROJECTS',
      color: '#FFB74D',
      tools: [
        { name: 'load_project', description: 'Carga un proyecto activo (por nombre, URL o "listar")' },
        { name: 'unload_project', description: 'Descarga el proyecto activo y vuelve a modo general' },
        { name: 'new_project', description: 'Crea un nuevo proyecto desde cero con scaffold' },
        { name: 'run_pipeline', description: 'Ejecuta un pipeline predefinido por nombre' },
        { name: 'create_scheduled_task', description: 'Crea una tarea cron recurrente con expresión cron' },
      ],
    },
    {
      id: 'sessions',
      label: 'SESSIONS',
      color: '#00D4FF',
      tools: [
        { name: 'start_claude_session', description: 'Inicia una sesión Claude con nombre y tarea autónoma' },
        { name: 'chat_session', description: 'Envía un mensaje a una sesión Claude por nombre' },
        { name: 'list_sessions', description: 'Lista todas las sesiones Claude activas' },
        { name: 'close_session', description: 'Cierra una sesión Claude y guarda su historial en memoria' },
        { name: 'export_session', description: 'Exporta una sesión Claude a un archivo Markdown' },
      ],
    },
  ];

  const TOTAL_TOOLS = TOOL_CATALOG.reduce((sum, g) => sum + g.tools.length, 0);

  // ---------------------------------------------------------------------------
  // Top tools (call counts)
  // ---------------------------------------------------------------------------

  let topTools = $state<ToolStat[]>([]);

  async function loadTopTools() {
    try {
      topTools = await getTopTools(200);
    } catch {
      // non-critical
    }
  }

  function toolCallCount(toolName: string): number {
    return topTools.find(t => t.toolName === toolName)?.calls ?? 0;
  }

  // ---------------------------------------------------------------------------
  // Category filter
  // ---------------------------------------------------------------------------

  let selectedCategory = $state<string | null>(null);

  // ---------------------------------------------------------------------------
  // Search state
  // ---------------------------------------------------------------------------

  let searchQuery = $state('');

  let filteredCatalog = $derived.by(() => {
    const q = searchQuery.trim().toLowerCase();
    return TOOL_CATALOG
      .filter(group => selectedCategory === null || group.id === selectedCategory)
      .map(group => ({
        ...group,
        tools: group.tools.filter(
          tool =>
            q === '' ||
            tool.name.toLowerCase().includes(q) ||
            tool.description.toLowerCase().includes(q),
        ),
      }))
      .filter(g => g.tools.length > 0);
  });

  let filteredTotalTools = $derived(filteredCatalog.reduce((sum, g) => sum + g.tools.length, 0));

  // ---------------------------------------------------------------------------
  // Machine capabilities helpers
  // ---------------------------------------------------------------------------

  function isLoadingMachine(id: string): boolean {
    return loadingIds.has(id);
  }

  function existsOnlyHere(item: string, machineId: string, getter: (c: MachineCapabilities) => string[]): boolean {
    if (capabilities.length <= 1) return false;
    const others = capabilities.filter(c => c.machineId !== machineId);
    return others.every(c => !getter(c).includes(item));
  }

  function machineAccent(index: number): string {
    const accents = ['var(--cyan)', 'var(--green)', 'var(--amber)', '#c084fc'];
    return accents[index % accents.length];
  }

  function accentForId(machineId: string): string {
    const idx = capabilities.findIndex(c => c.machineId === machineId);
    return machineAccent(idx >= 0 ? idx : 0);
  }

  function toggleAgent(key: string) {
    expandedAgent = expandedAgent === key ? null : key;
  }

  async function refreshMachine(machineId: string) {
    loadingIds = new Set([...loadingIds, machineId]);
    try {
      const fresh = await fetchSingleMachineCapabilities(machineId);
      capabilities = capabilities.map(c => c.machineId === machineId ? fresh : c);
    } catch (e) {
      addToast(t('common.error') + ': ' + handleError(e), 'error');
    } finally {
      loadingIds = new Set([...loadingIds].filter(id => id !== machineId));
    }
  }

  async function refreshAll() {
    globalLoading = true;
    try {
      capabilities = await fetchMachineCapabilities();
    } catch (e) {
      addToast(t('common.error') + ': ' + handleError(e), 'error');
    } finally {
      globalLoading = false;
    }
  }

  onMount(() => {
    refreshAll();
    checkVoiceServer();
    loadTopTools();
  });
</script>

<div class="cap-tab">
  <!-- Toolbar -->
  <div class="cap-toolbar">
    <span class="cap-title">{$tr('caps.title')}</span>
    <button
      class="refresh-btn"
      class:spinning={globalLoading}
      disabled={globalLoading}
      onclick={refreshAll}
    >
      {globalLoading ? $tr('common.loading') : $tr('caps.refreshAll')}
    </button>
  </div>

  <!-- Scrollable content area -->
  <div class="cap-scroll">

    <!-- Voice Agent status card -->
    <div class="voice-card">
      <div class="voice-header">
        <span class="voice-icon">V</span>
        <span class="voice-title">JARVIS Voice Agent</span>
        <span class="voice-port">:{ VOICE_PORT }</span>
        <button
          class="verify-btn"
          class:checking={voiceServerChecking}
          disabled={voiceServerChecking}
          onclick={checkVoiceServer}
        >
          {voiceServerChecking ? '...' : 'Verificar'}
        </button>
      </div>
      <div class="voice-status-row">
        {#if voiceServerStatus === 'unknown'}
          <span class="status-dot dot-unknown"></span>
          <span class="status-text status-unknown">Verificando...</span>
        {:else if voiceServerStatus === 'online'}
          <span class="status-dot dot-online"></span>
          <span class="status-text status-online">Servidor activo en :{VOICE_PORT}</span>
        {:else}
          <span class="status-dot dot-offline"></span>
          <span class="status-text status-offline">Servidor offline</span>
        {/if}
        <span class="tools-summary">{TOTAL_TOOLS} herramientas disponibles en {TOOL_CATALOG.length} plugins</span>
      </div>
    </div>

    <!-- Tool catalog search -->
    <div class="catalog-header">
      <div class="catalog-title-row">
        <span class="catalog-title">CATÁLOGO DE HERRAMIENTAS</span>
        <span class="catalog-count">
          {filteredTotalTools} / {TOTAL_TOOLS} herramientas
        </span>
      </div>
      <div class="search-wrap">
        <span class="search-icon">⌕</span>
        <input
          class="search-input"
          type="text"
          placeholder="Buscar herramienta..."
          bind:value={searchQuery}
        />
        {#if searchQuery}
          <button class="search-clear" onclick={() => (searchQuery = '')}>✕</button>
        {/if}
      </div>
    </div>

    <!-- Category filter tabs -->
    <div class="category-tabs">
      <button
        class="cat-tab"
        class:cat-tab-active={selectedCategory === null}
        onclick={() => (selectedCategory = null)}
      >Todos</button>
      {#each TOOL_CATALOG as group}
        <button
          class="cat-tab"
          class:cat-tab-active={selectedCategory === group.id}
          style="--cat-color: {group.color}"
          onclick={() => (selectedCategory = selectedCategory === group.id ? null : group.id)}
        >{group.label}</button>
      {/each}
    </div>

    <!-- Tool groups -->
    <div class="catalog-groups">
      {#if filteredCatalog.length === 0}
        <div class="catalog-empty">Sin resultados{searchQuery ? ` para "${searchQuery}"` : ''}</div>
      {:else}
        {#each filteredCatalog as group}
          <div class="tool-group" style="--group-color: {group.color}">
            <div class="group-header">
              <span class="group-label">{group.label}</span>
              <span class="group-count">{group.tools.length} herramientas</span>
            </div>
            <div class="group-tools">
              {#each group.tools as tool}
                {@const callCount = toolCallCount(tool.name)}
                <div class="tool-row">
                  <span class="tool-glyph">◈</span>
                  <span class="tool-name">{tool.name}</span>
                  <span class="tool-dash">—</span>
                  <span class="tool-desc">{tool.description}</span>
                  {#if callCount > 0}
                    <span class="tool-calls-badge" title="{callCount} calls">{callCount}</span>
                  {/if}
                </div>
              {/each}
            </div>
          </div>
        {/each}
      {/if}
    </div>

    <!-- Machine capabilities section -->
    <div class="section-sep">
      <span class="section-sep-label">CAPACIDADES POR MÁQUINA</span>
    </div>

    <!-- Machine selector pills -->
    {#if machines.length > 1}
      <div class="machine-pills">
        <button
          class="pill"
          class:pill-active={selectedMachineId === null}
          onclick={() => (selectedMachineId = null)}
        >
          {$tr('caps.all')}
        </button>
        {#each machines as m, idx}
          <button
            class="pill"
            class:pill-active={selectedMachineId === m.id}
            style="--pill-color: {machineAccent(idx)}"
            onclick={() => (selectedMachineId = selectedMachineId === m.id ? null : m.id)}
          >
            {m.name}
            {#if isLoadingMachine(m.id)}
              <span class="pill-spinner"></span>
            {/if}
          </button>
        {/each}
      </div>
    {/if}

    <!-- Capability cards -->
    {#if globalLoading && capabilities.length === 0}
      <div class="cards-grid">
        {#each [0, 1, 2, 3] as _}
          <div class="cap-card skeleton-card">
            <Skeleton width="40%" height="16px" />
            <Skeleton width="100%" height="12px" count={3} />
            <Skeleton width="60%" height="10px" count={2} />
          </div>
        {/each}
      </div>
    {:else if capabilities.length === 0}
      <div class="empty-state">{$tr('caps.none')}</div>
    {:else}
      <div class="cards-grid">
        {#each visibleCaps as cap, idx}
          {@const accent = accentForId(cap.machineId)}
          {@const loading = isLoadingMachine(cap.machineId)}
          <div class="cap-card" class:card-loading={loading} style="--card-accent: {accent}">
            <div class="card-header">
              <span class="card-name" style="color: {accent}">{cap.machineName}</span>
              <span class="card-id">{cap.machineId}</span>
              <button
                class="card-refresh-btn"
                class:spinning={loading}
                disabled={loading}
                title={$tr('common.refresh')}
                onclick={() => refreshMachine(cap.machineId)}
              >
                {#if loading}
                  {$tr('caps.loadingMachine')}
                {:else}
                  ↻
                {/if}
              </button>
            </div>

            <!-- Plugins -->
            <div class="section">
              <div class="section-title">
                <span class="section-icon">P</span>
                {$tr('caps.plugins')}
                <span class="section-count">{cap.plugins.length}</span>
              </div>
              {#if cap.plugins.length === 0}
                <div class="section-empty">{$tr('caps.noneDetected')}</div>
              {:else}
                <div class="items-list">
                  {#each cap.plugins as plugin}
                    {@const unique = existsOnlyHere(plugin.name, cap.machineId, c => c.plugins.map(p => p.name))}
                    <div class="item-row" class:unique-item={unique}>
                      <span class="item-name">{plugin.name}</span>
                      <span class="badge" class:badge-on={plugin.enabled} class:badge-off={!plugin.enabled}>
                        {plugin.enabled ? $tr('common.on') : $tr('common.off')}
                      </span>
                      {#if unique}
                        <span class="unique-badge">{$tr('common.unique')}</span>
                      {/if}
                    </div>
                  {/each}
                </div>
              {/if}
            </div>

            <!-- Agents -->
            <div class="section">
              <div class="section-title">
                <span class="section-icon">A</span>
                {$tr('caps.agents')}
                <span class="section-count">{cap.agents.length}</span>
              </div>
              {#if cap.agents.length === 0}
                <div class="section-empty">{$tr('caps.noneDetected')}</div>
              {:else}
                <div class="items-list">
                  {#each cap.agents as agent}
                    {@const agentKey = cap.machineId + ':' + agent.filename}
                    {@const unique = existsOnlyHere(agent.filename, cap.machineId, c => c.agents.map(a => a.filename))}
                    <button
                      class="item-row agent-row"
                      class:unique-item={unique}
                      class:expanded={expandedAgent === agentKey}
                      onclick={() => toggleAgent(agentKey)}
                    >
                      <span class="item-name">{agent.filename}</span>
                      {#if unique}
                        <span class="unique-badge">{$tr('common.unique')}</span>
                      {/if}
                      <span class="expand-arrow">{expandedAgent === agentKey ? '\u25B2' : '\u25BC'}</span>
                    </button>
                    {#if expandedAgent === agentKey}
                      <div class="agent-preview">{agent.contentPreview || $tr('caps.noContent')}</div>
                    {/if}
                  {/each}
                </div>
              {/if}
            </div>

            <!-- Skills Used -->
            <div class="section">
              <div class="section-title">
                <span class="section-icon">S</span>
                {$tr('caps.skills')}
                <span class="section-count">{cap.skillsUsed.length}</span>
              </div>
              {#if cap.skillsUsed.length === 0}
                <div class="section-empty">{$tr('caps.noneDetected')}</div>
              {:else}
                <div class="chips-wrap">
                  {#each cap.skillsUsed as skill}
                    {@const unique = existsOnlyHere(skill, cap.machineId, c => c.skillsUsed)}
                    <span class="skill-chip" class:unique-chip={unique} title={unique ? $tr('caps.onlyHere') : ''}>
                      {skill}
                    </span>
                  {/each}
                </div>
              {/if}
            </div>

            <!-- MCPs -->
            <div class="section">
              <div class="section-title">
                <span class="section-icon">M</span>
                {$tr('caps.mcps')}
                <span class="section-count">{cap.mcps.length}</span>
              </div>
              {#if cap.mcps.length === 0}
                <div class="section-empty">{$tr('caps.noneDetected')}</div>
              {:else}
                <div class="chips-wrap">
                  {#each cap.mcps as mcp}
                    {@const unique = existsOnlyHere(mcp, cap.machineId, c => c.mcps)}
                    <span class="mcp-chip" class:unique-chip={unique} title={unique ? $tr('caps.onlyHere') : ''}>
                      {mcp}
                    </span>
                  {/each}
                </div>
              {/if}
            </div>
          </div>
        {/each}
      </div>
    {/if}

  </div><!-- end cap-scroll -->
</div>

<style>
  .cap-tab {
    display: flex;
    flex-direction: column;
    height: 100%;
    width: 100%;
    overflow: hidden;
  }

  .cap-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 12px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }

  .cap-title {
    font-family: var(--font-display);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 1px;
    text-transform: uppercase;
    color: var(--text-1);
  }

  .refresh-btn {
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 600;
    letter-spacing: 0.5px;
    text-transform: uppercase;
    color: var(--cyan);
    background: rgba(0, 212, 255, 0.07);
    border: 1px solid rgba(0, 212, 255, 0.2);
    border-radius: var(--radius);
    padding: 4px 12px;
    cursor: pointer;
    transition: background 0.15s ease, border-color 0.15s ease;
  }

  .refresh-btn:hover {
    background: rgba(0, 212, 255, 0.13);
    border-color: rgba(0, 212, 255, 0.35);
  }

  .refresh-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .refresh-btn.spinning {
    animation: pulse-glow 1s infinite;
  }

  /* Main scrollable area */
  .cap-scroll {
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
    padding: 10px 12px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  /* ---------------------------------------------------------------------------
     Voice Agent card
  --------------------------------------------------------------------------- */

  .voice-card {
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 10px 12px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    border-top: 2px solid var(--cyan);
    box-shadow: 0 0 8px rgba(0, 212, 255, 0.06);
    flex-shrink: 0;
  }

  .voice-header {
    display: flex;
    align-items: center;
    gap: 7px;
  }

  .voice-icon {
    width: 18px;
    height: 18px;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 9px;
    font-weight: 800;
    background: rgba(0, 212, 255, 0.12);
    border: 1px solid rgba(0, 212, 255, 0.3);
    border-radius: 4px;
    color: var(--cyan);
    flex-shrink: 0;
  }

  .voice-title {
    font-family: var(--font-display);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 1.5px;
    text-transform: uppercase;
    color: var(--cyan);
  }

  .voice-port {
    font-family: var(--font-mono);
    font-size: 9px;
    color: var(--text-3);
    background: var(--bg-0);
    border: 1px solid var(--border);
    border-radius: 3px;
    padding: 1px 5px;
  }

  .verify-btn {
    margin-left: auto;
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 600;
    letter-spacing: 0.5px;
    text-transform: uppercase;
    color: var(--text-2);
    background: var(--bg-3);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 3px 9px;
    cursor: pointer;
    transition: background 0.15s ease, color 0.15s ease, border-color 0.15s ease;
  }

  .verify-btn:hover {
    color: var(--cyan);
    border-color: rgba(0, 212, 255, 0.3);
    background: rgba(0, 212, 255, 0.07);
  }

  .verify-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .verify-btn.checking {
    animation: pulse-glow 1s infinite;
    color: var(--cyan);
  }

  .voice-status-row {
    display: flex;
    align-items: center;
    gap: 7px;
    padding-left: 2px;
  }

  .status-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .dot-online {
    background: var(--green);
    box-shadow: 0 0 5px rgba(0, 255, 136, 0.5);
    animation: pulse-dot 2s ease-in-out infinite;
  }

  .dot-offline {
    background: #ff4444;
    box-shadow: 0 0 5px rgba(255, 68, 68, 0.4);
  }

  .dot-unknown {
    background: var(--text-3);
    animation: pulse-dot 1s ease-in-out infinite;
  }

  @keyframes pulse-dot {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.5; }
  }

  .status-text {
    font-family: var(--font-display);
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.3px;
  }

  .status-online  { color: var(--green); }
  .status-offline { color: #ff4444; }
  .status-unknown { color: var(--text-3); }

  .tools-summary {
    margin-left: auto;
    font-size: 9px;
    color: var(--text-3);
    font-family: var(--font-display);
    letter-spacing: 0.3px;
  }

  /* ---------------------------------------------------------------------------
     Tool catalog
  --------------------------------------------------------------------------- */

  .catalog-header {
    display: flex;
    flex-direction: column;
    gap: 6px;
    flex-shrink: 0;
  }

  .catalog-title-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .catalog-title {
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 1.5px;
    text-transform: uppercase;
    color: var(--text-2);
  }

  .catalog-count {
    font-size: 9px;
    font-family: var(--font-mono);
    color: var(--text-3);
  }

  .search-wrap {
    display: flex;
    align-items: center;
    gap: 6px;
    background: var(--bg-0);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 4px 8px;
    transition: border-color 0.15s ease;
  }

  .search-wrap:focus-within {
    border-color: rgba(0, 212, 255, 0.4);
  }

  .search-icon {
    color: var(--text-3);
    font-size: 12px;
    flex-shrink: 0;
  }

  .search-input {
    flex: 1;
    background: transparent;
    border: none;
    outline: none;
    color: var(--text-1);
    font-size: 11px;
    font-family: var(--font-mono);
    min-width: 0;
  }

  .search-input::placeholder {
    color: var(--text-3);
  }

  .search-clear {
    background: none;
    border: none;
    color: var(--text-3);
    font-size: 9px;
    cursor: pointer;
    padding: 0 2px;
    line-height: 1;
    transition: color 0.15s ease;
  }

  .search-clear:hover {
    color: var(--text-1);
  }

  /* Category filter tabs */
  .category-tabs {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-wrap: wrap;
    flex-shrink: 0;
  }

  .cat-tab {
    padding: 2px 9px;
    border-radius: 10px;
    font-family: var(--font-display);
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.8px;
    text-transform: uppercase;
    border: 1px solid var(--border);
    background: var(--bg-2);
    color: var(--text-3);
    cursor: pointer;
    transition: background 0.15s ease, border-color 0.15s ease, color 0.15s ease;
  }

  .cat-tab:hover {
    color: var(--text-1);
    background: var(--bg-3);
    border-color: var(--border-bright);
  }

  .cat-tab-active {
    background: color-mix(in srgb, var(--cat-color, var(--cyan)) 12%, transparent);
    border-color: color-mix(in srgb, var(--cat-color, var(--cyan)) 40%, transparent);
    color: var(--cat-color, var(--cyan));
  }

  .catalog-groups {
    display: flex;
    flex-direction: column;
    gap: 8px;
    flex-shrink: 0;
  }

  .catalog-empty {
    font-size: 11px;
    font-style: italic;
    color: var(--text-3);
    padding: 12px 4px;
    text-align: center;
  }

  .tool-group {
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: 6px;
    overflow: hidden;
    border-left: 2px solid var(--group-color);
  }

  .group-header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    background: var(--bg-0);
    border-bottom: 1px solid var(--border);
  }

  .group-label {
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 800;
    letter-spacing: 1.5px;
    text-transform: uppercase;
    color: var(--group-color);
  }

  .group-count {
    margin-left: auto;
    font-size: 9px;
    color: var(--text-3);
    font-family: var(--font-mono);
  }

  .group-tools {
    display: flex;
    flex-direction: column;
    padding: 4px 0;
  }

  .tool-row {
    display: flex;
    align-items: baseline;
    gap: 5px;
    padding: 3px 10px;
    transition: background 0.1s ease;
  }

  .tool-row:hover {
    background: var(--bg-3);
  }

  .tool-glyph {
    font-size: 8px;
    color: var(--group-color);
    opacity: 0.7;
    flex-shrink: 0;
    line-height: 1.4;
  }

  .tool-name {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--text-1);
    font-weight: 600;
    white-space: nowrap;
    flex-shrink: 0;
  }

  .tool-dash {
    font-size: 9px;
    color: var(--text-3);
    flex-shrink: 0;
  }

  .tool-desc {
    font-size: 10px;
    color: var(--text-2);
    line-height: 1.4;
    min-width: 0;
    flex: 1;
  }

  .tool-calls-badge {
    margin-left: auto;
    padding: 1px 5px;
    border-radius: 8px;
    font-family: var(--font-mono);
    font-size: 8px;
    font-weight: 700;
    background: rgba(0, 212, 255, 0.1);
    color: var(--cyan);
    border: 1px solid rgba(0, 212, 255, 0.25);
    flex-shrink: 0;
    white-space: nowrap;
  }

  /* ---------------------------------------------------------------------------
     Section separator
  --------------------------------------------------------------------------- */

  .section-sep {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
  }

  .section-sep::before,
  .section-sep::after {
    content: '';
    flex: 1;
    height: 1px;
    background: var(--border);
  }

  .section-sep-label {
    font-family: var(--font-display);
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 1.5px;
    text-transform: uppercase;
    color: var(--text-3);
    white-space: nowrap;
    flex-shrink: 0;
  }

  /* Machine selector pills */
  .machine-pills {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
    flex-wrap: wrap;
  }

  .pill {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 3px 10px;
    border-radius: 12px;
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.5px;
    text-transform: uppercase;
    border: 1px solid var(--border);
    background: var(--bg-2);
    color: var(--text-2);
    cursor: pointer;
    transition: background 0.15s ease, border-color 0.15s ease, color 0.15s ease;
  }

  .pill:hover {
    border-color: var(--border-bright);
    color: var(--text-1);
    background: var(--bg-3);
  }

  .pill-active {
    background: rgba(0, 212, 255, 0.1);
    border-color: rgba(0, 212, 255, 0.35);
    color: var(--cyan);
  }

  .pill-active[style] {
    background: color-mix(in srgb, var(--pill-color) 10%, transparent);
    border-color: color-mix(in srgb, var(--pill-color) 35%, transparent);
    color: var(--pill-color);
  }

  .pill-spinner {
    width: 6px;
    height: 6px;
    border: 1px solid currentColor;
    border-top-color: transparent;
    border-radius: 50%;
    animation: spin 0.7s linear infinite;
    flex-shrink: 0;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  /* Card loading overlay */
  .card-loading {
    opacity: 0.7;
    pointer-events: none;
  }

  /* Per-card refresh button */
  .card-refresh-btn {
    margin-left: auto;
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 600;
    color: var(--text-3);
    background: transparent;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 2px 7px;
    cursor: pointer;
    transition: color 0.15s ease, border-color 0.15s ease, background 0.15s ease;
  }

  .card-refresh-btn:hover {
    color: var(--text-1);
    border-color: var(--border-bright);
    background: var(--bg-3);
  }

  .card-refresh-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .card-refresh-btn.spinning {
    animation: pulse-glow 1s infinite;
    color: var(--cyan);
    border-color: rgba(0, 212, 255, 0.3);
  }

  .skeleton-card {
    pointer-events: none;
  }

  .empty-state {
    color: var(--text-3);
    font-size: 11px;
    font-style: italic;
    padding: 24px;
    text-align: center;
  }

  .cards-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
    gap: 10px;
    align-content: start;
    flex-shrink: 0;
  }

  .cap-card {
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    position: relative;
    overflow-y: auto;
    overflow-x: hidden;
    transition: border-color 0.2s ease, box-shadow 0.2s ease, opacity 0.2s ease;
  }

  .cap-card::before {
    content: '';
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    height: 2px;
    background: var(--card-accent);
    box-shadow: 0 0 8px color-mix(in srgb, var(--card-accent) 40%, transparent);
  }

  .cap-card:hover {
    border-color: var(--border-bright);
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.3);
  }

  .card-header {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .card-name {
    font-family: var(--font-display);
    font-size: 13px;
    font-weight: 700;
    letter-spacing: 2px;
    text-transform: uppercase;
  }

  .card-id {
    font-size: 9px;
    color: var(--text-3);
    background: var(--bg-3);
    padding: 2px 6px;
    border-radius: 3px;
    font-family: var(--font-mono);
  }

  .section {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .section-title {
    display: flex;
    align-items: center;
    gap: 6px;
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 1px;
    text-transform: uppercase;
    color: var(--text-2);
    padding-bottom: 2px;
    border-bottom: 1px solid var(--border);
  }

  .section-icon {
    width: 16px;
    height: 16px;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 8px;
    font-weight: 800;
    background: var(--bg-0);
    border: 1px solid var(--border);
    border-radius: 3px;
    color: var(--text-2);
    flex-shrink: 0;
  }

  .section-count {
    margin-left: auto;
    font-size: 9px;
    color: var(--text-3);
    font-weight: 600;
  }

  .section-empty {
    font-size: 10px;
    color: var(--text-3);
    font-style: italic;
    padding: 2px 0 2px 22px;
  }

  .items-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .item-row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 3px 6px 3px 22px;
    border-radius: 3px;
    font-size: 10px;
    transition: background 0.1s ease;
  }

  .item-row:hover {
    background: var(--bg-3);
  }

  .item-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-1);
    font-family: var(--font-mono);
    font-size: 10px;
  }

  .badge {
    padding: 1px 6px;
    border-radius: 3px;
    font-family: var(--font-display);
    font-size: 7px;
    font-weight: 700;
    letter-spacing: 0.5px;
    text-transform: uppercase;
    flex-shrink: 0;
  }

  .badge-on {
    background: rgba(0, 255, 136, 0.1);
    color: var(--green);
    border: 1px solid rgba(0, 255, 136, 0.25);
  }

  .badge-off {
    background: rgba(255, 255, 255, 0.04);
    color: var(--text-3);
    border: 1px solid var(--border);
  }

  .unique-badge {
    padding: 1px 5px;
    border-radius: 3px;
    font-family: var(--font-display);
    font-size: 7px;
    font-weight: 700;
    letter-spacing: 0.5px;
    text-transform: uppercase;
    background: rgba(255, 183, 77, 0.12);
    color: var(--amber);
    border: 1px solid rgba(255, 183, 77, 0.3);
    flex-shrink: 0;
  }

  .unique-item {
    background: rgba(255, 183, 77, 0.04);
  }

  .agent-row {
    width: 100%;
    border: none;
    background: transparent;
    cursor: pointer;
    text-align: left;
    font: inherit;
    color: inherit;
  }

  .agent-row.expanded {
    background: var(--bg-3);
  }

  .expand-arrow {
    font-size: 8px;
    color: var(--text-3);
    flex-shrink: 0;
    margin-left: auto;
  }

  .agent-preview {
    font-size: 9px;
    color: var(--text-2);
    background: var(--bg-0);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 6px 10px;
    margin: 2px 0 4px 22px;
    white-space: pre-wrap;
    word-break: break-word;
    line-height: 1.5;
    max-height: 80px;
    overflow-y: auto;
    font-family: var(--font-mono);
  }

  .chips-wrap {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    padding: 2px 0 2px 22px;
  }

  .skill-chip {
    padding: 2px 8px;
    border-radius: 3px;
    font-size: 9px;
    font-family: var(--font-mono);
    background: rgba(0, 212, 255, 0.08);
    color: var(--cyan);
    border: 1px solid rgba(0, 212, 255, 0.2);
    transition: background 0.15s ease, transform 0.1s ease;
    cursor: default;
  }

  .skill-chip:hover {
    background: rgba(0, 212, 255, 0.15);
    transform: translateY(-1px);
  }

  .mcp-chip {
    padding: 2px 8px;
    border-radius: 3px;
    font-size: 9px;
    font-family: var(--font-mono);
    background: rgba(192, 132, 252, 0.08);
    color: #c084fc;
    border: 1px solid rgba(192, 132, 252, 0.2);
    transition: background 0.15s ease, transform 0.1s ease;
    cursor: default;
  }

  .mcp-chip:hover {
    background: rgba(192, 132, 252, 0.15);
    transform: translateY(-1px);
  }

  .unique-chip {
    box-shadow: 0 0 6px rgba(255, 183, 77, 0.2);
    border-color: rgba(255, 183, 77, 0.35);
  }
</style>
