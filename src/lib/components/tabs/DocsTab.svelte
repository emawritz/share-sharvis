<script lang="ts">
  import { sendTask as apiSendTask, getRepoStatuses, sendTaskGraph, onTaskStarted, onTaskDone, fetchTasks } from '../../api';
  import { addToast } from '../../stores/notifications';
  import { handleError } from '../../utils';
  import type { TaskGraphNode, TaskGraph, Task } from '../../types';

  import { onMount } from 'svelte';

  let fileInputEl: HTMLInputElement | undefined = $state();
  let fileName = $state('');
  let fileContent = $state('');
  let fileLines = $state(0);
  let loaded = $state(false);
  let sending = $state(false);
  let searchQuery = $state('');
  let debouncedQuery = $state('');
  let viewMode = $state<'preview' | 'raw'>('preview');
  let debounceTimer: ReturnType<typeof setTimeout> | undefined;
  let dragging = $state(false);

  const MAX_FILE_SIZE = 10 * 1024 * 1024;
  const PAGE_SIZE = 500;
  let currentPage = $state(0);

  // Execution pipeline state
  type ExecPhase = 'idle' | 'analyzing' | 'review' | 'review_raw' | 'executing' | 'done';
  type PlanStep = { target: string; description: string; status: 'pending' | 'running' | 'done' | 'error'; taskId?: number; output?: string };

  let phase = $state<ExecPhase>('idle');
  let planSteps = $state<PlanStep[]>([]);
  let repoInfo = $state('');
  let analyzeOutput = $state('');
  let executionTaskIds = $state<number[]>([]);
  let pollInterval: ReturnType<typeof setInterval> | undefined;

  let completedCount = $derived(planSteps.filter(s => s.status === 'done').length);
  let errorCount = $derived(planSteps.filter(s => s.status === 'error').length);
  let runningCount = $derived(planSteps.filter(s => s.status === 'running').length);

  onMount(() => {
    // Listen for task events to update step status in real-time
    let unStarted: (() => void) | undefined;
    let unDone: (() => void) | undefined;

    onTaskStarted((data) => {
      planSteps = planSteps.map(s =>
        s.taskId === data.id ? { ...s, status: 'running' as const } : s
      );
    }).then(fn => { unStarted = fn; });

    onTaskDone((data) => {
      planSteps = planSteps.map(s => {
        if (s.taskId !== data.id) return s;
        const hasError = data.output?.toLowerCase().includes('error:') || data.output?.toLowerCase().includes('fatal:');
        return { ...s, status: hasError ? 'error' as const : 'done' as const, output: data.output?.substring(0, 500) };
      });
      // Check if all done
      const allDone = planSteps.every(s => s.status === 'done' || s.status === 'error');
      if (allDone && phase === 'executing') {
        phase = 'done';
        const doneNow = planSteps.filter(s => s.status === 'done').length;
        const errNow = planSteps.filter(s => s.status === 'error').length;
        addToast(`Ejecucion completada: ${doneNow} OK, ${errNow} errores`, errNow > 0 ? 'error' : 'success');
      }
    }).then(fn => { unDone = fn; });

    return () => {
      unStarted?.();
      unDone?.();
      if (pollInterval) { clearInterval(pollInterval); pollInterval = undefined; }
    };
  });

  $effect(() => {
    const q = searchQuery;
    clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => { debouncedQuery = q; }, 200);
  });

  let totalPages = $derived(Math.ceil(fileLines / PAGE_SIZE));
  let lines = $derived(fileContent.split('\n'));
  let visibleLines = $derived(
    lines.slice(currentPage * PAGE_SIZE, (currentPage + 1) * PAGE_SIZE)
  );
  let filteredLines = $derived.by(() => {
    if (!debouncedQuery.trim()) return null;
    const q = debouncedQuery.toLowerCase();
    const results: { num: number; text: string }[] = [];
    for (let i = 0; i < lines.length; i++) {
      if (lines[i].toLowerCase().includes(q)) {
        results.push({ num: i + 1, text: lines[i] });
      }
      if (results.length >= 200) break;
    }
    return results;
  });

  function triggerUpload() {
    fileInputEl?.click();
  }

  function processFile(file: File) {
    if (!file.name.endsWith('.md') && !file.name.endsWith('.txt')) {
      addToast('Solo archivos .md o .txt', 'error');
      return;
    }
    if (file.size > MAX_FILE_SIZE) {
      addToast('Archivo demasiado grande (max 10 MB)', 'error');
      return;
    }
    file.text().then((text) => {
      fileName = file.name;
      fileContent = text;
      fileLines = text.length === 0 ? 0 : text.split('\n').length;
      loaded = true;
      resetAll();
      currentPage = 0;
      searchQuery = '';
      debouncedQuery = '';
      addToast(`${file.name} cargado (${fileLines.toLocaleString()} lineas)`, 'success');
    }).catch((err) => {
      addToast('Error: ' + handleError(err), 'error');
    });
  }

  function handleFile(e: Event) {
    const input = e.target as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) return;
    processFile(file);
    input.value = '';
  }

  function handleDragOver(e: DragEvent) {
    e.preventDefault();
    dragging = true;
  }

  function handleDragLeave() {
    dragging = false;
  }

  function handleDrop(e: DragEvent) {
    e.preventDefault();
    dragging = false;
    const file = e.dataTransfer?.files?.[0];
    if (file) processFile(file);
  }

  // PHASE 1: Analyze — send .md to agent, get structured task list back
  async function startAnalysis() {
    if (!fileContent) return;
    phase = 'analyzing';
    sending = true;
    planSteps = [];
    analyzeOutput = '';

    try {
      // Gather repo status
      try {
        const [atlas, pixel] = await getRepoStatuses();
        repoInfo = `ATLAS: branch=${atlas.branch}, changed=${atlas.changed}, staged=${atlas.staged}, ahead=${atlas.ahead}, behind=${atlas.behind}, lastCommit="${atlas.lastCommit}"\nPIXEL: branch=${pixel.branch}, changed=${pixel.changed}, staged=${pixel.staged}, ahead=${pixel.ahead}, behind=${pixel.behind}, lastCommit="${pixel.lastCommit}"`;
      } catch { repoInfo = 'No se pudo obtener estado de repos'; }

      // Build analysis prompt
      const MAX_CHARS = 30000;
      const content = fileContent.length > MAX_CHARS
        ? fileContent.substring(0, MAX_CHARS) + `\n\n[... truncado: ${fileContent.length} chars de ${fileLines} lineas total]`
        : fileContent;

      const prompt = `Sos un planificador de tareas. Te doy un documento con requerimientos/plan y el estado actual de los repos.

ESTADO DE REPOS:
${repoInfo}

DOCUMENTO (${fileName}, ${fileLines} lineas):
${content}

INSTRUCCIONES:
1. Analiza el documento completo
2. Desglosa en tareas CONCRETAS y ejecutables
3. Para cada tarea usa EXACTAMENTE este formato (una por linea):
TASK|atlas|descripcion concreta de lo que hay que hacer
TASK|pixel|descripcion concreta de lo que hay que hacer

Reglas:
- atlas = backend/local, pixel = frontend/GPU/remoto
- Si hay cambios sin commitear, la primera tarea debe ser resolverlos
- Ordena las tareas por dependencia (las primeras se ejecutan primero)
- Cada tarea debe ser autocontenida y ejecutable por Claude
- Se especifico: "Implementar endpoint POST /api/users en src/routes/users.rs" NO "hacer el backend"
- Maximo 20 tareas
- Responde SOLO con las lineas TASK|target|descripcion, nada mas`;

      const task = await apiSendTask('atlas', prompt);

      // Poll for result — store interval so cleanup can clear it
      addToast('Analizando documento...', 'info');
      if (pollInterval) clearInterval(pollInterval);
      pollInterval = setInterval(async () => {
        try {
          const tasks = await fetchTasks();
          const t = tasks.find((x: Task) => x.id === task.id);
          if (!t) return;
          if (t.status === 'done' || t.status === 'error' || t.status === 'timeout') {
            clearInterval(pollInterval);
            pollInterval = undefined;
            analyzeOutput = t.output || '';
            if (t.status !== 'done' || !t.output) {
              phase = 'idle';
              sending = false;
              addToast('Error en analisis: ' + (t.output?.substring(0, 200) || 'sin respuesta'), 'error');
              return;
            }
            // Parse TASK lines — tolerate markdown wrapping, leading whitespace, backticks, list markers
            const parsed: PlanStep[] = [];
            for (const line of t.output.split('\n')) {
              const cleaned = line.replace(/^[\s`*\->]+/, '').replace(/[`*]+$/, '').trim();
              const m = cleaned.match(/^TASK\|(\w+)\|(.+)$/);
              if (m) {
                parsed.push({ target: m[1].toLowerCase(), description: m[2].trim(), status: 'pending' });
              }
            }
            if (parsed.length === 0) {
              // Show raw output so user can see what the agent returned
              addToast('No se pudieron parsear tareas. Se muestra la respuesta del agente.', 'error');
              phase = 'review_raw';
            } else {
              planSteps = parsed;
              phase = 'review';
              addToast(`${parsed.length} tareas identificadas — revisa y aprueba`, 'success');
            }
            sending = false;
          }
        } catch { /* keep polling */ }
      }, 3000);
    } catch (err) {
      addToast('Error: ' + handleError(err), 'error');
      phase = 'idle';
      sending = false;
    }
  }

  // PHASE 2: User can edit/remove steps, then approve

  function removeStep(idx: number) {
    planSteps = planSteps.filter((_, i) => i !== idx);
    if (planSteps.length === 0) phase = 'idle';
  }

  function moveStep(idx: number, dir: -1 | 1) {
    const newIdx = idx + dir;
    if (newIdx < 0 || newIdx >= planSteps.length) return;
    const copy = [...planSteps];
    [copy[idx], copy[newIdx]] = [copy[newIdx], copy[idx]];
    planSteps = copy;
  }

  // PHASE 3: Execute — send as task graph (parallel per machine, sequential within)
  async function executeSteps() {
    phase = 'executing';
    try {
      // Build DAG: steps for same target chain sequentially, different targets run in parallel
      const lastByTarget: Record<string, string> = {};
      const nodes: TaskGraphNode[] = planSteps.map((s, i) => {
        const nodeId = `step-${i}`;
        const deps: string[] = [];
        if (lastByTarget[s.target]) {
          deps.push(lastByTarget[s.target]);
        }
        lastByTarget[s.target] = nodeId;
        return {
          id: nodeId,
          target: s.target,
          prompt: s.description,
          dependsOn: deps,
          onFailure: 'skip_dependents',
        };
      });

      const graph: TaskGraph = { nodes };
      const taskIds = await sendTaskGraph(graph);

      // Map task IDs back to steps — status will be updated by onTaskStarted events
      planSteps = planSteps.map((s, i) => ({
        ...s,
        taskId: taskIds[i],
        status: 'pending' as const,
      }));
      executionTaskIds = taskIds;
      addToast(`Ejecutando ${taskIds.length} tareas (paralelo por maquina)...`, 'info');
    } catch (err) {
      addToast('Error ejecutando: ' + handleError(err), 'error');
      phase = 'review';
    }
  }

  function cancelExecution() {
    phase = 'review';
    planSteps = planSteps.map(s => ({ ...s, status: 'pending' as const, taskId: undefined, output: undefined }));
  }

  function resetAll() {
    if (pollInterval) { clearInterval(pollInterval); pollInterval = undefined; }
    phase = 'idle';
    planSteps = [];
    analyzeOutput = '';
    executionTaskIds = [];
  }

  function clearFile() {
    fileName = '';
    fileContent = '';
    fileLines = 0;
    loaded = false;
    currentPage = 0;
    searchQuery = '';
    debouncedQuery = '';
    resetAll();
  }

  function goPage(page: number) {
    if (page >= 0 && page < totalPages) currentPage = page;
  }

  function renderMarkdown(text: string): string {
    let escaped = text
      .replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');

    const codeBlocks: string[] = [];
    escaped = escaped.replace(/```[^\n]*\n([\s\S]*?)```/g, (_match, code) => {
      const idx = codeBlocks.length;
      codeBlocks.push(`<pre><code>${code.replace(/\n$/, '')}</code></pre>`);
      return `\x00CB${idx}\x00`;
    });

    escaped = escaped
      .replace(/^### (.+)$/gm, '<h4>$1</h4>')
      .replace(/^## (.+)$/gm, '<h3>$1</h3>')
      .replace(/^# (.+)$/gm, '<h2>$1</h2>')
      .replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>')
      .replace(/^[-─—]{3,}$/gm, '<hr>')
      .replace(/^- (.+)$/gm, '<li>$1</li>')
      .replace(/^(\d+)\. (.+)$/gm, '<li>$2</li>')
      .replace(/`([^`]+)`/g, '<code>$1</code>')
      .replace(/\n{2,}/g, '<br><br>')
      .replace(/\n/g, '<br>');

    escaped = escaped.replace(/\x00CB(\d+)\x00/g, (_m, idx) => codeBlocks[Number(idx)]);

    return escaped;
  }

  let renderedPreview = $derived(
    viewMode === 'preview' ? renderMarkdown(visibleLines.join('\n')) : ''
  );
</script>

<input type="file" accept=".md,.txt" class="sr-only" bind:this={fileInputEl} onchange={handleFile} />

<div class="docs-tab">
  {#if !loaded}
    <div class="empty-state">
      <div class="upload-area" class:dragging role="button" tabindex="0"
        onclick={triggerUpload}
        onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') triggerUpload(); }}
        ondragover={handleDragOver}
        ondragleave={handleDragLeave}
        ondrop={handleDrop}
      >
        <div class="upload-icon">&#128196;</div>
        <div class="upload-title">Cargar archivo .md o .txt</div>
        <div class="upload-desc">Arrastra o haz click para seleccionar un archivo.<br>Soporta archivos de hasta 10 MB.</div>
      </div>
    </div>
  {:else}
    <!-- Header -->
    <div class="doc-header">
      <div class="doc-info">
        <span class="doc-icon">&#128196;</span>
        <span class="doc-name">{fileName}</span>
        <span class="doc-meta">{fileLines.toLocaleString()} lineas</span>
        {#if phase !== 'idle'}
          <span class="phase-badge" class:analyzing={phase === 'analyzing'} class:review={phase === 'review' || phase === 'review_raw'} class:executing={phase === 'executing'} class:done={phase === 'done'}>
            {phase === 'analyzing' ? 'Analizando...' : phase === 'review' ? 'Revisar plan' : phase === 'review_raw' ? 'Sin tareas parseadas' : phase === 'executing' ? `Ejecutando ${completedCount}/${planSteps.length}` : `Listo (${completedCount} OK, ${errorCount} err)`}
          </span>
        {/if}
      </div>
      <div class="doc-actions">
        {#if phase === 'idle'}
          <input type="text" class="doc-search" placeholder="Buscar..." bind:value={searchQuery} />
          <button class="doc-btn" class:active={viewMode === 'preview'} onclick={() => { viewMode = 'preview'; }}>Preview</button>
          <button class="doc-btn" class:active={viewMode === 'raw'} onclick={() => { viewMode = 'raw'; }}>Raw</button>
          <button class="doc-btn plan-btn" onclick={startAnalysis} disabled={sending || fileLines === 0}>Ejecutar Plan</button>
        {:else if phase === 'review'}
          <button class="doc-btn plan-btn" onclick={executeSteps} disabled={planSteps.length === 0}>Aprobar y ejecutar ({planSteps.length})</button>
          <button class="doc-btn" onclick={resetAll}>Cancelar</button>
        {:else if phase === 'review_raw'}
          <button class="doc-btn" onclick={startAnalysis} disabled={sending}>Reintentar</button>
          <button class="doc-btn" onclick={resetAll}>Cancelar</button>
        {:else if phase === 'executing'}
          <button class="doc-btn close-btn" onclick={cancelExecution}>Detener</button>
        {:else if phase === 'done'}
          <button class="doc-btn" onclick={resetAll}>Nuevo analisis</button>
        {/if}
        <button class="doc-btn" onclick={triggerUpload} title="Cargar otro archivo">Abrir</button>
        <button class="doc-btn close-btn" onclick={clearFile} title="Cerrar">&#x2715;</button>
      </div>
    </div>

    <!-- PHASE: Analyzing -->
    {#if phase === 'analyzing'}
      <div class="doc-content">
        <div class="analyzing-msg">
          <div class="spinner"></div>
          <span>El agente esta analizando {fileName} y desglosando en tareas...</span>
        </div>
        {#if repoInfo}
          <div class="repo-status"><strong>Estado de repos:</strong><br>{repoInfo}</div>
        {/if}
      </div>

    <!-- PHASE: Review raw — agent returned no parseable TASK lines -->
    {:else if phase === 'review_raw'}
      <div class="doc-content">
        <div class="raw-analysis">
          <div class="raw-analysis-header">El agente no devolvio lineas TASK|target|descripcion parseables. Respuesta completa:</div>
          <pre class="raw-analysis-output">{analyzeOutput || '(sin respuesta)'}</pre>
        </div>
      </div>

    <!-- PHASE: Review steps -->
    {:else if phase === 'review'}
      <div class="doc-content">
        <div class="steps-list">
          {#each planSteps as step, i}
            <div class="step-row">
              <span class="step-num">{i + 1}</span>
              <span class="step-target" class:atlas={step.target === 'atlas'} class:pixel={step.target === 'pixel'}>{step.target.toUpperCase()}</span>
              <span class="step-desc">{step.description}</span>
              <div class="step-controls">
                <button class="step-btn" onclick={() => moveStep(i, -1)} disabled={i === 0} title="Subir">&#x2191;</button>
                <button class="step-btn" onclick={() => moveStep(i, 1)} disabled={i === planSteps.length - 1} title="Bajar">&#x2193;</button>
                <button class="step-btn del" onclick={() => removeStep(i)} title="Eliminar">&#x2715;</button>
              </div>
            </div>
          {/each}
        </div>
      </div>

    <!-- PHASE: Executing / Done -->
    {:else if phase === 'executing' || phase === 'done'}
      <div class="doc-content">
        <div class="steps-list">
          {#each planSteps as step, i}
            <div class="step-row" class:step-running={step.status === 'running'} class:step-done={step.status === 'done'} class:step-error={step.status === 'error'}>
              <span class="step-status">
                {#if step.status === 'pending'}&#x23F3;{:else if step.status === 'running'}<span class="spinner-sm"></span>{:else if step.status === 'done'}&#x2705;{:else}&#x274C;{/if}
              </span>
              <span class="step-num">{i + 1}</span>
              <span class="step-target" class:atlas={step.target === 'atlas'} class:pixel={step.target === 'pixel'}>{step.target.toUpperCase()}</span>
              <span class="step-desc">{step.description}</span>
              {#if step.output}
                <div class="step-output">{step.output}</div>
              {/if}
            </div>
          {/each}
        </div>
      </div>

    <!-- PHASE: Idle — show file content -->
    {:else if filteredLines}
      <div class="search-results">
        <div class="search-header">{filteredLines.length} resultados para "{debouncedQuery}"</div>
        <div class="search-list">
          {#each filteredLines as match}
            <div class="search-item">
              <span class="search-line-num">{match.num}</span>
              <span class="search-line-text">{match.text}</span>
            </div>
          {/each}
        </div>
      </div>
    {:else}
      <div class="doc-content">
        {#if fileLines === 0}
          <div class="empty-file">Archivo vacio. Usa "Abrir" para cargar otro.</div>
        {:else if viewMode === 'preview'}
          <div class="md-preview">{@html renderedPreview}</div>
        {:else}
          <div class="md-raw">
            {#each visibleLines as line, i}
              <div class="raw-line">
                <span class="line-num">{currentPage * PAGE_SIZE + i + 1}</span>
                <span class="line-text">{line}</span>
              </div>
            {/each}
          </div>
        {/if}
      </div>
      {#if totalPages > 1}
        <div class="pagination">
          <button class="page-btn" disabled={currentPage === 0} onclick={() => goPage(0)}>&#x226A;</button>
          <button class="page-btn" disabled={currentPage === 0} onclick={() => goPage(currentPage - 1)}>&#x2039;</button>
          <span class="page-info">Pagina {currentPage + 1} de {totalPages}</span>
          <button class="page-btn" disabled={currentPage >= totalPages - 1} onclick={() => goPage(currentPage + 1)}>&#x203A;</button>
          <button class="page-btn" disabled={currentPage >= totalPages - 1} onclick={() => goPage(totalPages - 1)}>&#x226B;</button>
        </div>
      {/if}
    {/if}
  {/if}
</div>

<style>
  .docs-tab {
    height: 100%;
    display: flex;
    flex-direction: column;
  }

  .empty-state {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 2rem;
  }
  .upload-area {
    border: 2px dashed var(--border);
    border-radius: 16px;
    padding: 3rem 4rem;
    text-align: center;
    cursor: pointer;
    transition: border-color 0.2s, background 0.2s;
    max-width: 480px;
  }
  .upload-area:hover, .upload-area:focus-visible {
    border-color: var(--cyan);
    background: rgba(0, 212, 255, 0.04);
  }
  .upload-area.dragging {
    border-color: var(--cyan);
    background: rgba(0, 212, 255, 0.08);
  }
  .upload-icon { font-size: 48px; margin-bottom: 12px; }
  .upload-title {
    font-size: 18px;
    font-weight: 600;
    color: var(--text-0);
    margin-bottom: 8px;
  }
  .upload-desc {
    font-size: 13px;
    color: var(--text-3);
    line-height: 1.6;
  }

  .doc-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 16px;
    background: var(--bg-2);
    border-bottom: 1px solid var(--border);
    gap: 12px;
    flex-wrap: wrap;
    flex-shrink: 0;
  }
  .doc-info {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .doc-icon { font-size: 18px; }
  .doc-name {
    font-weight: 600;
    font-size: 14px;
    color: var(--text-0);
  }
  .doc-meta {
    font-size: 11px;
    color: var(--text-3);
    font-family: var(--font-mono);
    background: var(--bg-1);
    padding: 2px 8px;
    border-radius: 10px;
  }
  .doc-actions {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .doc-search {
    background: var(--bg-1);
    border: 1px solid var(--border);
    color: var(--text-1);
    padding: 5px 10px;
    border-radius: var(--radius);
    font-size: 12px;
    width: 160px;
    font-family: var(--font-mono);
  }
  .doc-search:focus { border-color: var(--cyan); outline: none; }
  .doc-btn {
    background: var(--bg-1);
    color: var(--text-2);
    border: 1px solid var(--border);
    padding: 5px 12px;
    border-radius: var(--radius);
    font-size: 11px;
    font-weight: 600;
    cursor: pointer;
    text-transform: uppercase;
    letter-spacing: 0.3px;
    transition: all 0.15s;
    white-space: nowrap;
  }
  .doc-btn:hover { background: var(--bg-3); color: var(--text-0); border-color: var(--border-bright); }
  .doc-btn.active { color: var(--cyan); border-color: #00d4ff44; background: #00d4ff10; }
  .plan-btn { color: var(--green, #3fb950); border-color: rgba(63,185,80,0.3); }
  .plan-btn:hover:not(:disabled) { background: rgba(63,185,80,0.1); border-color: rgba(63,185,80,0.5); }
  .plan-btn:disabled { opacity: 0.5; cursor: not-allowed; }
  .close-btn:hover { color: var(--red, #f85149); border-color: rgba(248,81,73,0.3); }

  .empty-file {
    color: var(--text-3);
    font-size: 13px;
    text-align: center;
    padding: 2rem;
  }

  .search-results {
    flex: 1;
    overflow-y: auto;
    padding: 0;
  }
  .search-header {
    padding: 8px 16px;
    font-size: 12px;
    color: var(--text-3);
    border-bottom: 1px solid var(--border);
    background: var(--bg-2);
    position: sticky;
    top: 0;
  }
  .search-list { padding: 4px 0; }
  .search-item {
    display: flex;
    gap: 12px;
    padding: 4px 16px;
    font-size: 12px;
    font-family: var(--font-mono);
    border-bottom: 1px solid var(--border);
  }
  .search-item:hover { background: var(--bg-2); }
  .search-line-num {
    color: var(--text-3);
    min-width: 50px;
    text-align: right;
    flex-shrink: 0;
  }
  .search-line-text {
    color: var(--text-1);
    white-space: pre-wrap;
    word-break: break-all;
  }

  .doc-content {
    flex: 1 1 0;
    min-height: 0;
    overflow-y: auto;
    padding: 12px 16px;
  }

  .md-preview {
    font-size: 13px;
    line-height: 1.7;
    color: var(--text-1);
  }
  .md-preview :global(h2) {
    font-size: 1.3em;
    color: var(--cyan);
    margin: 1.5em 0 0.5em;
    padding-bottom: 0.3em;
    border-bottom: 1px solid var(--border);
  }
  .md-preview :global(h3) {
    font-size: 1.1em;
    color: var(--green, #3fb950);
    margin: 1.2em 0 0.4em;
  }
  .md-preview :global(h4) {
    font-size: 1em;
    color: var(--yellow, #d29922);
    margin: 1em 0 0.3em;
  }
  .md-preview :global(strong) { color: var(--text-0); }
  .md-preview :global(code) {
    background: var(--bg-1);
    padding: 1px 6px;
    border-radius: 4px;
    font-size: 0.9em;
    font-family: var(--font-mono);
  }
  .md-preview :global(pre) {
    background: var(--bg-1);
    padding: 12px 16px;
    border-radius: 8px;
    overflow-x: auto;
    margin: 0.8em 0;
  }
  .md-preview :global(pre code) {
    background: none;
    padding: 0;
    border-radius: 0;
    font-size: 12px;
    white-space: pre;
  }
  .md-preview :global(li) {
    margin-left: 1.2em;
    margin-bottom: 0.2em;
    list-style: disc;
  }
  .md-preview :global(hr) {
    border: none;
    border-top: 1px solid var(--border);
    margin: 1.5em 0;
  }

  .md-raw { font-family: var(--font-mono); font-size: 12px; }
  .raw-line {
    display: flex;
    gap: 16px;
    padding: 1px 0;
    line-height: 1.6;
  }
  .raw-line:hover { background: var(--bg-2); }
  .line-num {
    color: var(--text-3);
    min-width: 50px;
    text-align: right;
    flex-shrink: 0;
    user-select: none;
  }
  .line-text {
    color: var(--text-1);
    white-space: pre-wrap;
    word-break: break-all;
  }

  .pagination {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 10px;
    border-top: 1px solid var(--border);
    background: var(--bg-2);
    flex-shrink: 0;
  }
  .page-btn {
    background: var(--bg-1);
    color: var(--text-2);
    border: 1px solid var(--border);
    padding: 4px 10px;
    border-radius: var(--radius);
    cursor: pointer;
    font-size: 14px;
  }
  .page-btn:hover:not(:disabled) { background: var(--bg-3); color: var(--text-0); }
  .page-btn:disabled { opacity: 0.3; cursor: not-allowed; }
  .page-info {
    font-size: 12px;
    color: var(--text-2);
  }
  .page-range {
    color: var(--text-3);
    font-size: 11px;
  }

  /* Phase badge */
  .phase-badge {
    font-size: 10px;
    font-weight: 600;
    padding: 2px 8px;
    border-radius: 10px;
    text-transform: uppercase;
    letter-spacing: 0.3px;
  }
  .phase-badge.analyzing { background: rgba(0,212,255,0.15); color: var(--cyan); }
  .phase-badge.review { background: rgba(210,153,34,0.15); color: var(--yellow, #d29922); }
  .phase-badge.executing { background: rgba(88,166,255,0.15); color: var(--accent, #58a6ff); }
  .phase-badge.done { background: rgba(63,185,80,0.15); color: var(--green, #3fb950); }

  /* Analyzing state */
  .analyzing-msg {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px;
    color: var(--text-2);
    font-size: 13px;
  }
  .spinner {
    width: 16px; height: 16px;
    border: 2px solid var(--border);
    border-top-color: var(--cyan);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
  .spinner-sm {
    display: inline-block;
    width: 12px; height: 12px;
    border: 2px solid var(--border);
    border-top-color: var(--cyan);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin { to { transform: rotate(360deg); } }

  .repo-status {
    font-size: 11px;
    font-family: var(--font-mono);
    color: var(--text-3);
    padding: 8px 12px;
    background: var(--bg-1);
    border-radius: var(--radius);
    margin: 8px 12px;
    white-space: pre-wrap;
  }

  /* Raw analysis output (when no TASK lines parsed) */
  .raw-analysis { padding: 12px; }
  .raw-analysis-header {
    font-size: 13px;
    color: var(--text-2);
    margin-bottom: 8px;
    font-weight: 500;
  }
  .raw-analysis-output {
    font-size: 12px;
    font-family: var(--font-mono);
    color: var(--text-1);
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 12px;
    white-space: pre-wrap;
    word-break: break-word;
    max-height: 60vh;
    overflow-y: auto;
  }

  /* Steps list */
  .steps-list { padding: 4px 0; }
  .step-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 12px;
    border-bottom: 1px solid var(--border);
    font-size: 12px;
    transition: background 0.15s;
  }
  .step-row:hover { background: var(--bg-2); }
  .step-row.step-running { background: rgba(0,212,255,0.05); }
  .step-row.step-done { opacity: 0.7; }
  .step-row.step-error { background: rgba(248,81,73,0.05); }
  .step-num {
    color: var(--text-3);
    min-width: 20px;
    text-align: right;
    font-family: var(--font-mono);
    font-size: 11px;
  }
  .step-target {
    font-size: 9px;
    font-weight: 700;
    padding: 1px 6px;
    border-radius: 4px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    flex-shrink: 0;
  }
  .step-target.atlas { background: rgba(0,212,255,0.15); color: var(--cyan); }
  .step-target.pixel { background: rgba(168,85,247,0.15); color: #a855f7; }
  .step-desc { flex: 1; color: var(--text-1); line-height: 1.4; }
  .step-status { flex-shrink: 0; font-size: 14px; width: 20px; text-align: center; }
  .step-controls { display: flex; gap: 2px; flex-shrink: 0; }
  .step-btn {
    background: none;
    border: 1px solid var(--border);
    color: var(--text-3);
    padding: 1px 5px;
    border-radius: 3px;
    cursor: pointer;
    font-size: 10px;
  }
  .step-btn:hover { color: var(--text-0); border-color: var(--border-bright); }
  .step-btn.del:hover { color: var(--red, #f85149); }
  .step-btn:disabled { opacity: 0.3; cursor: not-allowed; }
  .step-output {
    width: 100%;
    font-size: 11px;
    font-family: var(--font-mono);
    color: var(--text-3);
    padding: 4px 8px;
    margin-top: 4px;
    background: var(--bg-1);
    border-radius: 4px;
    white-space: pre-wrap;
    max-height: 60px;
    overflow-y: auto;
  }

  .sr-only {
    position: absolute; width: 1px; height: 1px;
    overflow: hidden; clip: rect(0,0,0,0); border: 0;
  }
</style>
