<script lang="ts">
  import { onDestroy } from 'svelte';
  import { tr } from '$lib/i18n';

  let { direction = 'horizontal' }: { direction?: 'horizontal' | 'vertical' } = $props();

  let isActive = $state(false);
  let cleanupDrag: (() => void) | null = null;
  onDestroy(() => { cleanupDrag?.(); });

  function onKeyDown(e: KeyboardEvent) {
    const handle = e.currentTarget as HTMLElement;
    const parent = handle.parentElement;
    if (!parent) return;

    if (direction === 'horizontal') {
      if (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight') return;
      e.preventDefault();
      const dx = e.key === 'ArrowLeft' ? -20 : 20;
      const prev = handle.previousElementSibling as HTMLElement | null;
      const next = handle.nextElementSibling as HTMLElement | null;
      if (!prev || !next) return;
      const wL = prev.offsetWidth;
      const wR = next.offsetWidth;
      const total = wL + wR;
      const nL = Math.max(120, Math.min(total - 120, wL + dx));
      prev.style.flex = 'none';
      next.style.flex = 'none';
      prev.style.width = nL + 'px';
      next.style.width = (total - nL) + 'px';
    } else {
      if (e.key !== 'ArrowUp' && e.key !== 'ArrowDown') return;
      e.preventDefault();
      const dy = e.key === 'ArrowUp' ? 20 : -20;
      const next = handle.nextElementSibling as HTMLElement | null;
      if (!next) return;
      const h0 = next.offsetHeight;
      next.style.height = Math.max(60, Math.min(window.innerHeight - 200, h0 + dy)) + 'px';
    }
  }

  function onMouseDown(e: MouseEvent) {
    e.preventDefault();
    isActive = true;
    const cls = direction === 'horizontal' ? 'resizing' : 'resizing-v';
    document.body.classList.add(cls);

    const handle = e.currentTarget as HTMLElement;
    const parent = handle.parentElement;
    if (!parent) return;

    if (direction === 'horizontal') {
      const prev = handle.previousElementSibling as HTMLElement | null;
      const next = handle.nextElementSibling as HTMLElement | null;
      if (!prev || !next) return;

      const x0 = e.clientX;
      const wL = prev.offsetWidth;
      const wR = next.offsetWidth;

      function onMove(ev: MouseEvent) {
        const dx = ev.clientX - x0;
        const total = wL + wR;
        const nL = Math.max(120, Math.min(total - 120, wL + dx));
        prev!.style.flex = 'none';
        next!.style.flex = 'none';
        prev!.style.width = nL + 'px';
        next!.style.width = (total - nL) + 'px';
      }
      function onUp() {
        isActive = false;
        document.body.classList.remove('resizing');
        document.removeEventListener('mousemove', onMove);
        document.removeEventListener('mouseup', onUp);
        cleanupDrag = null;
      }
      cleanupDrag = () => {
        document.removeEventListener('mousemove', onMove);
        document.removeEventListener('mouseup', onUp);
        document.body.classList.remove('resizing');
      };
      document.addEventListener('mousemove', onMove);
      document.addEventListener('mouseup', onUp);
    } else {
      const next = handle.nextElementSibling as HTMLElement | null;
      if (!next) return;

      const y0 = e.clientY;
      const h0 = next.offsetHeight;

      function onMove(ev: MouseEvent) {
        next!.style.height = Math.max(60, Math.min(window.innerHeight - 200, h0 - (ev.clientY - y0))) + 'px';
      }
      function onUp() {
        isActive = false;
        document.body.classList.remove('resizing-v');
        document.removeEventListener('mousemove', onMove);
        document.removeEventListener('mouseup', onUp);
        cleanupDrag = null;
      }
      cleanupDrag = () => {
        document.removeEventListener('mousemove', onMove);
        document.removeEventListener('mouseup', onUp);
        document.body.classList.remove('resizing-v');
      };
      document.addEventListener('mousemove', onMove);
      document.addEventListener('mouseup', onUp);
    }
  }
</script>

{#if direction === 'horizontal'}
  <button
    class="resize-h"
    class:active={isActive}
    onmousedown={onMouseDown}
    onkeydown={onKeyDown}
    aria-label={$tr('a11y.resizePanels')}
  ></button>
{:else}
  <button
    class="resize-v"
    class:active={isActive}
    onmousedown={onMouseDown}
    onkeydown={onKeyDown}
    aria-label={$tr('a11y.resizeSections')}
  ></button>
{/if}

<style>
  .resize-h {
    width: 5px;
    background: var(--border);
    cursor: col-resize;
    flex-shrink: 0;
    z-index: 10;
    transition: background 0.15s ease;
    position: relative;
    border: none;
    padding: 0;
    margin: 0;
    appearance: none;
    display: block;
  }
  .resize-h::after {
    content: '';
    position: absolute;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    width: 1px;
    height: 24px;
    background: var(--text-3);
    border-radius: 1px;
    opacity: 0;
    transition: opacity 0.15s ease;
  }
  .resize-h:hover, .resize-h.active { background: var(--cyan); }
  .resize-h:hover::after { opacity: 1; }
  .resize-h:focus-visible, .resize-v:focus-visible {
    outline: 2px solid var(--cyan);
    outline-offset: -1px;
  }
  .resize-v {
    height: 5px;
    background: var(--border);
    cursor: row-resize;
    flex-shrink: 0;
    z-index: 10;
    transition: background 0.15s ease;
    position: relative;
    border: none;
    padding: 0;
    margin: 0;
    appearance: none;
    display: block;
    width: 100%;
  }
  .resize-v::after {
    content: '';
    position: absolute;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    width: 24px;
    height: 1px;
    background: var(--text-3);
    border-radius: 1px;
    opacity: 0;
    transition: opacity 0.15s ease;
  }
  .resize-v:hover, .resize-v.active { background: var(--cyan); }
  .resize-v:hover::after { opacity: 1; }
</style>
