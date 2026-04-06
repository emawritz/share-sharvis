<script lang="ts">
  import type { MachineStats } from '../types';

  interface MetricsData { stats: MachineStats; tokens?: { input: number; output: number; total: number } }
  let { metrics, hasGpu = false }: { metrics: MetricsData | null; hasGpu?: boolean } = $props();

  function pctClass(val: string | undefined): string {
    const n = parseInt(val || '');
    if (isNaN(n)) return 'off';
    if (n >= 90) return 'crit';
    if (n >= 70) return 'warn';
    return 'ok';
  }

  function fmtTokens(n: number): string {
    if (n >= 1000000) return (n / 1000000).toFixed(1) + 'M';
    if (n >= 1000) return (n / 1000).toFixed(0) + 'K';
    return String(n);
  }

  let isOnline = $derived(metrics?.stats?.online ?? false);
  let stats = $derived(metrics?.stats);
  let tokens = $derived(metrics?.tokens);

  let statPairs = $derived.by(() => {
    if (!stats) return [];
    const pairs: [string, string | undefined][] = [
      ['CPU', stats.cpu],
      ['MEM', stats.mem],
      ['DSK', stats.disk]
    ];
    if (hasGpu && stats.gpu && stats.gpu !== 'n/a') {
      pairs.push(['GPU', stats.gpu]);
    }
    return pairs;
  });
</script>

<div class="agent-metrics" aria-label="Metricas">
  <span class="online-badge" class:on={isOnline} class:off={!isOnline}>
    {isOnline ? 'Online' : 'Offline'}
  </span>
  {#if isOnline}
    {#each statPairs as [label, val]}
      <span class="metric">
        <span class="metric-label">{label}</span>
        <span class="metric-val {pctClass(val)}">{val || '-'}</span>
      </span>
    {/each}
    <span class="metric-sep"></span>
    {#if tokens}
      <span
        class="token-count"
        title="{tokens.input.toLocaleString()} in / {tokens.output.toLocaleString()} out"
      >
        <span class="tok-label">TKN</span>
        <span class="tok-val">{fmtTokens(tokens.total)}</span>
      </span>
    {/if}
  {/if}
</div>

<style>
  .agent-metrics {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-left: auto;
    font-size: 10px;
    font-variant-numeric: tabular-nums;
  }
  .metric {
    display: flex;
    align-items: center;
    gap: 4px;
    color: var(--text-2);
  }
  .metric-label {
    color: var(--text-3);
  }
  .metric-val { font-weight: 500; }
  .metric-val.ok { color: var(--green); }
  .metric-val.warn { color: var(--amber); }
  .metric-val.crit { color: var(--red); }
  .metric-val.off { color: var(--text-3); }
  .metric-sep {
    width: 1px;
    height: 14px;
    background: var(--border);
  }
  .token-count {
    display: flex;
    align-items: center;
    gap: 4px;
    color: var(--text-2);
    font-family: var(--font-display);
    font-size: 9px;
    letter-spacing: 0.5px;
  }
  .tok-label { color: var(--text-3); }
  .tok-val { color: var(--cyan); font-weight: 600; }
  .online-badge {
    font-family: var(--font-display);
    font-size: 8px;
    font-weight: 700;
    padding: 1px 6px;
    border-radius: 3px;
    letter-spacing: 1px;
    text-transform: uppercase;
  }
  .online-badge.on { background: var(--green-dim); color: var(--green); border: 1px solid #00ff8833; }
  .online-badge.off { background: #ff335518; color: var(--red); border: 1px solid #ff335533; }
</style>
