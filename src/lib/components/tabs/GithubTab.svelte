<script lang="ts">
  import { fetchGithubPRs, mergePR, getJarvisConfig, fetchChecks, createPR, getPrComments, addPrComment, getPrFiles } from '../../api';
  import type { PrComment } from '../../api';
  import { addToast } from '../../stores/notifications';
  import { handleError } from '../../utils';
  import type { PR, Check } from '../../types';
  import { t, tr } from '$lib/i18n';
  import ConfirmModal from '../ConfirmModal.svelte';
  import { open as shellOpen } from '@tauri-apps/plugin-shell';

  let repo = $state('');
  let prs = $state<PR[]>([]);
  let loading = $state(false);
  let availableRepos = $state<{name: string, github: string}[]>([]);
  let mergeMethod = $state<'squash' | 'merge' | 'rebase'>('squash');
  let showMergeConfirm = $state(false);
  let pendingMerge = $state<{id: number, title: string, method: string} | null>(null);
  let refreshTimer: ReturnType<typeof setTimeout> | null = null;

  // CI checks cache: branch -> Check[]
  let checksCache = $state<Record<string, Check[]>>({});
  let checksLoading = $state<Record<string, boolean>>({});

  // Nueva PR form
  let showNewPR = $state(false);
  let newPRTitle = $state('');
  let newPRBody = $state('');
  let newPRHead = $state('');
  let newPRBase = $state('main');
  let newPRLoading = $state(false);

  // Expanded PR panel state
  let expandedPR = $state<number | null>(null);

  // Comments state: prNumber -> PrComment[]
  let commentsCache = $state<Record<number, PrComment[]>>({});
  let commentsLoading = $state<Record<number, boolean>>({});
  let newCommentText = $state<Record<number, string>>({});
  let commentSubmitting = $state<Record<number, boolean>>({});

  // Files state: prNumber -> string[]
  let filesCache = $state<Record<number, string[]>>({});
  let filesLoading = $state<Record<number, boolean>>({});
  let filesExpanded = $state<Record<number, boolean>>({});

  // --- Helpers ---

  function getAuthorLogin(author: unknown): string {
    if (author && typeof author === 'object' && 'login' in author) {
      return (author as {login: string}).login;
    }
    return String(author ?? '');
  }

  function getAuthorInitials(author: unknown): string {
    const login = getAuthorLogin(author);
    return login.slice(0, 2).toUpperCase() || '??';
  }

  function relativeTime(dateStr: string): string {
    if (!dateStr) return '';
    const diff = Date.now() - new Date(dateStr).getTime();
    const mins = Math.floor(diff / 60000);
    if (mins < 60) return `hace ${mins}m`;
    const hrs = Math.floor(mins / 60);
    if (hrs < 24) return `hace ${hrs}h`;
    const days = Math.floor(hrs / 24);
    if (days < 30) return `hace ${days}d`;
    const months = Math.floor(days / 30);
    return `hace ${months}mo`;
  }

  function ciDotClass(branch: string): string {
    const checks = checksCache[branch];
    if (!checks || checks.length === 0) return 'ci-dot unknown';
    const failing = checks.some(c => c.conclusion === 'failure' || c.conclusion === 'cancelled');
    if (failing) return 'ci-dot failing';
    const pending = checks.some(c => c.status === 'in_progress' || c.status === 'queued' || (c.status === 'completed' && !c.conclusion));
    if (pending) return 'ci-dot pending';
    const allSuccess = checks.every(c => c.conclusion === 'success' || c.conclusion === 'skipped');
    if (allSuccess) return 'ci-dot passing';
    return 'ci-dot unknown';
  }

  function ciTooltip(branch: string): string {
    const checks = checksCache[branch];
    if (!checks || checks.length === 0) return 'Sin datos de CI';
    const failing = checks.filter(c => c.conclusion === 'failure');
    if (failing.length > 0) return `Fallando: ${failing.map(c => c.name).join(', ')}`;
    const pending = checks.filter(c => c.status !== 'completed');
    if (pending.length > 0) return `En progreso: ${pending.map(c => c.name).join(', ')}`;
    return `${checks.length} checks OK`;
  }

  // --- Data loading ---

  async function loadChecksForBranch(branch: string) {
    if (!repo || !branch || checksLoading[branch]) return;
    checksLoading = { ...checksLoading, [branch]: true };
    try {
      const result = await fetchChecks(repo, branch);
      checksCache = { ...checksCache, [branch]: result };
    } catch {
      // silently ignore check fetch errors
    }
    checksLoading = { ...checksLoading, [branch]: false };
  }

  async function refresh() {
    if (!repo) return;
    loading = true;
    checksCache = {};
    checksLoading = {};
    expandedPR = null;
    commentsCache = {};
    filesCache = {};
    filesExpanded = {};
    try {
      prs = await fetchGithubPRs(repo);
      // Kick off CI checks for each open PR branch
      for (const pr of prs) {
        if ((pr.state || '').toUpperCase() === 'OPEN' && pr.headRefName) {
          loadChecksForBranch(pr.headRefName);
        }
      }
    } catch (e) {
      addToast(t('github.errorLoadingPRs') + ': ' + handleError(e), 'error');
      prs = [];
    }
    loading = false;
  }

  async function loadRepos() {
    try {
      const config = await getJarvisConfig();
      const repos: {name: string, github: string}[] = [];
      const seen = new Set<string>();
      for (const m of config.machines) {
        for (const r of m.repos) {
          if (r.github && !seen.has(r.github)) {
            seen.add(r.github);
            repos.push({ name: r.name, github: r.github });
          }
        }
      }
      availableRepos = repos;
      if (repos.length > 0 && !repos.find(r => r.github === repo)) {
        repo = repos[0].github;
      }
      refresh();
    } catch {
      refresh();
    }
  }

  // --- Expand / Comments / Files ---

  async function toggleExpand(prNumber: number) {
    if (expandedPR === prNumber) {
      expandedPR = null;
      return;
    }
    expandedPR = prNumber;
    // Load comments if not cached
    if (!(prNumber in commentsCache)) {
      loadComments(prNumber);
    }
  }

  async function loadComments(prNumber: number) {
    if (!repo || commentsLoading[prNumber]) return;
    commentsLoading = { ...commentsLoading, [prNumber]: true };
    try {
      const result = await getPrComments(repo, prNumber);
      commentsCache = { ...commentsCache, [prNumber]: result };
    } catch (e) {
      addToast('Error al cargar comentarios: ' + handleError(e), 'error');
      commentsCache = { ...commentsCache, [prNumber]: [] };
    }
    commentsLoading = { ...commentsLoading, [prNumber]: false };
  }

  async function submitComment(prNumber: number) {
    const body = (newCommentText[prNumber] || '').trim();
    if (!body) return;
    commentSubmitting = { ...commentSubmitting, [prNumber]: true };
    try {
      await addPrComment(repo, prNumber, body);
      newCommentText = { ...newCommentText, [prNumber]: '' };
      addToast('Comentario enviado', 'success');
      // Reload comments
      commentsCache = { ...commentsCache };
      delete (commentsCache as Record<number, PrComment[]>)[prNumber];
      commentsCache = { ...commentsCache };
      loadComments(prNumber);
    } catch (e) {
      addToast('Error al enviar comentario: ' + handleError(e), 'error');
    }
    commentSubmitting = { ...commentSubmitting, [prNumber]: false };
  }

  async function toggleFiles(prNumber: number) {
    const nowExpanded = !filesExpanded[prNumber];
    filesExpanded = { ...filesExpanded, [prNumber]: nowExpanded };
    if (nowExpanded && !(prNumber in filesCache)) {
      await loadFiles(prNumber);
    }
  }

  async function loadFiles(prNumber: number) {
    if (!repo || filesLoading[prNumber]) return;
    filesLoading = { ...filesLoading, [prNumber]: true };
    try {
      const result = await getPrFiles(repo, prNumber);
      filesCache = { ...filesCache, [prNumber]: result };
    } catch (e) {
      addToast('Error al cargar archivos: ' + handleError(e), 'error');
      filesCache = { ...filesCache, [prNumber]: [] };
    }
    filesLoading = { ...filesLoading, [prNumber]: false };
  }

  // --- Merge ---

  function handleMerge(prNumber: number, prTitle: string) {
    pendingMerge = { id: prNumber, title: prTitle, method: mergeMethod };
    showMergeConfirm = true;
  }

  async function confirmMerge() {
    if (!pendingMerge) return;
    const { id, method } = pendingMerge;
    showMergeConfirm = false;
    pendingMerge = null;
    try {
      const ok = await mergePR(repo, id, method as 'squash' | 'merge' | 'rebase');
      if (ok) {
        addToast(t('github.merged', { id, method }), 'success');
        refreshTimer = setTimeout(refresh, 1000);
      } else {
        addToast(t('github.mergeFailed'), 'error');
      }
    } catch (e) {
      addToast('Error: ' + handleError(e), 'error');
    }
  }

  function cancelMerge() {
    showMergeConfirm = false;
    pendingMerge = null;
  }

  function handleRepoChange() {
    refresh();
  }

  // --- PR actions ---

  async function openInBrowser(prNumber: number) {
    if (!repo) return;
    const url = `https://github.com/${repo}/pull/${prNumber}`;
    try {
      await shellOpen(url);
    } catch (e) {
      addToast('No se pudo abrir el navegador: ' + handleError(e), 'error');
    }
  }

  async function copyUrl(prNumber: number) {
    const url = `https://github.com/${repo}/pull/${prNumber}`;
    try {
      await navigator.clipboard.writeText(url);
      addToast('URL copiada al portapapeles', 'success');
    } catch {
      addToast('No se pudo copiar: ' + url, 'info');
    }
  }

  function approvePrompt() {
    addToast('Aprobación requiere gh CLI: gh pr review --approve <numero>', 'info');
  }

  // --- Nueva PR ---

  async function submitNewPR() {
    if (!newPRTitle.trim()) {
      addToast('El título es obligatorio', 'error');
      return;
    }
    if (!repo) {
      addToast('Selecciona un repositorio', 'error');
      return;
    }
    newPRLoading = true;
    try {
      const result = await createPR(repo, newPRTitle.trim(), newPRBody.trim(), newPRHead.trim());
      if (result) {
        addToast('PR creada: ' + result, 'success');
        showNewPR = false;
        newPRTitle = '';
        newPRBody = '';
        newPRHead = '';
        newPRBase = 'main';
        refreshTimer = setTimeout(refresh, 1500);
      } else {
        addToast('gh pr create no devolvió URL. Verifica que la rama existe y tienes permisos.', 'error');
      }
    } catch (e) {
      addToast('Error al crear PR: ' + handleError(e), 'error');
    }
    newPRLoading = false;
  }

  $effect(() => {
    loadRepos();
    return () => { if (refreshTimer) { clearTimeout(refreshTimer); refreshTimer = null; } };
  });
