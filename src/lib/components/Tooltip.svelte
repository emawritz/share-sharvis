<script lang="ts">
  import { onDestroy } from 'svelte';
  import type { Snippet } from 'svelte';

  let {
    text,
    position = 'top',
    delay = 400,
    children
  }: {
    text: string;
    position?: 'top' | 'bottom' | 'left' | 'right';
    delay?: number;
    children: Snippet;
  } = $props();

  let visible = $state(false);
  let timer: ReturnType<typeof setTimeout> | null = null;
  let wrapper: HTMLElement | undefined = $state();
  let tooltipStyle = $state('');

  function show() {
    timer = setTimeout(() => {
      if (!wrapper) return;
      const rect = wrapper.getBoundingClientRect();
      tooltipStyle = calcPosition(rect);
      visible = true;
    }, delay);
  }

  function hide() {
    if (timer) { clearTimeout(timer); timer = null; }
    visible = false;
  }

  onDestroy(() => { if (timer) { clearTimeout(timer); timer = null; } });

  function calcPosition(rect: DOMRect): string {
    const gap = 6;
    switch (position) {
      case 'bottom':
        return `left: ${rect.left + rect.width / 2}px; top: ${rect.bottom + gap}px; transform: translateX(-50%);`;
      case 'left':
        return `left: ${rect.left - gap}px; top: ${rect.top + rect.height / 2}px; transform: translate(-100%, -50%);`;
      case 'right':
        return `left: ${rect.right + gap}px; top: ${rect.top + rect.height / 2}px; transform: translateY(-50%);`;
      default: // top
        return `left: ${rect.left + rect.width / 2}px; top: ${rect.top - gap}px; transform: translate(-50%, -100%);`;
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<span
  class="tooltip-wrapper"
  bind:this={wrapper}
  onmouseenter={show}
  onmouseleave={hide}
  onfocusin={show}
  onfocusout={hide}
>
  {@render children()}
</span>

{#if visible && text}
  <div class="tooltip tooltip-{position}" style={tooltipStyle} role="tooltip">
    {text}
    <span class="tooltip-arrow"></span>
  </div>
{/if}

<style>
  .tooltip-wrapper {
    display: inline-flex;
    align-items: center;
  }
  .tooltip {
    position: fixed;
    z-index: 9999;
    background: var(--bg-3);
    color: var(--text-0);
    font-size: 10px;
    font-family: var(--font-display);
    padding: 4px 8px;
    border-radius: 4px;
    white-space: nowrap;
    pointer-events: none;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.4);
    opacity: 0;
    animation: tooltipFadeIn 150ms ease forwards;
  }
  @keyframes tooltipFadeIn {
    to { opacity: 1; }
  }
  .tooltip-arrow {
    position: absolute;
    width: 6px;
    height: 6px;
    background: var(--bg-3);
    transform: rotate(45deg);
  }
  /* Arrow positioning per direction */
  .tooltip-top .tooltip-arrow {
    bottom: -3px;
    left: 50%;
    margin-left: -3px;
  }
  .tooltip-bottom .tooltip-arrow {
    top: -3px;
    left: 50%;
    margin-left: -3px;
  }
  .tooltip-left .tooltip-arrow {
    right: -3px;
    top: 50%;
    margin-top: -3px;
  }
  .tooltip-right .tooltip-arrow {
    left: -3px;
    top: 50%;
    margin-top: -3px;
  }
</style>
