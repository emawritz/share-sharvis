<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { addToast } from '../../../stores/notifications';
  import { handleError } from '../../../utils';
  import { t, tr } from '$lib/i18n';
  import type { WebhookConfig, WebhookDelivery } from '$lib/types';

  const EVENT_FILTER_OPTIONS = [
    { value: 'task_complete', label: 'task_complete' },
    { value: 'task_error', label: 'task_error' },
    { value: 'pipeline_done', label: 'pipeline_done' },
    { value: 'rule_fired', label: 'rule_fired' },
    { value: 'cron_fired', label: 'cron_fired' },
  ];

  let webhookCfg = $state<WebhookConfig>({
    enabled: false,
    url: '',
    webhookType: 'discord',
    onTaskComplete: true,
    onTaskFail: false,
    onPipelineComplete: false,
    eventFilter: [],
    lastDelivery: null,
    lastStatusCode: null,
  });
  let savingWebhook = $state(false);
  let testingWebhook = $state(false);
  let deliveries = $state<WebhookDelivery[]>([]);
  let loadingDeliveries = $state(false);
  let expandedIds = $state<Set<number>>(new Set());

  onMount(() => {
    loadWebhookConfig();
    loadDeliveries();
  });

  async function loadWebhookConfig() {
    try {
      const cfg = await invoke<WebhookConfig>('get_webhook_config');
      webhookCfg = cfg;
    } catch (e) {
      // Config may not exist yet, use defaults
    }
  }

  async function loadDeliveries() {
    loadingDeliveries = true;
    try {
      const result = await invoke<WebhookDelivery[]>('get_webhook_deliveries');
      deliveries = result.slice(0, 20);
    } catch (e) {
      // Deliveries may not exist yet
    }
    loadingDeliveries = false;
  }

  async function saveWebhookConfig() {
    savingWebhook = true;
    try {
      await invoke('save_webhook_settings', { config: webhookCfg });
      addToast(t('webhooks.saved'), 'success');
    } catch (e) {
      addToast('Error: ' + handleError(e), 'error');
    }
    savingWebhook = false;
  }

  async function testWebhookNotification() {
    testingWebhook = true;
    try {
      await invoke('test_webhook');
      addToast(t('webhooks.testSent'), 'success');
    } catch (e) {
      addToast('Error: ' + handleError(e), 'error');
    }
    testingWebhook = false;
  }

  function toggleEventFilter(value: string) {
    const current = webhookCfg.eventFilter ?? [];
    if (current.includes(value)) {
      webhookCfg.eventFilter = current.filter((v) => v !== value);
    } else {
      webhookCfg.eventFilter = [...current, value];
    }
  }

  function toggleExpanded(id: number) {
    const next = new Set(expandedIds);
    if (next.has(id)) {
      next.delete(id);
    } else {
      next.add(id);
    }
    expandedIds = next;
  }

  function formatTimestamp(ts: string): string {
    try {
      return new Date(ts).toLocaleString();
    } catch {
      return ts;
    }
  }

  function statusClass(code: number | null): string {
    if (code === null) return 'status-unknown';
    if (code >= 200 && code < 300) return 'status-ok';
    return 'status-err';
  }

  let lastDeliveryDisplay = $derived(
    webhookCfg.lastDelivery ? formatTimestamp(webhookCfg.lastDelivery) : '—'
  );
  let lastStatusDisplay = $derived(
    webhookCfg.lastStatusCode !== null ? String(webhookCfg.lastStatusCode) : '—'
  );
  let lastStatusCls = $derived(statusClass(webhookCfg.lastStatusCode));
</script>

