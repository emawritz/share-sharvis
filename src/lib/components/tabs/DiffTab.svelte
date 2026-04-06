<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { handleError } from '../../utils';
  import { addToast } from '../../stores/notifications';
  import { t, tr } from '$lib/i18n';
  import { detectConflicts } from '$lib/api';
  import type { ConflictReport } from '$lib/types';

  interface DiffFile {
    path: string;
    status: string;
    additions: number;
    deletions: number;
    diffText: string;
  }

  interface DiffResult {
    machineId: string;
    repoName: string;
    branch: string;
    files: DiffFile[];
    totalAdditions: number;
    totalDeletions: number;
  }

  interface MachineConfig {
    id: string;
    name: string;
    enabled: boolean;
    repos: { name: string; path: string; github: string }[];
  }

  let machines = $state<MachineConfig[]>([]);
  let selectedMachine = $state('');
  let results = $state<DiffResult[]>([]);
  let loading = $state(false);
  let expandedFiles = $state<Set<string>>(new Set());
  let sidebarVisible = $state(true);

  // Conflict detection state
  let conflicts = $state<ConflictReport[]>([]);
  let conflictsChecked = $state(false);
  let conflictsLoading = $state(false);

  let totalFiles = $derived(results.reduce((s, r) => s + r.files.length, 0));
  let totalAdds = $derived(results.reduce((s, r) => s + r.totalAdditions, 0));
  let totalDels = $derived(results.reduce((s, r) => s + r.totalDeletions, 0));

  // Flat list of all files for sidebar
  interface SidebarFile {
    key: string;
    repoName: string;
    path: string;
    additions: number;
    deletions: number;
    status: string;
  }
  let sidebarFiles = $derived<SidebarFile[]>(
    results.flatMap(r =>
      r.files.map(f => ({
        key: r.repoName + ':' + f.path,
        repoName: r.repoName,
        path: f.path,
        additions: f.additions,
        deletions: f.deletions,
        status: f.status,
      }))
    )
  );

  // Build full diff text for copy/download
  let fullDiffText = $derived(
    results.flatMap(r => r.files.map(f => f.diffText)).join('\n')
  );

  async function loadMachines() {
    try {
      const config: { machines: MachineConfig[] } = await invoke('get_jarvis_config');
      machines = config.machines.filter(m => m.enabled && m.repos.length > 0);
      if (machines.length > 0 && !machines.find(m => m.id === selectedMachine)) {
        selectedMachine = machines[0].id;
      }
      if (selectedMachine) refresh();
    } catch (e) {
      addToast(t('common.error') + ': ' + handleError(e), 'error');
    }
  }

  async function refresh() {
    if (!selectedMachine) return;
    loading = true;
    expandedFiles = new Set();
    try {
      results = await invoke('get_git_diff', { machineId: selectedMachine });
    } catch (e) {
      addToast(t('diff.errorLoading') + ': ' + handleError(e), 'error');
      results = [];
    }
    loading = false;
  }

  function toggleFile(key: string) {
    const next = new Set(expandedFiles);
    if (next.has(key)) next.delete(key);
    else next.add(key);
    expandedFiles = next;
  }

  function statusBadge(status: string): { label: string; cls: string } {
    switch (status) {
      case 'modified': return { label: t('diff.modified'), cls: 'badge-modified' };
      case 'added': return { label: t('diff.added'), cls: 'badge-added' };
      case 'deleted': return { label: t('diff.deleted'), cls: 'badge-deleted' };
      case 'renamed': return { label: t('diff.renamed'), cls: 'badge-renamed' };
      default: return { label: '?', cls: 'badge-modified' };
    }
  }

  function lineClass(line: string): string {
    if (line.startsWith('diff ') || line.startsWith('index ')) return 'diff-meta';
    if (line.startsWith('+++') || line.startsWith('---')) return 'diff-meta';
    if (line.startsWith('@@')) return 'diff-hunk';
    if (line.startsWith('+')) return 'diff-add';
    if (line.startsWith('-')) return 'diff-del';
    return 'diff-ctx';
  }

  function scrollToFile(key: string) {
    const el = document.getElementById('diff-file-' + key);
    if (el) el.scrollIntoView({ behavior: 'smooth', block: 'start' });
    // Also expand if not expanded
    if (!expandedFiles.has(key)) toggleFile(key);
  }

  async function copyDiff() {
    try {
      await navigator.clipboard.writeText(fullDiffText);
      addToast('Diff copiado al portapapeles', 'success');
    } catch {
      addToast('No se pudo copiar el diff', 'error');
    }
  }

  function downloadPatch() {
    const date = new Date().toISOString().slice(0, 10);
    const blob = new Blob([fullDiffText], { type: 'text/plain' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `jarvis-diff-${date}.patch`;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
    addToast('Descargando patch...', 'info');
  }

  // Compute line numbers for a diff block
  // Returns array of { lineNum: string; cls: string; text: string }
  function parseDiffLines(diffText: string): { num: string; cls: string; text: string }[] {
    const lines = diffText.split('\n');
    let addNum = 0;
    let delNum = 0;
    let ctxNum = 0;

    return lines.map(line => {
      const cls = lineClass(line);
      let num = '';

      if (cls === 'diff-hunk') {
        // Parse @@ -a,b +c,d @@ to init counters
        const m = line.match(/^@@ -(\d+)(?:,\d+)? \+(\d+)/);
        if (m) {
          delNum = parseInt(m[1], 10);
          addNum = parseInt(m[2], 10);
          ctxNum = delNum;
        }
        num = '@@';
      } else if (cls === 'diff-add') {
        num = String(addNum++);
      } else if (cls === 'diff-del') {
        num = String(delNum++);
      } else if (cls === 'diff-ctx') {
        num = String(ctxNum++);
        addNum = ctxNum;
        delNum = ctxNum;
      } else {
        num = '';
      }

      return { num, cls, text: line };
    });
  }

  async function checkConflicts() {
    conflictsLoading = true;
    try {
      conflicts = await detectConflicts();
      conflictsChecked = true;
      if (conflicts.length > 0) {
        addToast(`${conflicts.length} conflict${conflicts.length !== 1 ? 's' : ''} detected across shared repos`, 'error');
      }
    } catch (e) {
      addToast('Conflict check failed: ' + handleError(e), 'error');
    }
    conflictsLoading = false;
  }

  onMount(() => {
    loadMachines();
    checkConflicts();
  });
</script>

<div class="diff-container" role="tabpanel">
  <!-- Toolbar -->
  <div class="toolbar">
    <div class="toolbar-left">
      <select bind:value={selectedMachine} onchange={refresh} class="machine-select">
        {#each machines as m}
          <option value={m.id}>{m.name}</option>
        {/each}
      </select>
      <button class="btn-refresh" onclick={refresh} disabled={loading}>
        {loading ? $tr('common.loading') : $tr('common.refresh')}
      </button>
      {#if sidebarFiles.length > 0}
        <button class="btn-sidebar" onclick={() => sidebarVisible = !sidebarVisible} title="Toggle file tree">
          {sidebarVisible ? '⊟' : '⊞'}
        </button>
      {/if}
    </div>
    <div class="toolbar-right">
      {#if results.length > 0}
        <div class="summary">
          <span class="summary-stat">
            <span class="sum-files">{totalFiles}</span> {totalFiles !== 1 ? $tr('diff.files') : $tr('diff.file')}
          </span>
          <span class="sum-add">+{totalAdds}</span>
          <span class="sum-sep">/</span>
          <span class="sum-del">-{totalDels}</span>
          <span class="sum-label">{$tr('diff.lines')}</span>
          {#each results as r}
            <span class="branch-chip">{r.repoName}: {r.branch || $tr('diff.noBranch')}</span>
          {/each}
        </div>
        <div class="action-btns">
          <button class="btn-action" onclick={copyDiff} title="Copiar diff completo">
            ⎘ Copiar diff
          </button>
          <button class="btn-action" onclick={downloadPatch} title="Descargar como .patch">
            ↓ .patch
          </button>
        </div>
      {/if}
    </div>
  </div>

  <!-- Conflict Detection Panel -->
  <div class="conflict-panel">
    <div class="conflict-header">
      <span class="conflict-title">CONFLICT CHECK</span>
      <button class="btn-conflicts" onclick={checkConflicts} disabled={conflictsLoading}>
        {conflictsLoading ? 'Checking...' : 'Check Conflicts'}
      </button>
    </div>
    {#if conflictsLoading}
      <div class="conflict-status loading">Scanning branches for overlapping changes...</div>
    {:else if conflictsChecked}
      {#if conflicts.length === 0}
        <div class="conflict-status ok">No conflicts detected</div>
      {:else}
        <div class="conflict-list">
          {#each conflicts as c}
            <div class="conflict-card">
              <div class="conflict-machines">
                <span class="machine-tag">{c.machineA}</span>
                <span class="conflict-vs">vs</span>
                <span class="machine-tag">{c.machineB}</span>
                <span class="conflict-repo">{c.repo}</span>
              </div>
              <div class="conflict-branches">
                <span class="branch-label">{c.branchA}</span>
                <span class="conflict-vs">↔</span>
                <span class="branch-label">{c.branchB}</span>
              </div>
              <div class="conflict-files">
                {#each c.overlappingFiles as file}
                  <span class="conflict-file">{file}</span>
                {/each}
              </div>
            </div>
          {/each}
        </div>
      {/if}
    {/if}
  </div>

  <!-- Main area: sidebar + content -->
  <div class="main-area">

    <!-- File tree sidebar -->
    {#if sidebarVisible && sidebarFiles.length > 0}
      <div class="file-sidebar">
        <div class="sidebar-header">FILES ({sidebarFiles.length})</div>
        <div class="sidebar-list">
          {#each sidebarFiles as sf}
            {@const badge = statusBadge(sf.status)}
            <button class="sidebar-item" onclick={() => scrollToFile(sf.key)} title={sf.path}>
              <span class="sidebar-badge {badge.cls}">{badge.label}</span>
              <span class="sidebar-path">{sf.path.split('/').pop()}</span>
              <span class="sidebar-adds">+{sf.additions}</span>
              <span class="sidebar-dels">-{sf.deletions}</span>
            </button>
          {/each}
        </div>
      </div>
    {/if}

    <!-- Content -->
    <div class="diff-content">
      {#if loading}
        <div class="empty-state">
          <span class="empty-icon">&#x25CC;</span>
          <div class="empty-title">{$tr('common.loading')}</div>
          <div class="empty-hint">Reading git diff...</div>
        </div>
      {:else if results.length === 0}
        <div class="empty-state">
          <span class="empty-icon">&#x2212;</span>
          <div class="empty-title">{$tr('diff.noChanges')}</div>
          <div class="empty-hint">Working tree is clean</div>
        </div>
      {:else}
        {#each results as result}
          {#if result.files.length === 0}
            <div class="repo-section">
              <div class="section-title">{result.repoName}</div>
              <div class="empty-state">
                <span class="empty-icon">&#x2212;</span>
                <div class="empty-title">{$tr('diff.noChanges')}</div>
                <div class="empty-hint">No changes in this repo</div>
              </div>
            </div>
          {:else}
            <div class="repo-section">
              <div class="section-title">{result.repoName}</div>
              {#each result.files as file}
                {@const key = result.repoName + ':' + file.path}
                {@const badge = statusBadge(file.status)}
                <div id="diff-file-{key}" class="file-anchor">
                  <button class="file-row" onclick={() => toggleFile(key)}>
                    <span class="status-badge {badge.cls}">{badge.label}</span>
                    <span class="file-path">{file.path}</span>
                    <span class="file-adds">+{file.additions}</span>
                    <span class="file-dels">-{file.deletions}</span>
                    <div class="diff-stat">
                      <div class="diff-stat-add" style="width: {(file.additions / (file.additions + file.deletions || 1)) * 100}%"></div>
                      <div class="diff-stat-del" style="width: {(file.deletions / (file.additions + file.deletions || 1)) * 100}%"></div>
                    </div>
                    <span class="expand-icon">{expandedFiles.has(key) ? '\u25BC' : '\u25B6'}</span>
                  </button>
                  {#if expandedFiles.has(key) && file.diffText}
                    <div class="diff-block">
                      <table class="diff-table" aria-label="diff for {file.path}">
                        <tbody>
                          {#each parseDiffLines(file.diffText) as row}
                            <tr class="diff-line-row {row.cls}">
                              <td class="diff-gutter">{row.num}</td>
                              <td class="diff-line-content">{row.text}</td>
                            </tr>
                          {/each}
                        </tbody>
                      </table>
                    </div>
                  {/if}
                </div>
              {/each}
            </div>
          {/if}
        {/each}
      {/if}
    </div>
  </div>
</div>

<style>
  .diff-container { display: flex; flex-direction: column; flex: 1; min-height: 0; overflow: hidden; }

  .toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 6px 14px;
    background: var(--bg-2);
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
    flex-wrap: wrap;
  }
  .toolbar-left { display: flex; align-items: center; gap: 8px; }
  .toolbar-right { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }

  .machine-select {
    background: var(--bg-1);
    color: var(--text-1);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 3px 8px;
    font-family: var(--font-display);
    font-size: 11px;
    cursor: pointer;
  }
  .machine-select:focus { outline: 1px solid var(--cyan); border-color: var(--cyan); }

  .btn-refresh {
    background: var(--bg-3);
    color: var(--text-1);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 3px 10px;
    font-family: var(--font-display);
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    cursor: pointer;
    transition: background 0.15s ease, border-color 0.15s ease;
  }
  .btn-refresh:hover:not(:disabled) { background: var(--cyan-dim); border-color: #00d4ff44; color: var(--cyan); }
  .btn-refresh:disabled { opacity: 0.5; cursor: default; }

  .btn-sidebar {
    background: var(--bg-3);
    color: var(--text-2);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 2px 7px;
    font-size: 12px;
    cursor: pointer;
    line-height: 1;
    transition: color 0.15s;
  }
  .btn-sidebar:hover { color: var(--cyan); border-color: #00d4ff44; }

  .summary {
    font-size: 10px;
    color: var(--text-2);
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }
  .summary-stat { color: var(--text-2); }
  .sum-files { color: var(--text-1); font-weight: 700; }
  .sum-add { color: var(--green); font-weight: 600; }
  .sum-del { color: var(--red); font-weight: 600; }
  .sum-sep { color: var(--text-3); }
  .sum-label { color: var(--text-3); }
  .branch-chip {
    background: var(--bg-3);
    border: 1px solid var(--border);
    border-radius: 3px;
    padding: 1px 6px;
    font-size: 9px;
    font-weight: 600;
    color: var(--cyan);
  }

  .action-btns { display: flex; gap: 5px; }
  .btn-action {
    background: var(--bg-3);
    color: var(--text-2);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 3px 9px;
    font-family: var(--font-display);
    font-size: 10px;
    font-weight: 600;
    cursor: pointer;
    transition: background 0.15s, border-color 0.15s, color 0.15s;
    letter-spacing: 0.3px;
  }
  .btn-action:hover { background: var(--cyan-dim); border-color: #00d4ff44; color: var(--cyan); }

  /* Main layout: sidebar + content */
  .main-area {
    display: flex;
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }

  /* File sidebar */
  .file-sidebar {
    width: 200px;
    flex-shrink: 0;
    background: var(--bg-2);
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .sidebar-header {
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 700;
    color: var(--text-3);
    text-transform: uppercase;
    letter-spacing: 1.5px;
    padding: 6px 10px 4px;
    flex-shrink: 0;
    border-bottom: 1px solid var(--border);
  }

  .sidebar-list {
    flex: 1;
    overflow-y: auto;
    padding: 4px 0;
  }
  .sidebar-list::-webkit-scrollbar { width: 3px; }
  .sidebar-list::-webkit-scrollbar-thumb { background: var(--border-bright); border-radius: 2px; }

  .sidebar-item {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 3px 8px;
    width: 100%;
    background: none;
    border: none;
    color: inherit;
    cursor: pointer;
    text-align: left;
    transition: background 0.1s;
    border-radius: 0;
  }
  .sidebar-item:hover { background: var(--bg-3); }

  .sidebar-badge {
    font-family: var(--font-mono);
    font-size: 8px;
    font-weight: 700;
    width: 13px;
    height: 13px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 2px;
    flex-shrink: 0;
  }

  .sidebar-path {
    font-family: var(--font-mono);
    font-size: 9px;
    color: var(--text-1);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
    min-width: 0;
  }

  .sidebar-adds { color: var(--green); font-size: 9px; font-weight: 600; flex-shrink: 0; }
  .sidebar-dels { color: var(--red); font-size: 9px; font-weight: 600; flex-shrink: 0; }

  .diff-content {
    flex: 1;
    overflow-y: auto;
    padding: 8px 14px;
    background: var(--bg-1);
    min-width: 0;
  }
  .diff-content::-webkit-scrollbar { width: 3px; }
  .diff-content::-webkit-scrollbar-thumb { background: var(--border-bright); border-radius: 2px; }

  .repo-section { margin-bottom: 12px; }

  .section-title {
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 700;
    color: var(--text-2);
    text-transform: uppercase;
    letter-spacing: 2px;
    margin-bottom: 4px;
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

  .file-anchor { scroll-margin-top: 8px; }

  .file-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 3px 4px;
    font-size: 11px;
    font-variant-numeric: tabular-nums;
    cursor: pointer;
    border-radius: 3px;
    transition: background 0.1s ease;
    background: none;
    border: none;
    color: inherit;
    width: 100%;
    text-align: left;
  }
  .file-row:hover { background: var(--bg-2); }

  .status-badge {
    font-family: var(--font-mono);
    font-size: 9px;
    font-weight: 700;
    width: 16px;
    height: 16px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 3px;
    flex-shrink: 0;
  }
  .badge-modified { background: var(--amber); color: var(--bg-0); }
  .badge-added { background: var(--green); color: var(--bg-0); }
  .badge-deleted { background: var(--red); color: var(--bg-0); }
  .badge-renamed { background: var(--cyan); color: var(--bg-0); }

  .file-path {
    font-family: var(--font-mono);
    color: var(--text-1);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
    min-width: 0;
    font-size: 10px;
  }

  .file-adds { color: var(--green); font-weight: 600; font-size: 10px; flex-shrink: 0; }
  .file-dels { color: var(--red); font-weight: 600; font-size: 10px; flex-shrink: 0; }

  .diff-stat {
    width: 40px;
    height: 4px;
    border-radius: 2px;
    background: var(--bg-3);
    overflow: hidden;
    flex-shrink: 0;
    display: flex;
  }
  .diff-stat-add { height: 100%; background: var(--green); }
  .diff-stat-del { height: 100%; background: var(--red); }

  .expand-icon {
    font-size: 8px;
    color: var(--text-3);
    flex-shrink: 0;
    width: 10px;
  }

  /* Diff block with line numbers */
  .diff-block {
    background: var(--bg-0);
    border: 1px solid var(--border);
    border-radius: 4px;
    margin: 2px 0 6px 24px;
    overflow-x: auto;
    font-family: var(--font-mono);
    font-size: 10px;
    line-height: 1.5;
  }

  .diff-table {
    border-collapse: collapse;
    width: 100%;
    min-width: max-content;
  }

  .diff-line-row {
    vertical-align: top;
  }

  .diff-gutter {
    user-select: none;
    text-align: right;
    padding: 0 6px 0 8px;
    min-width: 36px;
    color: var(--text-3);
    font-size: 9px;
    border-right: 1px solid var(--border);
    background: var(--bg-2);
    opacity: 0.8;
    white-space: nowrap;
    vertical-align: top;
    line-height: 1.5;
  }

  .diff-line-content {
    padding: 0 8px;
    white-space: pre;
    width: 100%;
  }

  /* Row-level coloring */
  .diff-line-row.diff-add { background: rgba(0, 200, 83, 0.12); }
  .diff-line-row.diff-add .diff-line-content { color: var(--green); }
  .diff-line-row.diff-add .diff-gutter { background: rgba(0, 200, 83, 0.08); color: var(--green); opacity: 0.7; }

  .diff-line-row.diff-del { background: rgba(255, 82, 82, 0.12); }
  .diff-line-row.diff-del .diff-line-content { color: var(--red); }
  .diff-line-row.diff-del .diff-gutter { background: rgba(255, 82, 82, 0.08); color: var(--red); opacity: 0.7; }

  .diff-line-row.diff-hunk { background: rgba(0, 100, 200, 0.1); }
  .diff-line-row.diff-hunk .diff-line-content { color: var(--cyan); font-weight: 700; }
  .diff-line-row.diff-hunk .diff-gutter { background: rgba(0, 100, 200, 0.08); }

  .diff-line-row.diff-meta .diff-line-content { color: var(--text-3); font-style: italic; }

  .diff-line-row.diff-ctx .diff-line-content { color: var(--text-2); }

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

  /* Conflict Detection Panel */
  .conflict-panel {
    flex-shrink: 0;
    background: var(--bg-2);
    border-bottom: 1px solid var(--border);
    padding: 6px 14px;
  }

  .conflict-header {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 4px;
  }

  .conflict-title {
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 700;
    color: var(--text-3);
    text-transform: uppercase;
    letter-spacing: 1.5px;
  }

  .btn-conflicts {
    background: var(--bg-3);
    color: var(--text-1);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 2px 8px;
    font-family: var(--font-display);
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    cursor: pointer;
    transition: background 0.15s ease, border-color 0.15s ease;
  }
  .btn-conflicts:hover:not(:disabled) { background: var(--amber-dim, rgba(255,180,0,0.12)); border-color: rgba(255,180,0,0.3); color: var(--amber, #ffb400); }
  .btn-conflicts:disabled { opacity: 0.5; cursor: default; }

  .conflict-status {
    font-size: 11px;
    padding: 3px 0;
  }
  .conflict-status.ok { color: var(--green); }
  .conflict-status.loading { color: var(--text-3); font-style: italic; }

  .conflict-list {
    display: flex;
    flex-direction: column;
    gap: 4px;
    max-height: 140px;
    overflow-y: auto;
  }
  .conflict-list::-webkit-scrollbar { width: 3px; }
  .conflict-list::-webkit-scrollbar-thumb { background: var(--border-bright); border-radius: 2px; }

  .conflict-card {
    background: rgba(255, 82, 82, 0.08);
    border: 1px solid rgba(255, 82, 82, 0.25);
    border-radius: 4px;
    padding: 5px 8px;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .conflict-machines {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
  }

  .machine-tag {
    background: var(--bg-0);
    border: 1px solid var(--border);
    border-radius: 3px;
    padding: 1px 5px;
    font-family: var(--font-display);
    font-size: 9px;
    font-weight: 700;
    color: var(--cyan);
    text-transform: uppercase;
  }

  .conflict-repo {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--text-2);
    margin-left: auto;
  }

  .conflict-vs {
    color: var(--text-3);
    font-size: 10px;
    font-weight: 600;
  }

  .conflict-branches {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .branch-label {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--amber, #ffb400);
  }

  .conflict-files {
    display: flex;
    flex-wrap: wrap;
    gap: 3px;
    margin-top: 2px;
  }

  .conflict-file {
    background: rgba(255,82,82,0.1);
    border: 1px solid rgba(255,82,82,0.2);
    border-radius: 3px;
    padding: 1px 5px;
    font-family: var(--font-mono);
    font-size: 9px;
    color: var(--red);
  }
</style>
