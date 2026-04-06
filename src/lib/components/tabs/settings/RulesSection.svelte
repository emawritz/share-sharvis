<script lang="ts">
  import { onMount } from 'svelte';
  import { getRules, saveRule, deleteRule, toggleRule, reorderRules, dryRunRule, getRuleHistory } from '../../../api';
  import { addToast } from '../../../stores/notifications';
  import { handleError } from '../../../utils';
  import { t, tr } from '$lib/i18n';
  import ConfirmModal from '../../ConfirmModal.svelte';
  import type { AutoRule, RuleFireEvent } from '../../../types';

  let rules = $state<AutoRule[]>([]);
  let loadingRules = $state(false);
  let addingRule = $state(false);
  let showDeleteConfirm = $state(false);
  let pendingDeleteRule = $state<string | null>(null);
  let searchQuery = $state('');
  let newRule = $state<AutoRule>({
    id: '', name: '', trigger: 'on_task_complete',
    condition: undefined,
    action: { actionType: 'alert', message: '' },
    enabled: true, fireCount: 0, priority: 0
  });

  // Drag-and-drop state
  let dragSourceId = $state<string | null>(null);
  let dragOverId = $state<string | null>(null);

  // Dry run state
  let dryRunRuleId = $state<string | null>(null);
  let dryRunPrompt = $state('');
  let dryRunLoading = $state(false);
  let dryRunResult = $state<boolean | null>(null);

  // Rule history state
  let historyRuleId = $state<string | null>(null);
  let historyEvents = $state<RuleFireEvent[]>([]);
  let historyLoading = $state(false);

  let filteredRules = $derived(
    searchQuery.trim()
      ? rules.filter(r =>
          r.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
          (r.condition?.value ?? '').toLowerCase().includes(searchQuery.toLowerCase()) ||
          r.trigger.toLowerCase().includes(searchQuery.toLowerCase())
        )
      : rules
  );

  onMount(() => {
    loadRules();
  });

  async function loadRules() {
    loadingRules = true;
    try { rules = await getRules(); } catch { rules = []; }
    loadingRules = false;
  }

  async function handleToggleRule(id: string, enabled: boolean) {
    try {
      await toggleRule(id, enabled);
      await loadRules();
    } catch (e) { addToast('Error: ' + handleError(e), 'error'); }
  }

  function requestDeleteRule(id: string) {
    pendingDeleteRule = id;
    showDeleteConfirm = true;
  }

  async function confirmDeleteRule() {
    if (!pendingDeleteRule) return;
    showDeleteConfirm = false;
    const id = pendingDeleteRule;
    pendingDeleteRule = null;
    try {
      await deleteRule(id);
      await loadRules();
      addToast(t('rules.deleted'), 'success');
    } catch (e) { addToast('Error: ' + handleError(e), 'error'); }
  }

  function cancelDeleteRule() {
    showDeleteConfirm = false;
    pendingDeleteRule = null;
  }

  async function handleSaveNewRule() {
    if (!newRule.name.trim()) { addToast(t('rules.enterName'), 'error'); return; }
    newRule.id = crypto.randomUUID();
    newRule.priority = rules.length;
    try {
      await saveRule(newRule);
      await loadRules();
      addingRule = false;
      newRule = { id: '', name: '', trigger: 'on_task_complete', condition: undefined, action: { actionType: 'alert', message: '' }, enabled: true, fireCount: 0, priority: 0 };
      addToast(t('rules.created'), 'success');
    } catch (e) { addToast('Error: ' + handleError(e), 'error'); }
  }

  // --- Drag-and-drop handlers ---

  function handleDragStart(e: DragEvent, ruleId: string) {
    dragSourceId = ruleId;
    if (e.dataTransfer) {
      e.dataTransfer.effectAllowed = 'move';
      e.dataTransfer.setData('text/plain', ruleId);
    }
  }

  function handleDragOver(e: DragEvent, ruleId: string) {
    e.preventDefault();
    if (e.dataTransfer) e.dataTransfer.dropEffect = 'move';
    dragOverId = ruleId;
  }

  function handleDragLeave() {
    dragOverId = null;
  }

  async function handleDrop(e: DragEvent, targetId: string) {
    e.preventDefault();
    dragOverId = null;
    if (!dragSourceId || dragSourceId === targetId) { dragSourceId = null; return; }

    const sourceIdx = rules.findIndex(r => r.id === dragSourceId);
    const targetIdx = rules.findIndex(r => r.id === targetId);
    if (sourceIdx === -1 || targetIdx === -1) { dragSourceId = null; return; }

    const reordered = [...rules];
    const [moved] = reordered.splice(sourceIdx, 1);
    reordered.splice(targetIdx, 0, moved);
    rules = reordered;
    dragSourceId = null;

    try {
      await reorderRules(reordered.map(r => r.id));
    } catch (e) {
      addToast('Error reordering: ' + handleError(e), 'error');
      await loadRules();
    }
  }

  function handleDragEnd() {
    dragSourceId = null;
    dragOverId = null;
  }

  // --- Dry run ---

  function openDryRun(ruleId: string) {
    dryRunRuleId = ruleId;
    dryRunPrompt = '';
    dryRunResult = null;
  }

  function closeDryRun() {
    dryRunRuleId = null;
    dryRunPrompt = '';
    dryRunResult = null;
  }

  async function runDryRun() {
    if (!dryRunRuleId) return;
    dryRunLoading = true;
    dryRunResult = null;
    try {
      dryRunResult = await dryRunRule(dryRunRuleId, dryRunPrompt);
    } catch (e) {
      addToast('Dry run error: ' + handleError(e), 'error');
    }
    dryRunLoading = false;
  }

  // --- Rule history ---

  async function openHistory(ruleId: string) {
    if (historyRuleId === ruleId) {
      historyRuleId = null;
      historyEvents = [];
      return;
    }
    historyRuleId = ruleId;
    historyLoading = true;
    historyEvents = [];
    try {
      const all = await getRuleHistory(ruleId);
      historyEvents = all.slice(0, 10);
    } catch {
      historyEvents = [];
    }
    historyLoading = false;
  }

  function closeHistory() {
    historyRuleId = null;
    historyEvents = [];
  }
