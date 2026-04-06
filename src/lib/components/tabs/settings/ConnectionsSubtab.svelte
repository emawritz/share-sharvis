<script lang="ts">
  import { checkMachineConnections, runFixCommand } from '../../../api';
  import { addToast } from '../../../stores/notifications';
  import { handleError } from '../../../utils';
  import { tr } from '$lib/i18n';
  import type { JarvisConfig, MachineConnections } from '../../../types';

  let { cfg }: { cfg: JarvisConfig } = $props();

  let connectionResults = $state<Record<string, MachineConnections>>({});
  let checkingAll = $state(false);

  // Fix panel state: { machineId, checkName } | null
  let fixPanel = $state<{ machineId: string; checkName: string } | null>(null);
  let fixRunning = $state(false);
  let fixOutput = $state('');

  // Fix commands per check name, keyed by machine OS
  function getFixCommand(checkName: string, os: string): { label: string; cmd: string } | null {
    const brew = 'brew install';
    const apt = 'sudo apt-get install -y';
    const mac = os === 'macos';
    switch (checkName) {
      case 'tailscale':
        return mac
          ? { label: 'Iniciar Tailscale', cmd: 'open -a Tailscale || brew install --cask tailscale && open -a Tailscale' }
          : { label: 'Iniciar Tailscale', cmd: 'sudo tailscale up' };
      case 'claude':
        return { label: 'Instalar Claude Code', cmd: 'npm install -g @anthropic-ai/claude-code' };
      case 'github':
        return mac
          ? { label: 'Instalar gh', cmd: `${brew} gh` }
          : { label: 'Instalar gh', cmd: `${apt} gh` };
      case 'node':
        return mac
          ? { label: 'Instalar Node.js', cmd: `${brew} node` }
          : { label: 'Instalar Node.js', cmd: `${apt} nodejs npm` };
      case 'git':
        return mac
          ? { label: 'Instalar Git', cmd: `${brew} git` }
          : { label: 'Instalar Git', cmd: `${apt} git` };
      case 'python':
        return mac
          ? { label: 'Instalar Python 3', cmd: 'brew install python3' }
          : { label: 'Instalar Python 3', cmd: 'sudo apt-get install -y python3 python3-pip' };
      case 'docker':
        return mac
          ? { label: 'Instalar Docker', cmd: 'brew install --cask docker' }
          : { label: 'Instalar Docker', cmd: 'curl -fsSL https://get.docker.com | sh' };
      case 'cargo':
        return { label: 'Instalar Rust/Cargo', cmd: 'curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y' };
      default:
        return null;
    }
  }

  async function checkAllConnections() {
    checkingAll = true;
    connectionResults = {};
    fixPanel = null;
    fixOutput = '';
    const promises = cfg.machines.map(async (m) => {
      try {
        const result = await checkMachineConnections(m.id);
        connectionResults = { ...connectionResults, [m.id]: result };
      } catch {
        // skip
      }
    });
    await Promise.all(promises);
    checkingAll = false;
  }

  function openFix(machineId: string, checkName: string) {
    fixPanel = { machineId, checkName };
    fixOutput = '';
  }

  function closeFix() {
    fixPanel = null;
    fixOutput = '';
  }

  async function runFix(machineId: string, cmd: string) {
    fixRunning = true;
    fixOutput = 'Ejecutando...';
    try {
      const out = await runFixCommand(machineId, cmd);
      fixOutput = out;
      addToast('Fix ejecutado', 'success');
    } catch (e) {
      fixOutput = '✗ ' + handleError(e);
    }
    fixRunning = false;
  }

  function copyCmd(cmd: string) {
    navigator.clipboard.writeText(cmd);
    addToast('Comando copiado', 'success');
  }

  function statusIcon(status: string): string {
    if (status === 'ok') return '\u2713';
    if (status === 'warning') return '\u26A0';
    return '\u2717';
  }

  function statusColor(status: string): string {
    if (status === 'ok') return 'var(--green)';
    if (status === 'warning') return '#ffb74d';
    return '#ef5350';
  }

  // Current machine for fix panel
  let fixMachine = $derived(fixPanel ? cfg.machines.find(m => m.id === fixPanel!.machineId) : null);
  let fixFix = $derived(
    fixPanel && fixMachine ? getFixCommand(fixPanel.checkName, fixMachine.os) : null
  );