<div class="section-title webhook-title">Webhooks</div>
<div class="webhook-zone">
  <!-- Header status line -->
  <div class="webhook-status-line">
    <span class="status-label">Last delivery:</span>
    <span class="status-value">{lastDeliveryDisplay}</span>
    <span class="status-sep">|</span>
    <span class="status-label">Last status:</span>
    <span class="status-value {lastStatusCls}">{lastStatusDisplay}</span>
  </div>

  <label class="webhook-toggle">
    <input type="checkbox" bind:checked={webhookCfg.enabled} />
    <span>{$tr('webhooks.enableWebhooks')}</span>
  </label>

  {#if webhookCfg.enabled}
    <div class="webhook-fields">
      <label class="jarvis-label webhook-field">
        {$tr('webhooks.type')}
        <select class="jarvis-input" bind:value={webhookCfg.webhookType}>
          <option value="discord">Discord</option>
          <option value="slack">Slack</option>
        </select>
      </label>
      <label class="jarvis-label webhook-field">
        {$tr('webhooks.url')}
        <input class="jarvis-input" type="text" bind:value={webhookCfg.url} placeholder="https://discord.com/api/webhooks/..." />
      </label>
    </div>

    <div class="webhook-events">
      <label class="webhook-check">
        <input type="checkbox" bind:checked={webhookCfg.onTaskComplete} />
        <span>{$tr('webhooks.taskComplete')}</span>
      </label>
      <label class="webhook-check">
        <input type="checkbox" bind:checked={webhookCfg.onTaskFail} />
        <span>{$tr('webhooks.taskFailed')}</span>
      </label>
      <label class="webhook-check">
        <input type="checkbox" bind:checked={webhookCfg.onPipelineComplete} />
        <span>{$tr('webhooks.pipelineComplete')}</span>
      </label>
    </div>

    <!-- Event filter multi-select -->
    <div class="event-filter-section">
      <div class="event-filter-title">Event Filter <span class="filter-hint">(empty = all events)</span></div>
      <div class="event-filter-checks">
        {#each EVENT_FILTER_OPTIONS as opt}
          <label class="webhook-check">
            <input
              type="checkbox"
              checked={(webhookCfg.eventFilter ?? []).includes(opt.value)}
              onchange={() => toggleEventFilter(opt.value)}
            />
            <span class="filter-tag">{opt.label}</span>
          </label>
        {/each}
      </div>
    </div>

    <div class="webhook-actions">
      <button class="jarvis-btn jarvis-btn-primary" onclick={saveWebhookConfig} disabled={savingWebhook}>
        {savingWebhook ? $tr('common.saving') : $tr('common.save')}
      </button>
      <button class="jarvis-btn jarvis-btn-test" onclick={testWebhookNotification} disabled={testingWebhook || !webhookCfg.url}>
        {testingWebhook ? $tr('common.sending') : $tr('common.test')}
      </button>
    </div>
  {/if}
</div>

<!-- Delivery Log -->
<div class="section-title delivery-title">Delivery Log</div>
<div class="delivery-zone">
  <div class="delivery-header">
    <span class="delivery-count">{deliveries.length} entries</span>
    <button class="jarvis-btn jarvis-btn-sm" onclick={loadDeliveries} disabled={loadingDeliveries}>
      {loadingDeliveries ? 'Loading…' : 'Refresh deliveries'}
    </button>
  </div>

  {#if deliveries.length === 0}
    <div class="delivery-empty">{loadingDeliveries ? 'Loading…' : 'No deliveries yet.'}</div>
  {:else}
    <table class="delivery-table">
      <thead>
        <tr>
          <th>Timestamp</th>
          <th>Event</th>
          <th>Status</th>
          <th>Result</th>
          <th>Response</th>
        </tr>
      </thead>
      <tbody>
        {#each deliveries as d, i}
          <tr class={i % 2 === 0 ? 'row-even' : 'row-odd'}>
            <td class="ts-cell">{formatTimestamp(d.timestamp)}</td>
            <td class="event-cell"><span class="event-tag">{d.eventType}</span></td>
            <td class="code-cell">
              <span class="status-badge {statusClass(d.statusCode)}">
                {d.statusCode ?? '—'}
              </span>
            </td>
            <td class="success-cell">
              {#if d.success}
                <span class="badge-ok">OK</span>
              {:else}
                <span class="badge-fail">FAIL</span>
              {/if}
            </td>
            <td class="snippet-cell">
              {#if d.responseSnippet}
                <button class="snippet-toggle" onclick={() => toggleExpanded(d.id)}>
                  {expandedIds.has(d.id) ? 'hide' : 'show'}
                </button>
                {#if expandedIds.has(d.id)}
                  <div class="snippet-body">{d.responseSnippet}</div>
                {/if}
              {:else}
                <span class="no-snippet">—</span>
              {/if}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>

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
  .webhook-title { color: var(--cyan); }
  .delivery-title { color: var(--cyan); margin-top: 10px; }

  .webhook-zone {
    background: #00d4ff08;
    border: 1px solid #00d4ff22;
    border-radius: 6px;
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  /* Status line */
  .webhook-status-line {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 10px;
    flex-wrap: wrap;
  }
  .status-label {
    color: var(--text-2);
    font-weight: 600;
  }
  .status-value {
    color: var(--text-1);
    font-family: var(--font-mono, monospace);
  }
  .status-sep { color: var(--text-2); }
  .status-ok { color: #4ade80; }
  .status-err { color: #f87171; }
  .status-unknown { color: var(--text-2); }

  .webhook-toggle {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    color: var(--text-1);
    font-weight: 600;
    cursor: pointer;
  }
  .webhook-toggle input[type="checkbox"] { accent-color: var(--cyan); }

  .webhook-fields {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .webhook-events {
    display: flex;
    gap: 12px;
    flex-wrap: wrap;
  }

  /* Event filter */
  .event-filter-section {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .event-filter-title {
    font-size: 10px;
    font-weight: 600;
    color: var(--text-2);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }
  .filter-hint {
    font-weight: 400;
    font-size: 9px;
    text-transform: none;
    letter-spacing: 0;
    color: var(--text-2);
  }
  .event-filter-checks {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }
  .filter-tag {
    font-family: var(--font-mono, monospace);
    font-size: 10px;
  }

  .webhook-check {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 10px;
    color: var(--text-1);
    cursor: pointer;
  }
  .webhook-check input[type="checkbox"] { accent-color: var(--cyan); }

  .webhook-actions {
    display: flex;
    gap: 8px;
  }

  /* Delivery log */
  .delivery-zone {
    background: #00d4ff05;
    border: 1px solid #00d4ff18;
    border-radius: 6px;
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .delivery-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .delivery-count {
    font-size: 10px;
    color: var(--text-2);
  }
  .jarvis-btn-sm {
    font-size: 10px;
    padding: 2px 8px;
  }
  .delivery-empty {
    font-size: 10px;
    color: var(--text-2);
    text-align: center;
    padding: 8px 0;
  }

  /* Table */
  .delivery-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 10px;
  }
  .delivery-table th {
    text-align: left;
    font-size: 9px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--text-2);
    padding: 3px 6px;
    border-bottom: 1px solid #00d4ff18;
  }
  .delivery-table td {
    padding: 3px 6px;
    color: var(--text-1);
    vertical-align: top;
  }
  .row-even { background: transparent; }
  .row-odd  { background: #00d4ff05; }

  .ts-cell {
    font-family: var(--font-mono, monospace);
    font-size: 9px;
    color: var(--text-2);
    white-space: nowrap;
  }
  .event-cell { }
  .event-tag {
    font-family: var(--font-mono, monospace);
    font-size: 9px;
    background: #00d4ff14;
    border-radius: 3px;
    padding: 1px 4px;
    color: var(--cyan);
  }
  .code-cell { }
  .status-badge {
    font-family: var(--font-mono, monospace);
    font-size: 9px;
    font-weight: 700;
    border-radius: 3px;
    padding: 1px 4px;
  }
  .status-badge.status-ok  { background: #4ade8022; color: #4ade80; }
  .status-badge.status-err { background: #f8717122; color: #f87171; }
  .status-badge.status-unknown { background: #ffffff10; color: var(--text-2); }

  .success-cell { }
  .badge-ok {
    font-size: 9px;
    font-weight: 700;
    color: #4ade80;
    background: #4ade8018;
    border-radius: 3px;
    padding: 1px 4px;
  }
  .badge-fail {
    font-size: 9px;
    font-weight: 700;
    color: #f87171;
    background: #f8717118;
    border-radius: 3px;
    padding: 1px 4px;
  }

  .snippet-cell { }
  .snippet-toggle {
    background: none;
    border: none;
    cursor: pointer;
    font-size: 9px;
    color: var(--cyan);
    padding: 0;
    text-decoration: underline;
  }
  .snippet-body {
    margin-top: 3px;
    font-family: var(--font-mono, monospace);
    font-size: 9px;
    color: var(--text-2);
    white-space: pre-wrap;
    word-break: break-all;
    background: #00000020;
    border-radius: 3px;
    padding: 3px 5px;
    max-height: 80px;
    overflow-y: auto;
  }
  .no-snippet {
    color: var(--text-2);
    font-size: 9px;
  }
</style>
