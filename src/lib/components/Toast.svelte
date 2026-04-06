<script lang="ts">
  import { toasts, removeToast } from '../stores/notifications';
  import { tr } from '$lib/i18n';

  const typeIcon: Record<string, string> = {
    success: '+',
    error: 'x',
    info: 'i',
    warning: '!',
  };
</script>

<div class="toast-container" aria-live="polite">
  {#each $toasts as toast (toast.id)}
    <div class="toast {toast.type}" role="alert">
      <span class="toast-icon" aria-hidden="true">{typeIcon[toast.type] ?? 'i'}</span>
      <span class="toast-message">{toast.message}</span>
      <button
        class="toast-close"
        onclick={() => removeToast(toast.id)}
        aria-label={$tr('toast.close')}
      >
        &times;
      </button>
    </div>
  {/each}
</div>

<style>
  .toast-container {
    position: fixed;
    top: 56px;
    right: 16px;
    z-index: 10000;
    display: flex;
    flex-direction: column;
    gap: 6px;
    pointer-events: none;
  }
  .toast {
    background: var(--bg-2);
    border: 1px solid var(--border-bright);
    border-radius: var(--radius);
    padding: 8px 14px;
    font-size: 11px;
    color: var(--text-0);
    box-shadow: 0 8px 24px rgba(0,0,0,0.4);
    animation: toast-in 0.3s ease;
    pointer-events: auto;
    display: flex;
    align-items: center;
    gap: 8px;
    max-width: 420px;
    word-break: break-word;
    border-left-width: 3px;
    border-left-style: solid;
  }
  .toast.success { border-left-color: var(--green); }
  .toast.error   { border-left-color: var(--red); }
  .toast.info    { border-left-color: var(--cyan); }
  .toast.warning { border-left-color: #f59e0b; }

  .toast-icon {
    flex-shrink: 0;
    font-size: 10px;
    font-weight: 700;
    font-family: var(--font-mono, monospace);
    width: 14px;
    height: 14px;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    line-height: 1;
  }
  .toast.success .toast-icon { color: var(--green); }
  .toast.error   .toast-icon { color: var(--red); }
  .toast.info    .toast-icon { color: var(--cyan); }
  .toast.warning .toast-icon { color: #f59e0b; }

  .toast-message { flex: 1; }

  .toast-close {
    background: none;
    border: none;
    color: var(--text-2);
    cursor: pointer;
    font-size: 14px;
    padding: 0 2px;
    line-height: 1;
    flex-shrink: 0;
  }
  .toast-close:hover { color: var(--text-0); }

  @keyframes toast-in {
    from { opacity: 0; transform: translateX(16px); }
    to   { opacity: 1; transform: translateX(0); }
  }
</style>
