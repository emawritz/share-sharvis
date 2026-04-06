<script lang="ts">
  interface Props {
    width?: string;
    height?: string;
    variant?: 'text' | 'card' | 'circle';
    count?: number;
  }

  let { width = '100%', height = '14px', variant = 'text', count = 1 }: Props = $props();

  const radiusMap = { text: '4px', card: '8px', circle: '50%' };
</script>

<div class="skeleton-wrapper">
  {#each Array.from({ length: count }) as _, i}
    <div
      class="skeleton-bar"
      style="width:{width}; height:{height}; border-radius:{radiusMap[variant]}"
    ></div>
  {/each}
</div>

<style>
  .skeleton-wrapper {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .skeleton-bar {
    background: var(--bg-2);
    position: relative;
    overflow: hidden;
  }

  .skeleton-bar::after {
    content: '';
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background: linear-gradient(
      90deg,
      transparent 0%,
      var(--bg-3) 50%,
      transparent 100%
    );
    animation: shimmer 1.5s infinite;
  }

  @keyframes shimmer {
    0% {
      transform: translateX(-100%);
    }
    100% {
      transform: translateX(100%);
    }
  }
</style>
