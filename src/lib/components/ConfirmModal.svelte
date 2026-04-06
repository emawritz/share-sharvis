<script lang="ts">
  import { tick } from 'svelte';
  import { fade, scale } from 'svelte/transition';
  import { tr } from '$lib/i18n';

  interface Props {
    open: boolean;
    title: string;
    message: string;
    confirmText?: string;
    cancelText?: string;
    onConfirm: () => void;
    onCancel: () => void;
    variant?: 'danger' | 'default';
    /** If true, show an input field and pass its value to onConfirmWithValue */
    showInput?: boolean;
    inputPlaceholder?: string;
    inputValue?: string;
    onConfirmWithValue?: (value: string) => void;
  }

  let {
    open,
    title,
    message,
    confirmText = '',
    cancelText = '',
    onConfirm,
    onCancel,
    variant = 'default',
    showInput = false,
    inputPlaceholder = '',
    inputValue = '',
    onConfirmWithValue,
  }: Props = $props();

  let confirmBtn: HTMLButtonElement | undefined = $state();
  let cancelBtn: HTMLButtonElement | undefined = $state();
  let inputEl: HTMLInputElement | undefined = $state();
  let localInputValue = $state('');

  $effect(() => {
    if (open) localInputValue = inputValue;
  });

  $effect(() => {
    if (open) {
      tick().then(() => {
        if (showInput && inputEl) inputEl.focus();
        else if (confirmBtn) confirmBtn.focus();
      });
    }
  });

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.stopPropagation();
      onCancel();
    }
    if (e.key === 'Enter' && showInput) {
      e.preventDefault();
      handleConfirm();
    }
    if (e.key === 'Tab') {
      const focusable: HTMLElement[] = [];
      if (showInput && inputEl) focusable.push(inputEl);
      if (cancelBtn) focusable.push(cancelBtn);
      if (confirmBtn) focusable.push(confirmBtn);
      if (focusable.length === 0) return;
      const idx = focusable.indexOf(document.activeElement as HTMLElement);
      if (e.shiftKey) {
        e.preventDefault();
        const prev = idx <= 0 ? focusable.length - 1 : idx - 1;
        focusable[prev].focus();
      } else {
        e.preventDefault();
        const next = idx >= focusable.length - 1 ? 0 : idx + 1;
        focusable[next].focus();
      }
    }
  }

  function handleOverlayClick(e: MouseEvent) {
    if ((e.target as HTMLElement)?.classList?.contains('confirm-overlay')) {
      onCancel();
    }
  }

  function handleConfirm() {
    if (showInput && onConfirmWithValue) {
      onConfirmWithValue(localInputValue);
    } else {
      onConfirm();
    }
  }
</script>

{#if open}
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="confirm-overlay" role="dialog" aria-modal="true" aria-label={title} tabindex="-1" onclick={handleOverlayClick} onkeydown={handleKeydown} transition:fade={{ duration: 150 }}>
  <div class="confirm-modal" in:scale={{ duration: 200, start: 0.95 }} out:fade={{ duration: 100 }}>
    <div class="confirm-title">{title}</div>
    <div class="confirm-message">{message}</div>
    {#if showInput}
      <input
        class="confirm-input"
        type="text"
        placeholder={inputPlaceholder}
        bind:value={localInputValue}
        bind:this={inputEl}
      />
    {/if}
    <div class="confirm-actions">
      <button class="confirm-btn cancel" type="button" onclick={onCancel} bind:this={cancelBtn}>{cancelText || $tr('common.cancel')}</button>
      <button
        class="confirm-btn ok {variant}"
        type="button"
        onclick={handleConfirm}
        bind:this={confirmBtn}
        disabled={showInput && !localInputValue.trim()}
      >{confirmText || $tr('common.confirm')}</button>
    </div>
  </div>
</div>
{/if}

<style>
  .confirm-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.85);
    z-index: 2000;
    display: grid;
    place-items: center;
    backdrop-filter: blur(4px);
  }
  .confirm-modal {
    background: var(--bg-0);
    border: 1px solid var(--border-bright);
    border-radius: 12px;
    padding: 24px;
    min-width: 340px;
    max-width: 480px;
    box-shadow: 0 24px 80px rgba(0, 0, 0, 0.6);
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .confirm-title {
    font-family: var(--font-display);
    font-size: 13px;
    font-weight: 700;
    letter-spacing: 1px;
    color: var(--text-0);
    text-transform: uppercase;
  }
  .confirm-message {
    font-size: 12px;
    color: var(--text-1);
    line-height: 1.6;
  }
  .confirm-input {
    background: var(--bg-1);
    color: var(--text-0);
    border: 1px solid var(--border-bright);
    border-radius: 6px;
    padding: 9px 14px;
    font-family: var(--font-mono);
    font-size: 12px;
  }
  .confirm-input:focus {
    outline: none;
    border-color: var(--cyan);
    box-shadow: 0 0 0 3px var(--cyan-dim);
  }
  .confirm-input::placeholder {
    color: var(--text-3);
  }
  .confirm-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 4px;
  }
  .confirm-btn {
    padding: 9px 20px;
    border-radius: 6px;
    font-family: var(--font-display);
    font-size: 10px;
    font-weight: 700;
    cursor: pointer;
    letter-spacing: 1px;
    text-transform: uppercase;
    transition: background 0.15s, box-shadow 0.15s;
    border: 1px solid var(--border);
    white-space: nowrap;
  }
  .confirm-btn.cancel {
    background: var(--bg-2);
    color: var(--text-1);
  }
  .confirm-btn.cancel:hover {
    background: var(--bg-3);
    color: var(--text-0);
  }
  .confirm-btn.ok.default {
    background: linear-gradient(180deg, #0088cc 0%, #006699 100%);
    color: var(--text-0);
    border: 1px solid #0099dd;
  }
  .confirm-btn.ok.default:hover {
    background: linear-gradient(180deg, #0099dd 0%, #0077aa 100%);
    box-shadow: 0 2px 16px rgba(0, 136, 204, 0.4);
  }
  .confirm-btn.ok.danger {
    background: linear-gradient(180deg, #d32f2f 0%, #b71c1c 100%);
    color: var(--text-0);
    border: 1px solid #ef5350;
  }
  .confirm-btn.ok.danger:hover {
    background: linear-gradient(180deg, #e53935 0%, #c62828 100%);
    box-shadow: 0 2px 16px rgba(239, 83, 80, 0.4);
  }
  .confirm-btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
</style>