</script>

<div class="connections-panel">
  <div class="connections-header">
    <button class="jarvis-btn" onclick={checkAllConnections} disabled={checkingAll}>
      {checkingAll ? $tr('settings.verifying') : $tr('settings.verifyAll')}
    </button>
  </div>
  {#if Object.keys(connectionResults).length === 0 && !checkingAll}
    <div class="connections-empty">{$tr('settings.verifyHint')}</div>
  {:else}
    <div class="connections-grid" style="grid-template-columns: 100px repeat({cfg.machines.length}, 1fr)">
      <div class="conn-header-cell"></div>
      {#each cfg.machines as m}
        <div class="conn-header-cell machine-name">{m.name}</div>
      {/each}

      {#each ['ssh', 'tailscale', 'claude', 'github', 'git', 'node', 'python', 'docker', 'cargo', 'disk', 'gpu'] as checkName}
        <div class="conn-row-label">{checkName.toUpperCase()}</div>
        {#each cfg.machines as m}
          {@const mc = connectionResults[m.id]}
          {@const check = mc?.checks.find((c: {name: string}) => c.name === checkName)}
          {@const hasFix = check && check.status !== 'ok' && getFixCommand(checkName, m.os) !== null}
          {@const isActive = fixPanel?.machineId === m.id && fixPanel?.checkName === checkName}
          <div class="conn-cell" class:cell-active={isActive} title={check?.detail || ''}>
            {#if !mc}
              <span class="conn-pending">&mdash;</span>
            {:else if check}
              <span class="conn-status" style="color: {statusColor(check.status)}">{statusIcon(check.status)}</span>
              <span class="conn-detail">{check.detail}</span>
              {#if hasFix}
                <button
                  class="fix-btn"
                  class:fix-btn-active={isActive}
                  onclick={() => isActive ? closeFix() : openFix(m.id, checkName)}
                  title="Ver solución"
                >Fix</button>
              {/if}
            {:else}
              <span class="conn-pending">&mdash;</span>
            {/if}
          </div>
        {/each}
      {/each}
    </div>

    {#if fixPanel && fixMachine && fixFix}
      <div class="fix-panel">
        <div class="fix-panel-header">
          <span class="fix-panel-title">⚙ Fix: {fixPanel.checkName.toUpperCase()} en {fixMachine.name}</span>
          <button class="fix-close" onclick={closeFix}>✕</button>
        </div>
        <div class="fix-panel-body">
          <div class="fix-label">{fixFix.label}</div>
          <div class="fix-cmd-row">
            <code class="fix-cmd">{fixFix.cmd}</code>
            <button class="fix-copy-btn" onclick={() => copyCmd(fixFix!.cmd)}>Copiar</button>
            <button class="fix-run-btn" onclick={() => runFix(fixPanel!.machineId, fixFix!.cmd)} disabled={fixRunning}>
              {fixRunning ? '...' : 'Ejecutar'}
            </button>
          </div>
          {#if fixOutput}
            <pre class="fix-output" class:fix-output-error={fixOutput.startsWith('✗')}>{fixOutput}</pre>
          {/if}
        </div>
      </div>
    {/if}
  {/if}
</div>

<style>
  .connections-panel { padding: 10px 14px; overflow: auto; flex: 1; display: flex; flex-direction: column; gap: 10px; }
  .connections-header { display: flex; gap: 8px; }
  .connections-empty { color: var(--text-3); font-size: 11px; text-align: center; padding: 20px; }
  .connections-grid {
    display: grid;
    gap: 1px;
    background: var(--border);
    border: 1px solid var(--border);
    border-radius: 5px;
    overflow: hidden;
    flex-shrink: 0;
  }
  .conn-header-cell {
    background: var(--bg-1);
    padding: 6px 8px;
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--text-1);
    text-align: center;
  }
  .conn-header-cell.machine-name { color: var(--cyan); }
  .conn-row-label {
    background: var(--bg-1);
    padding: 4px 8px;
    font-size: 9px;
    font-weight: 600;
    color: var(--text-2);
    letter-spacing: 0.5px;
    display: flex;
    align-items: center;
  }
  .conn-cell {
    background: var(--bg-0);
    padding: 4px 8px;
    font-size: 10px;
    display: flex;
    align-items: center;
    gap: 4px;
    overflow: hidden;
  }
  .conn-cell.cell-active { background: rgba(0, 212, 255, 0.06); }
  .conn-pending { color: var(--text-3); }
  .conn-status { font-weight: 700; flex-shrink: 0; }
  .conn-detail {
    color: var(--text-2);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    font-size: 9px;
    flex: 1;
    min-width: 0;
  }

  /* Fix button inline */
  .fix-btn {
    flex-shrink: 0;
    font-size: 8px;
    font-family: var(--font-display);
    font-weight: 700;
    letter-spacing: 0.5px;
    color: #ef5350;
    background: rgba(239, 83, 80, 0.12);
    border: 1px solid rgba(239, 83, 80, 0.3);
    border-radius: 2px;
    padding: 1px 5px;
    cursor: pointer;
    transition: all 0.15s;
  }
  .fix-btn:hover, .fix-btn.fix-btn-active {
    background: rgba(239, 83, 80, 0.25);
    border-color: rgba(239, 83, 80, 0.6);
    color: #ff6b68;
  }

  /* Fix panel */
  .fix-panel {
    border: 1px solid rgba(0, 212, 255, 0.25);
    border-radius: 5px;
    background: var(--bg-1);
    overflow: hidden;
    flex-shrink: 0;
  }
  .fix-panel-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 7px 12px;
    background: var(--bg-2);
    border-bottom: 1px solid var(--border);
  }
  .fix-panel-title {
    font-size: 10px;
    font-family: var(--font-display);
    font-weight: 700;
    color: var(--cyan);
    letter-spacing: 0.5px;
    text-transform: uppercase;
  }
  .fix-close {
    font-size: 10px;
    color: var(--text-3);
    background: none;
    border: none;
    cursor: pointer;
    padding: 0 2px;
  }
  .fix-close:hover { color: var(--text-0); }
  .fix-panel-body { padding: 10px 12px; display: flex; flex-direction: column; gap: 8px; }
  .fix-label { font-size: 11px; color: var(--text-1); }
  .fix-cmd-row { display: flex; align-items: center; gap: 6px; }
  .fix-cmd {
    flex: 1;
    font-family: var(--font-mono, monospace);
    font-size: 10px;
    color: var(--text-0);
    background: var(--bg-0);
    border: 1px solid var(--border);
    border-radius: 3px;
    padding: 4px 8px;
    overflow-x: auto;
    white-space: nowrap;
  }
  .fix-copy-btn, .fix-run-btn {
    flex-shrink: 0;
    font-size: 9px;
    font-family: var(--font-display);
    font-weight: 600;
    border-radius: 3px;
    padding: 3px 9px;
    cursor: pointer;
    transition: all 0.15s;
    letter-spacing: 0.5px;
  }
  .fix-copy-btn {
    background: var(--bg-3);
    border: 1px solid var(--border);
    color: var(--text-1);
  }
  .fix-copy-btn:hover { background: var(--bg-2); color: var(--text-0); }
  .fix-run-btn {
    background: rgba(0, 212, 255, 0.12);
    border: 1px solid rgba(0, 212, 255, 0.3);
    color: var(--cyan);
  }
  .fix-run-btn:hover:not(:disabled) { background: rgba(0, 212, 255, 0.22); }
  .fix-run-btn:disabled { opacity: 0.5; cursor: not-allowed; }
  .fix-output {
    font-family: var(--font-mono, monospace);
    font-size: 9px;
    color: var(--green);
    background: var(--bg-0);
    border: 1px solid var(--border);
    border-radius: 3px;
    padding: 6px 8px;
    margin: 0;
    white-space: pre-wrap;
    word-break: break-all;
    max-height: 120px;
    overflow-y: auto;
  }
  .fix-output.fix-output-error { color: #ef5350; }
</style>
