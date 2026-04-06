<script lang="ts">
  interface Props {
    progress: number;  // 0–100
    step: string;
    visible: boolean;
  }
  let { progress, step, visible }: Props = $props();
</script>

{#if visible}
  <div class="loading-screen" class:fade-out={progress >= 100} aria-hidden={progress >= 100}>
    <div class="loading-content">
      <div class="logo">
        <span class="logo-j">J</span><span class="logo-rest">ARVIS</span>
      </div>
      <div class="subtitle">Mission Control</div>

      <div class="progress-wrap">
        <div class="progress-bar">
          <div class="progress-fill" style="width: {progress}%"></div>
        </div>
        <div class="progress-pct">{Math.round(progress)}%</div>
      </div>

      <div class="step-label">{step}</div>
    </div>
  </div>
{/if}

<style>
  .loading-screen {
    position: fixed;
    inset: 0;
    z-index: 9999;
    background: var(--bg-0);
    display: flex;
    align-items: center;
    justify-content: center;
    opacity: 1;
    transition: opacity 0.4s ease;
  }

  .loading-screen.fade-out {
    opacity: 0;
    pointer-events: none;
  }

  .loading-content {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    width: 280px;
  }

  .logo {
    font-family: var(--font-display);
    font-size: 2.8rem;
    font-weight: 700;
    letter-spacing: 0.12em;
    line-height: 1;
  }

  .logo-j {
    color: var(--cyan);
    text-shadow: 0 0 24px var(--cyan-glow);
  }

  .logo-rest {
    color: var(--text-0);
  }

  .subtitle {
    font-family: var(--font-mono);
    font-size: 0.65rem;
    letter-spacing: 0.3em;
    text-transform: uppercase;
    color: var(--text-2);
    margin-top: -8px;
    margin-bottom: 8px;
  }

  .progress-wrap {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .progress-bar {
    flex: 1;
    height: 3px;
    background: var(--bg-3);
    border-radius: 2px;
    overflow: hidden;
  }

  .progress-fill {
    height: 100%;
    background: var(--cyan);
    box-shadow: 0 0 8px var(--cyan-glow);
    border-radius: 2px;
    transition: width 0.3s ease;
  }

  .progress-pct {
    font-family: var(--font-mono);
    font-size: 0.65rem;
    color: var(--text-2);
    width: 30px;
    text-align: right;
    flex-shrink: 0;
  }

  .step-label {
    font-family: var(--font-mono);
    font-size: 0.65rem;
    color: var(--text-2);
    letter-spacing: 0.05em;
    min-height: 1.2em;
    text-align: center;
  }
</style>