</script>

<div class="section-title rules-title">{$tr('rules.title')}</div>

<!-- Search bar -->
<div class="rules-search">
  <input
    class="jarvis-input rules-search-input"
    type="text"
    placeholder="Filter rules by name or trigger..."
    bind:value={searchQuery}
  />
  {#if searchQuery}
    <button class="rules-search-clear" onclick={() => searchQuery = ''}>✕</button>
  {/if}
</div>

{#if loadingRules}
  <div class="snapshots-empty">{$tr('rules.loading')}</div>
{:else}
  <div class="rules-list">
    {#each filteredRules as rule (rule.id)}
      <div
        class="rule-row"
        class:drag-over={dragOverId === rule.id}
        class:dragging={dragSourceId === rule.id}
        draggable={true}
        ondragstart={(e) => handleDragStart(e, rule.id)}
        ondragover={(e) => handleDragOver(e, rule.id)}
        ondragleave={handleDragLeave}
        ondrop={(e) => handleDrop(e, rule.id)}
        ondragend={handleDragEnd}
        role="row"
        aria-label={rule.name}
      >
        <span class="drag-handle" title="Drag to reorder">⠿</span>
        <label class="rule-toggle">
          <input type="checkbox" checked={rule.enabled} onchange={() => handleToggleRule(rule.id, !rule.enabled)} />
        </label>
        {#if rule.priority !== undefined}
          <span class="priority-badge" title="Priority">#{rule.priority + 1}</span>
        {/if}
        <div class="rule-info">
          <span class="rule-name">{rule.name}</span>
          <span class="rule-meta">
            <span class="rule-trigger">{rule.trigger}</span>
            → <span class="rule-action-type">{rule.action.actionType}</span>
            {#if rule.fireCount > 0}
              <span class="rule-fires">({rule.fireCount}x)</span>
            {/if}
          </span>
        </div>
        <div class="rule-actions">
          <button class="jarvis-btn jarvis-btn-sm" title="Dry run" onclick={() => openDryRun(rule.id)}>▷ Test</button>
          <button
            class="jarvis-btn jarvis-btn-sm"
            class:active={historyRuleId === rule.id}
            title="Rule history"
            onclick={() => openHistory(rule.id)}
          >⟳ History</button>
          <button class="jarvis-btn jarvis-btn-danger jarvis-btn-sm" onclick={() => requestDeleteRule(rule.id)}>{$tr('common.delete')}</button>
        </div>
      </div>

      <!-- History dropdown (inline below matching rule) -->
      {#if historyRuleId === rule.id}
        <div class="history-panel">
          <div class="history-panel-header">
            <span>Last 10 fires</span>
            <button class="history-close" onclick={closeHistory}>✕</button>
          </div>
          {#if historyLoading}
            <div class="history-empty">Loading...</div>
          {:else if historyEvents.length === 0}
            <div class="history-empty">No history yet.</div>
          {:else}
            {#each historyEvents as ev}
              <div class="history-event">
                <span class="history-ts">{new Date(ev.timestamp).toLocaleString()}</span>
                <span class="history-trigger">{ev.trigger}</span>
                <span class="history-result" class:ok={ev.result === 'ok'}>{ev.result}</span>
              </div>
            {/each}
          {/if}
        </div>
      {/if}
    {/each}

    {#if filteredRules.length === 0 && searchQuery}
      <div class="snapshots-empty">No rules match "{searchQuery}"</div>
    {/if}
  </div>

  {#if addingRule}
    <div class="rule-editor">
      <label class="jarvis-label">{$tr('rules.labelName')} <input class="jarvis-input" type="text" bind:value={newRule.name} placeholder={t('rules.placeholderName')} /></label>
      <label class="jarvis-label">{$tr('rules.labelTrigger')}
        <select class="jarvis-input" bind:value={newRule.trigger}>
          <option value="on_task_complete">{$tr('rules.triggerTaskComplete')}</option>
          <option value="on_task_fail">{$tr('rules.triggerTaskFailed')}</option>
          <option value="on_push">{$tr('rules.triggerPush')}</option>
          <option value="on_pipeline_complete">{$tr('rules.triggerPipeline')}</option>
        </select>
      </label>
      <label class="jarvis-label">{$tr('rules.labelAction')}
        <select class="jarvis-input" bind:value={newRule.action.actionType}>
          <option value="alert">{$tr('rules.actionAlert')}</option>
          <option value="run_task">{$tr('rules.actionRunTask')}</option>
          <option value="send_webhook">{$tr('rules.actionWebhook')}</option>
          <option value="send_message">{$tr('rules.actionMessage')}</option>
        </select>
      </label>
      {#if newRule.action.actionType === 'run_task'}
        <label class="jarvis-label">{$tr('rules.labelTarget')}
          <select class="jarvis-input" bind:value={newRule.action.target}>
            <option value="atlas">Atlas</option>
            <option value="pixel">Pixel</option>
          </select>
        </label>
        <label class="jarvis-label">{$tr('rules.labelPrompt')} <input class="jarvis-input" type="text" bind:value={newRule.action.prompt} placeholder={t('rules.placeholderPrompt')} /></label>
      {:else}
        <label class="jarvis-label">{$tr('rules.labelMessage')} <input class="jarvis-input" type="text" bind:value={newRule.action.message} placeholder={t('rules.placeholderMessage')} /></label>
      {/if}
      <div class="jarvis-editor-actions">
        <button class="jarvis-btn jarvis-btn-cancel" onclick={() => addingRule = false}>{$tr('common.cancel')}</button>
        <button class="jarvis-btn jarvis-btn-primary" onclick={handleSaveNewRule}>{$tr('common.save')}</button>
      </div>
    </div>
  {:else}
    <button class="jarvis-btn" onclick={() => addingRule = true}>{$tr('rules.addRule')}</button>
  {/if}
{/if}

<!-- Dry run modal -->
{#if dryRunRuleId !== null}
  <div class="modal-backdrop" onclick={closeDryRun} role="dialog" aria-modal="true" aria-label="Dry run">
    <div class="dry-run-modal" onclick={(e) => e.stopPropagation()}>
      <div class="dry-run-header">
        <span class="dry-run-title">Dry Run Test</span>
        <button class="history-close" onclick={closeDryRun}>✕</button>
      </div>
      <label class="jarvis-label">
        Test prompt
        <input
          class="jarvis-input"
          type="text"
          placeholder="Enter a test prompt..."
          bind:value={dryRunPrompt}
          onkeydown={(e) => { if (e.key === 'Enter') runDryRun(); }}
        />
      </label>
      <div class="dry-run-actions">
        <button class="jarvis-btn jarvis-btn-primary" onclick={runDryRun} disabled={dryRunLoading}>
          {dryRunLoading ? 'Running...' : 'Run'}
        </button>
      </div>
      {#if dryRunResult !== null}
        <div class="dry-run-result" class:match={dryRunResult} class:no-match={!dryRunResult}>
          {dryRunResult ? '✓ would match' : '✗ no match'}
        </div>
      {/if}
    </div>
  </div>
{/if}

<ConfirmModal
  open={showDeleteConfirm}
  title={t('rules.deleteConfirmTitle')}
  message={t('rules.deleteConfirmMessage')}
  confirmText={t('common.delete')}
  cancelText={t('common.cancel')}
  variant="danger"
  onConfirm={confirmDeleteRule}
  onCancel={cancelDeleteRule}
/>

<style>
  .section-title {
    font-size: 9px;
    font-family: var(--font-display);
    text-transform: uppercase;
    letter-spacing: 1px;
    font-weight: 600;
    color: var(--text-2);
    margin-bottom: 2px;
  }
  .rules-title { color: var(--amber); }
  .snapshots-empty {
    font-size: 10px;
    color: var(--text-3);
    padding: 8px 0;
  }

  /* Search bar */
  .rules-search {
    position: relative;
    margin-bottom: 6px;
  }
  .rules-search-input {
    width: 100%;
    font-size: 10px;
    padding-right: 24px;
    box-sizing: border-box;
  }
  .rules-search-clear {
    position: absolute;
    right: 6px;
    top: 50%;
    transform: translateY(-50%);
    background: none;
    border: none;
    color: var(--text-3);
    cursor: pointer;
    font-size: 10px;
    padding: 0;
  }
  .rules-search-clear:hover { color: var(--text-0); }

  /* Rules list */
  .rules-list { display: flex; flex-direction: column; gap: 4px; }

  .rule-row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 8px;
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 5px;
    cursor: default;
    transition: background 0.1s, border-color 0.1s;
  }
  .rule-row.drag-over {
    border-color: var(--amber);
    background: var(--bg-2);
  }
  .rule-row.dragging {
    opacity: 0.5;
  }

  .drag-handle {
    color: var(--text-3);
    cursor: grab;
    font-size: 13px;
    line-height: 1;
    user-select: none;
    flex-shrink: 0;
  }
  .drag-handle:active { cursor: grabbing; }

  .rule-toggle input { accent-color: var(--amber); }

  .priority-badge {
    font-size: 8px;
    font-weight: 700;
    color: var(--amber);
    background: color-mix(in srgb, var(--amber) 15%, transparent);
    border: 1px solid color-mix(in srgb, var(--amber) 40%, transparent);
    border-radius: 3px;
    padding: 1px 4px;
    flex-shrink: 0;
    font-family: var(--font-display);
  }

  .rule-info { flex: 1; display: flex; flex-direction: column; gap: 1px; min-width: 0; }
  .rule-name { font-size: 11px; font-weight: 600; color: var(--text-0); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .rule-meta { font-size: 9px; color: var(--text-3); display: flex; gap: 4px; align-items: center; }
  .rule-trigger { color: var(--amber); font-weight: 600; }
  .rule-action-type { color: var(--cyan); }
  .rule-fires { color: var(--text-3); }

  .rule-actions { display: flex; gap: 4px; flex-shrink: 0; }

  .jarvis-btn-sm {
    font-size: 9px !important;
    padding: 2px 6px !important;
    height: auto !important;
  }
  .jarvis-btn-sm.active {
    background: color-mix(in srgb, var(--cyan) 20%, transparent);
    border-color: var(--cyan);
    color: var(--cyan);
  }

  /* History panel (inline below rule) */
  .history-panel {
    background: var(--bg-0);
    border: 1px solid var(--border);
    border-radius: 5px;
    padding: 6px 8px;
    margin-top: -2px;
    font-size: 10px;
  }
  .history-panel-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: 9px;
    font-weight: 700;
    color: var(--text-2);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    margin-bottom: 4px;
  }
  .history-close {
    background: none;
    border: none;
    color: var(--text-3);
    cursor: pointer;
    font-size: 10px;
    padding: 0;
  }
  .history-close:hover { color: var(--text-0); }
  .history-empty { color: var(--text-3); font-size: 10px; padding: 4px 0; }
  .history-event {
    display: flex;
    gap: 8px;
    align-items: center;
    padding: 2px 0;
    border-bottom: 1px solid var(--border);
  }
  .history-event:last-child { border-bottom: none; }
  .history-ts { color: var(--text-3); font-size: 9px; flex-shrink: 0; }
  .history-trigger { color: var(--amber); font-size: 9px; }
  .history-result { color: var(--text-3); font-size: 9px; margin-left: auto; }
  .history-result.ok { color: var(--green, #4ade80); }

  /* Rule editor */
  .rule-editor {
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  /* Dry run modal */
  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
  }
  .dry-run-modal {
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 14px;
    width: 320px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);
  }
  .dry-run-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .dry-run-title {
    font-size: 11px;
    font-weight: 700;
    color: var(--text-0);
    font-family: var(--font-display);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }
  .dry-run-actions { display: flex; justify-content: flex-end; }
  .dry-run-result {
    font-size: 12px;
    font-weight: 700;
    text-align: center;
    padding: 6px;
    border-radius: 5px;
  }
  .dry-run-result.match {
    color: var(--green, #4ade80);
    background: color-mix(in srgb, var(--green, #4ade80) 15%, transparent);
  }
  .dry-run-result.no-match {
    color: var(--red, #f87171);
    background: color-mix(in srgb, var(--red, #f87171) 15%, transparent);
  }
</style>
