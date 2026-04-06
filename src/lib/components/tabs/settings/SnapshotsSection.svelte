<script lang="ts">
  import { onMount } from 'svelte';
  import { saveSnapshot, listSnapshots, restoreSnapshot, deleteSnapshot } from '../../../api';
  import { addToast } from '../../../stores/notifications';
  import { handleError } from '../../../utils';
  import { t, tr } from '$lib/i18n';
  import type { SnapshotSummary } from '../../../types';
  import ConfirmModal from '../../ConfirmModal.svelte';

  let { onConfigReload }: { onConfigReload: () => Promise<void> } = $props();

  let snapshots = $state<SnapshotSummary[]>([]);
  let snapshotName = $state('');
  let savingSnapshot = $state(false);

  // ConfirmModal state
  let showRestoreConfirm = $state(false);
  let showDeleteConfirm = $state(false);
  let pendingSnapshotName = $state('');

  onMount(() => {
    loadSnapshots();
  });

  async function loadSnapshots() {
    try {
      snapshots = await listSnapshots();
    } catch (e) {
      addToast(t('common.error') + ': ' + handleError(e), 'error');
    }
  }

  async function handleSaveSnapshot() {
    if (!snapshotName.trim()) {
      addToast(t('snapshots.enterName'), 'error');
      return;
    }
    savingSnapshot = true;
    try {
      await saveSnapshot(snapshotName.trim());
      addToast(t('snapshots.saved') + ': ' + snapshotName, 'success');
      snapshotName = '';
      await loadSnapshots();
    } catch (e) {
      addToast(t('common.error') + ': ' + handleError(e), 'error');
    }
    savingSnapshot = false;
  }

  function confirmRestore(name: string) {
    pendingSnapshotName = name;
    showRestoreConfirm = true;
  }

  async function handleRestoreSnapshot() {
    try {
      await restoreSnapshot(pendingSnapshotName);
      addToast(t('snapshots.restored') + ': ' + pendingSnapshotName, 'success');
      await onConfigReload();
    } catch (e) {
      addToast(t('common.error') + ': ' + handleError(e), 'error');
    }
    showRestoreConfirm = false;
  }

  function confirmDelete(name: string) {
    pendingSnapshotName = name;
    showDeleteConfirm = true;
  }

  async function handleDeleteSnapshot() {
    try {
      await deleteSnapshot(pendingSnapshotName);
      addToast(t('snapshots.deleted') + ': ' + pendingSnapshotName, 'success');
      await loadSnapshots();
    } catch (e) {
      addToast(t('common.error') + ': ' + handleError(e), 'error');
    }
    showDeleteConfirm = false;
  }

  function formatDate(iso: string): string {
    try {
      return new Date(iso).toLocaleString('es-AR', { day: '2-digit', month: '2-digit', year: '2-digit', hour: '2-digit', minute: '2-digit' });
    } catch { return iso; }
  }
</script>

<div class="section-title">{$tr('snapshots.title')}</div>
<div class="snapshot-save-row">
  <input type="text" bind:value={snapshotName} placeholder={$tr('snapshots.namePlaceholder')} class="jarvis-input snapshot-input" />
  <button class="jarvis-btn jarvis-btn-primary" onclick={handleSaveSnapshot} disabled={savingSnapshot}>
    {savingSnapshot ? $tr('common.saving') : $tr('snapshots.saveSnapshot')}
  </button>
</div>
{#if snapshots.length > 0}
  <div class="snapshots-list">
    {#each snapshots as snap}
      <div class="snapshot-row">
        <div class="snapshot-info">
          <span class="snapshot-name">{snap.name}</span>
          <span class="snapshot-meta">{formatDate(snap.createdAt)}</span>
          {#if snap.rama}
            <span class="snapshot-meta">{$tr('snapshots.branch')}: {snap.rama}</span>
          {/if}
          {#if snap.objetivo}
            <span class="snapshot-meta snapshot-objetivo" title={snap.objetivo}>
              {snap.objetivo.length > 60 ? snap.objetivo.slice(0, 60) + '...' : snap.objetivo}
            </span>
          {/if}
        </div>
        <div class="snapshot-actions">
          <button class="jarvis-btn" onclick={() => confirmRestore(snap.name)}>{$tr('snapshots.restore')}</button>
          <button class="jarvis-btn jarvis-btn-danger" onclick={() => confirmDelete(snap.name)}>{$tr('snapshots.delete')}</button>
        </div>
      </div>
    {/each}
  </div>
{:else}
  <div class="snapshots-empty">{$tr('snapshots.noSnapshots')}</div>
{/if}

<ConfirmModal
  open={showRestoreConfirm}
  title={$tr('snapshots.restore')}
  message={$tr('snapshots.restoreConfirm', { name: pendingSnapshotName })}
  confirmText={$tr('common.confirm')}
  cancelText={$tr('common.cancel')}
  onConfirm={handleRestoreSnapshot}
  onCancel={() => showRestoreConfirm = false}
/>

<ConfirmModal
  open={showDeleteConfirm}
  title={$tr('snapshots.delete')}
  message={$tr('snapshots.deleteConfirm', { name: pendingSnapshotName })}
  confirmText={$tr('common.delete')}
  cancelText={$tr('common.cancel')}
  onConfirm={handleDeleteSnapshot}
  onCancel={() => showDeleteConfirm = false}
  variant="danger"
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
  .snapshot-save-row {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .snapshot-input { flex: 1; }
  .snapshots-list {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-top: 6px;
  }
  .snapshot-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 6px 10px;
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 5px;
  }
  .snapshot-info {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
    flex: 1;
  }
  .snapshot-name {
    font-size: 11px;
    font-weight: 700;
    color: var(--text-0);
  }
  .snapshot-meta {
    font-size: 9px;
    color: var(--text-3);
  }
  .snapshot-objetivo {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .snapshot-actions {
    display: flex;
    gap: 4px;
    flex-shrink: 0;
  }
  .snapshots-empty {
    font-size: 10px;
    color: var(--text-3);
    padding: 8px 0;
  }

</style>