</script>

<div class="github-panel">
  <div class="gh-header">
    <div class="gh-section-label">{$tr('github.pullRequests')}</div>
    <select class="gh-repo-select" bind:value={repo} onchange={handleRepoChange}>
      {#each availableRepos as r}
        <option value={r.github}>{r.name}</option>
      {/each}
    </select>
    <button class="new-pr-btn" type="button" onclick={() => showNewPR = !showNewPR}>
      {showNewPR ? '✕ Cancelar' : '+ Nueva PR'}
    </button>
    <button class="refresh-btn" type="button" onclick={refresh} title="Actualizar">&#x21BB;</button>
  </div>

  {#if showNewPR}
    <div class="new-pr-form">
      <div class="new-pr-form-title">Nueva Pull Request</div>
      <input
        class="new-pr-input"
        type="text"
        placeholder="Título de la PR *"
        bind:value={newPRTitle}
      />
      <div class="new-pr-row">
        <input
          class="new-pr-input"
          type="text"
          placeholder="Rama origen (head)"
          bind:value={newPRHead}
        />
        <select class="new-pr-select" bind:value={newPRBase}>
          <option value="main">main</option>
          <option value="master">master</option>
          <option value="develop">develop</option>
        </select>
      </div>
      <textarea
        class="new-pr-textarea"
        placeholder="Descripción (opcional)"
        bind:value={newPRBody}
        rows={3}
      ></textarea>
      <div class="new-pr-actions">
        <button class="pr-create-btn" type="button" onclick={submitNewPR} disabled={newPRLoading}>
          {newPRLoading ? 'Creando...' : 'Crear PR'}
        </button>
        <span class="new-pr-hint">Base: <strong>{newPRBase}</strong></span>
      </div>
    </div>
  {/if}

  {#if loading}
    <!-- Loading skeleton -->
    <div class="skeleton-list">
      {#each [1, 2, 3] as _}
        <div class="skeleton-card">
          <div class="skeleton-row">
            <div class="skeleton-pill w40"></div>
            <div class="skeleton-line w200"></div>
            <div class="skeleton-pill w50"></div>
          </div>
          <div class="skeleton-row">
            <div class="skeleton-line w120"></div>
            <div class="skeleton-line w80"></div>
            <div class="skeleton-line w60"></div>
          </div>
        </div>
      {/each}
    </div>
  {:else if prs.length === 0}
    <div class="empty-state">
      <span class="empty-icon">&#x2A2F;</span>
      <div class="empty-title">{$tr('github.noPRs')}</div>
      <div class="empty-hint">No open pull requests for this repo</div>
    </div>
  {:else}
    {#each prs as pr}
      <div class="pr-card" class:expanded={expandedPR === pr.number}>
        <div class="pr-main-row">
          <span class="pr-number">#{pr.number || '?'}</span>
          {#if (pr.state || '').toUpperCase() === 'OPEN' && pr.headRefName}
            <span
              class={ciDotClass(pr.headRefName)}
              title={ciTooltip(pr.headRefName)}
            ></span>
          {/if}
          <button class="pr-title-btn" type="button" onclick={() => toggleExpand(pr.number)} title="Ver comentarios y detalles">
            <span class="pr-title">{pr.title}</span>
            <span class="pr-expand-icon">{expandedPR === pr.number ? '▲' : '▼'}</span>
          </button>
          <span class="pr-state {(pr.state || 'open').toLowerCase()}">{pr.state || $tr('github.open')}</span>
          {#if (pr.state || '').toUpperCase() === 'OPEN'}
            <select class="pr-merge-method" bind:value={mergeMethod}>
              <option value="squash">squash</option>
              <option value="merge">merge</option>
              <option value="rebase">rebase</option>
            </select>
            <button class="pr-merge-btn" type="button" onclick={() => handleMerge(pr.number, pr.title)}>{$tr('github.merge')}</button>
          {/if}
        </div>

        <div class="pr-details">
          <span class="pr-branch" title={pr.headRefName}>{pr.headRefName}</span>
          {#if pr.author}
            <span class="pr-avatar" title={getAuthorLogin(pr.author)}>
              {getAuthorInitials(pr.author)}
            </span>
            <span class="pr-author">{getAuthorLogin(pr.author)}</span>
          {/if}
          <span class="pr-additions">+{pr.additions ?? 0}</span>
          <span class="pr-deletions">-{pr.deletions ?? 0}</span>
          <!-- Files Changed chip -->
          <button
            class="files-chip"
            type="button"
            onclick={() => toggleFiles(pr.number)}
            title="Ver archivos cambiados"
          >
            {#if filesLoading[pr.number]}
              <span class="chip-spin">&#x25CC;</span> Archivos...
            {:else if filesCache[pr.number] !== undefined}
              &#x1F4C4; Archivos ({filesCache[pr.number].length}) {filesExpanded[pr.number] ? '▲' : '▼'}
            {:else}
              &#x1F4C4; Files
            {/if}
          </button>
          {#if pr.createdAt}
            <span class="pr-time">{relativeTime(pr.createdAt)}</span>
          {/if}
        </div>

        <!-- Files Changed panel -->
        {#if filesExpanded[pr.number] && filesCache[pr.number] !== undefined}
          <div class="files-panel">
            {#if filesCache[pr.number].length === 0}
              <span class="files-empty">Sin archivos</span>
            {:else}
              {#each filesCache[pr.number] as file}
                <div class="file-entry">{file}</div>
              {/each}
            {/if}
          </div>
        {/if}

        <div class="pr-actions-row">
          <button class="pr-action-btn approve" type="button" onclick={approvePrompt} title="Aprobar PR">
            &#x2714; Aprobar
          </button>
          <button class="pr-action-btn open-browser" type="button" onclick={() => openInBrowser(pr.number)} title="Abrir en navegador">
            &#x2197; Abrir
          </button>
          <button class="pr-action-btn copy-url" type="button" onclick={() => copyUrl(pr.number)} title="Copiar URL">
            &#x29C9; Copiar URL
          </button>
        </div>

        <!-- Expanded: Comments section -->
        {#if expandedPR === pr.number}
          <div class="comments-section">
            <div class="comments-header">
              {#if commentsLoading[pr.number]}
                <span class="comments-label">Comentarios...</span>
              {:else}
                <span class="comments-label">Comentarios ({commentsCache[pr.number]?.length ?? 0})</span>
              {/if}
            </div>

            {#if commentsLoading[pr.number]}
              <div class="comments-loading">
                <span class="chip-spin">&#x25CC;</span> Cargando comentarios...
              </div>
            {:else if commentsCache[pr.number]?.length}
              <div class="comments-list">
                {#each commentsCache[pr.number] as comment}
                  <div class="comment-item">
                    <div class="comment-meta">
                      <span class="comment-author">{comment.author}</span>
                      <span class="comment-time">{relativeTime(comment.createdAt)}</span>
                    </div>
                    <div class="comment-body">{comment.body}</div>
                  </div>
                {/each}
              </div>
            {:else}
              <div class="comments-empty">Sin comentarios aún</div>
            {/if}

            <!-- Add comment -->
            <div class="add-comment">
              <textarea
                class="comment-textarea"
                placeholder="Añadir comentario..."
                rows={2}
                value={newCommentText[pr.number] ?? ''}
                oninput={(e) => { newCommentText = { ...newCommentText, [pr.number]: (e.currentTarget as HTMLTextAreaElement).value }; }}
              ></textarea>
              <button
                class="comment-submit-btn"
                type="button"
                onclick={() => submitComment(pr.number)}
                disabled={commentSubmitting[pr.number] || !(newCommentText[pr.number] ?? '').trim()}
              >
                {commentSubmitting[pr.number] ? 'Enviando...' : 'Enviar'}
              </button>
            </div>
          </div>
        {/if}
      </div>
    {/each}
  {/if}
</div>

<ConfirmModal
  open={showMergeConfirm}
  title={$tr('github.confirmMerge')}
  message={pendingMerge ? `PR #${pendingMerge.id} — ${pendingMerge.title} (${pendingMerge.method})` : ''}
  confirmText={$tr('github.merge')}
  cancelText={$tr('common.cancel')}
  onConfirm={confirmMerge}
  onCancel={cancelMerge}
/>

<style>
  .github-panel {
    padding: 8px 14px;
    overflow-y: auto;
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .gh-header {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .gh-section-label {
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 700;
    color: var(--text-2);
    text-transform: uppercase;
    letter-spacing: 2px;
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
  }
  .gh-section-label::before {
    content: '';
    width: 3px; height: 3px;
    border-radius: 50%;
    background: var(--text-2);
  }
  .gh-repo-select {
    background: var(--bg-1);
    color: var(--text-0);
    border: 1px solid var(--border);
    padding: 4px 8px;
    border-radius: var(--radius);
    font-family: var(--font-display);
    font-size: 10px;
    cursor: pointer;
    margin-left: auto;
    -webkit-appearance: none;
    appearance: none;
  }
  .new-pr-btn {
    background: var(--bg-2);
    color: var(--cyan);
    border: 1px solid var(--border);
    padding: 3px 10px;
    border-radius: var(--radius);
    font-family: var(--font-display);
    font-size: 8px;
    font-weight: 700;
    cursor: pointer;
    letter-spacing: 0.5px;
    text-transform: uppercase;
    flex-shrink: 0;
    transition: background 0.15s ease;
  }
  .new-pr-btn:hover { background: var(--bg-1); }
  .refresh-btn {
    background: transparent;
    color: var(--text-3);
    border: none;
    font-size: 14px;
    cursor: pointer;
    padding: 0 2px;
    flex-shrink: 0;
    transition: color 0.15s ease;
  }
  .refresh-btn:hover { color: var(--text-1); }

  /* Nueva PR form */
  .new-pr-form {
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 10px 12px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .new-pr-form-title {
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 700;
    color: var(--cyan);
    text-transform: uppercase;
    letter-spacing: 1px;
    margin-bottom: 2px;
  }
  .new-pr-input {
    background: var(--bg-1);
    color: var(--text-0);
    border: 1px solid var(--border);
    padding: 5px 8px;
    border-radius: var(--radius);
    font-size: 11px;
    font-family: var(--font-display);
    outline: none;
    flex: 1;
  }
  .new-pr-input:focus { border-color: var(--cyan); }
  .new-pr-row {
    display: flex;
    gap: 6px;
  }
  .new-pr-select {
    background: var(--bg-1);
    color: var(--text-1);
    border: 1px solid var(--border);
    padding: 5px 8px;
    border-radius: var(--radius);
    font-family: var(--font-display);
    font-size: 10px;
    cursor: pointer;
    -webkit-appearance: none;
    appearance: none;
    flex-shrink: 0;
    width: 90px;
  }
  .new-pr-textarea {
    background: var(--bg-1);
    color: var(--text-0);
    border: 1px solid var(--border);
    padding: 5px 8px;
    border-radius: var(--radius);
    font-size: 11px;
    font-family: var(--font-display);
    resize: vertical;
    outline: none;
    min-height: 50px;
  }
  .new-pr-textarea:focus { border-color: var(--cyan); }
  .new-pr-actions {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .pr-create-btn {
    background: var(--green-dim);
    color: var(--green);
    border: 1px solid #00ff8844;
    padding: 4px 14px;
    border-radius: var(--radius);
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 700;
    cursor: pointer;
    letter-spacing: 0.5px;
    text-transform: uppercase;
    transition: background 0.15s ease;
  }
  .pr-create-btn:hover:not(:disabled) { background: #00ff8833; }
  .pr-create-btn:disabled { opacity: 0.5; cursor: not-allowed; }
  .new-pr-hint {
    font-size: 9px;
    color: var(--text-3);
  }

  /* Loading skeleton */
  .skeleton-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .skeleton-card {
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 10px 12px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    animation: skeleton-pulse 1.4s ease-in-out infinite;
  }
  .skeleton-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .skeleton-line, .skeleton-pill {
    background: var(--border);
    border-radius: 3px;
    height: 10px;
    flex-shrink: 0;
  }
  .skeleton-pill { height: 14px; border-radius: 7px; }
  .w40 { width: 40px; }
  .w50 { width: 50px; }
  .w60 { width: 60px; }
  .w80 { width: 80px; }
  .w120 { width: 120px; }
  .w200 { width: 200px; flex: 1; max-width: 200px; }
  @keyframes skeleton-pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.45; }
  }

  /* PR cards */
  .pr-card {
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 8px 12px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    transition: border-color 0.15s ease;
  }
  .pr-card:hover { border-color: var(--border-bright); }
  .pr-card.expanded { border-color: var(--cyan); }
  .pr-main-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .pr-title-btn {
    display: flex;
    align-items: center;
    gap: 4px;
    background: none;
    border: none;
    padding: 0;
    cursor: pointer;
    flex: 1;
    min-width: 0;
    text-align: left;
  }
  .pr-expand-icon {
    font-size: 7px;
    color: var(--text-3);
    flex-shrink: 0;
  }
  .pr-details {
    display: flex;
    align-items: center;
    gap: 8px;
    padding-left: 2px;
  }
  .pr-actions-row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding-left: 2px;
    margin-top: 2px;
  }
  .pr-branch {
    font-family: var(--font-mono, monospace);
    font-size: 9px;
    color: var(--text-2);
    background: var(--bg-1);
    padding: 1px 5px;
    border-radius: 3px;
    max-width: 160px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .pr-avatar {
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: var(--bg-1);
    border: 1px solid var(--border);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-size: 7px;
    font-weight: 700;
    color: var(--text-2);
    letter-spacing: 0;
    flex-shrink: 0;
  }
  .pr-author {
    font-size: 9px;
    color: var(--text-3);
  }
  .pr-additions {
    font-family: var(--font-mono, monospace);
    font-size: 9px;
    color: var(--green);
  }
  .pr-deletions {
    font-family: var(--font-mono, monospace);
    font-size: 9px;
    color: var(--red);
  }
  .pr-time {
    font-size: 9px;
    color: var(--text-3);
    margin-left: auto;
  }
  .pr-merge-method {
    background: var(--bg-1);
    color: var(--text-2);
    border: 1px solid var(--border);
    padding: 2px 4px;
    border-radius: var(--radius);
    font-family: var(--font-display);
    font-size: 8px;
    cursor: pointer;
    -webkit-appearance: none;
    appearance: none;
    flex-shrink: 0;
  }
  .pr-number {
    font-family: var(--font-display);
    font-weight: 700;
    font-size: 11px;
    color: var(--cyan);
    flex-shrink: 0;
  }
  .pr-title {
    font-size: 11px;
    color: var(--text-0);
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .pr-state {
    font-family: var(--font-display);
    font-size: 8px;
    font-weight: 700;
    padding: 1px 6px;
    border-radius: 3px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    flex-shrink: 0;
  }
  .pr-state.open { background: var(--green-dim); color: var(--green); border: 1px solid #00ff8833; }
  .pr-state.closed { background: #ff335518; color: var(--red); border: 1px solid #ff335533; }
  .pr-state.merged { background: #6c3baa18; color: #c084fc; border: 1px solid #6c3baa33; }
  .pr-merge-btn {
    background: var(--green-dim);
    color: var(--green);
    border: 1px solid #00ff8844;
    padding: 3px 10px;
    border-radius: var(--radius);
    font-family: var(--font-display);
    font-size: 8px;
    font-weight: 700;
    cursor: pointer;
    letter-spacing: 0.5px;
    text-transform: uppercase;
    transition: background 0.15s ease;
    flex-shrink: 0;
  }
  .pr-merge-btn:hover { background: #00ff8833; }

  /* CI status dot */
  .ci-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    flex-shrink: 0;
    cursor: help;
  }
  .ci-dot.passing { background: var(--green); box-shadow: 0 0 4px var(--green); }
  .ci-dot.failing { background: var(--red); box-shadow: 0 0 4px var(--red); }
  .ci-dot.pending { background: #f59e0b; box-shadow: 0 0 4px #f59e0b; animation: ci-pulse 1.5s ease-in-out infinite; }
  .ci-dot.unknown { background: var(--text-3); }

  @keyframes ci-pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.4; }
  }

  /* Files chip */
  .files-chip {
    font-family: var(--font-display);
    font-size: 8px;
    font-weight: 700;
    padding: 2px 7px;
    border-radius: 10px;
    cursor: pointer;
    background: var(--bg-1);
    color: var(--text-2);
    border: 1px solid var(--border);
    transition: border-color 0.15s ease, color 0.15s ease;
    flex-shrink: 0;
    letter-spacing: 0.3px;
  }
  .files-chip:hover { border-color: var(--border-bright); color: var(--text-1); }

  /* Files panel */
  .files-panel {
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 6px 8px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: 140px;
    overflow-y: auto;
  }
  .file-entry {
    font-family: var(--font-mono, monospace);
    font-size: 9px;
    color: var(--text-1);
    padding: 1px 0;
    border-bottom: 1px solid var(--border);
  }
  .file-entry:last-child { border-bottom: none; }
  .files-empty {
    font-size: 9px;
    color: var(--text-3);
  }

  /* Action buttons */
  .pr-action-btn {
    font-family: var(--font-display);
    font-size: 8px;
    font-weight: 700;
    padding: 2px 8px;
    border-radius: var(--radius);
    cursor: pointer;
    letter-spacing: 0.3px;
    text-transform: uppercase;
    transition: background 0.15s ease;
    flex-shrink: 0;
  }
  .pr-action-btn.approve {
    background: var(--green-dim);
    color: var(--green);
    border: 1px solid #00ff8833;
  }
  .pr-action-btn.approve:hover { background: #00ff8822; }
  .pr-action-btn.open-browser {
    background: var(--bg-1);
    color: var(--cyan);
    border: 1px solid var(--border);
  }
  .pr-action-btn.open-browser:hover { border-color: var(--cyan); }
  .pr-action-btn.copy-url {
    background: var(--bg-1);
    color: var(--text-2);
    border: 1px solid var(--border);
  }
  .pr-action-btn.copy-url:hover { color: var(--text-1); border-color: var(--border-bright); }

  /* Comments section */
  .comments-section {
    margin-top: 4px;
    border-top: 1px solid var(--border);
    padding-top: 8px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .comments-header {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .comments-label {
    font-family: var(--font-display);
    font-size: 8px;
    font-weight: 700;
    color: var(--cyan);
    text-transform: uppercase;
    letter-spacing: 1px;
  }
  .comments-loading {
    font-size: 9px;
    color: var(--text-3);
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .comments-empty {
    font-size: 9px;
    color: var(--text-3);
    padding: 4px 0;
  }
  .comments-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
    max-height: 180px;
    overflow-y: auto;
  }
  .comment-item {
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 6px 8px;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .comment-meta {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .comment-author {
    font-family: var(--font-display);
    font-size: 8px;
    font-weight: 700;
    color: var(--cyan);
  }
  .comment-time {
    font-size: 8px;
    color: var(--text-3);
  }
  .comment-body {
    font-size: 10px;
    color: var(--text-1);
    white-space: pre-wrap;
    word-break: break-word;
    line-height: 1.4;
  }

  /* Add comment */
  .add-comment {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .comment-textarea {
    background: var(--bg-1);
    color: var(--text-0);
    border: 1px solid var(--border);
    padding: 5px 8px;
    border-radius: var(--radius);
    font-size: 10px;
    font-family: var(--font-display);
    resize: vertical;
    outline: none;
    min-height: 40px;
    width: 100%;
    box-sizing: border-box;
  }
  .comment-textarea:focus { border-color: var(--cyan); }
  .comment-submit-btn {
    align-self: flex-end;
    background: var(--bg-1);
    color: var(--cyan);
    border: 1px solid var(--border);
    padding: 3px 12px;
    border-radius: var(--radius);
    font-family: var(--font-display);
    font-size: 8px;
    font-weight: 700;
    cursor: pointer;
    letter-spacing: 0.5px;
    text-transform: uppercase;
    transition: border-color 0.15s ease, background 0.15s ease;
  }
  .comment-submit-btn:hover:not(:disabled) { border-color: var(--cyan); background: var(--bg-2); }
  .comment-submit-btn:disabled { opacity: 0.4; cursor: not-allowed; }

  /* Spinner */
  .chip-spin {
    display: inline-block;
    animation: spin 1s linear infinite;
  }
  @keyframes spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }

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
