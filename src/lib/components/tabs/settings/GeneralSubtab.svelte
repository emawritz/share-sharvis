<script lang="ts">
  import { onMount } from 'svelte';
  import { saveJarvisConfig, executeAction, saveTeamMemory, getTeamMemories, deleteTeamMemory, getNotificationsEnabled, setNotificationsEnabled, checkForUpdate, installUpdate, exportConfig as exportConfigApi, importConfig, getAppVersion, getEnvironmentInfo, getDbStats, vacuumDb, getNotificationHistory } from '../../../api';
  import type { AppVersion, EnvironmentInfo, DbStats, NotifHistoryEntry } from '../../../api';
  import { addToast, getNotifPrefs, saveNotifPrefs, soundVolume, toastHistory, clearToastHistory } from '../../../stores/notifications';
  import type { NotificationPrefs, ToastHistoryEntry } from '../../../stores/notifications';
  import { handleError, exportJson } from '../../../utils';
  import { t, tr } from '$lib/i18n';
  import type { JarvisConfig, TeamMemory } from '../../../types';
  import SnapshotsSection from './SnapshotsSection.svelte';
  import RulesSection from './RulesSection.svelte';
  import WebhooksSection from './WebhooksSection.svelte';
  import ConfirmModal from '../../ConfirmModal.svelte';
  import { budgetLimit, saveBudgetLimit } from '../../../stores/tokens';

  let { cfg, onConfigReload }: { cfg: JarvisConfig; onConfigReload: () => Promise<void> } = $props();

  let saving = $state(false);
  let nativeNotifs = $state(true);
  let budgetInput = $state('');

  // Updates state
  let checkingUpdate = $state(false);
  let installingUpdate = $state(false);
  let availableVersion = $state<string | null>(null);
  let updateChecked = $state(false);

  // Backup state
  let exportingConfig = $state(false);
  let importingConfig = $state(false);

  // ConfirmModal state
  let showResetConfirm = $state(false);
  let showClearHistoryConfirm = $state(false);
  let showKillAgentsConfirm = $state(false);
  let notifPrefs = $state<NotificationPrefs>(getNotifPrefs());
  let volumePct = $state(Math.round(70)); // synced below via store

  // Team memory state
  let memories = $state<TeamMemory[]>([]);
  let newMemory = $state('');
  let newMemoryTags = $state('');

  // About / system info state
  let appVersion = $state<AppVersion | null>(null);
  let envInfo = $state<EnvironmentInfo | null>(null);
  let systemInfoOpen = $state(false);

  // Database section state
  let dbStats = $state<DbStats | null>(null);
  let dbStatsLoading = $state(false);
  let vacuumingDb = $state(false);

  // Notification history (backend ring buffer) state
  let notifHistory = $state<NotifHistoryEntry[]>([]);
  let notifHistoryOpen = $state(false);
  let notifHistoryLoading = $state(false);

  let machineCount = $derived(cfg?.machines.length ?? 0);
  let repoCount = $derived(cfg?.machines.reduce((sum, m) => sum + m.repos.length, 0) ?? 0);

  onMount(() => {
    loadMemories();
    loadNotifSetting();
    loadAppInfo();
    // Sync budget input from store
    const unsubBudget = budgetLimit.subscribe(val => {
      budgetInput = val != null ? String(val) : '';
    });
    // Sync volume pct from store
    const unsubVol = soundVolume.subscribe(val => {
      volumePct = Math.round(val * 100);
    });
    return () => { unsubBudget(); unsubVol(); };
  });

  async function loadAppInfo() {
    try { appVersion = await getAppVersion(); } catch { /* ignore */ }
    try { envInfo = await getEnvironmentInfo(); } catch { /* ignore */ }
  }

  async function loadDbStats() {
    dbStatsLoading = true;
    try { dbStats = await getDbStats(); } catch (e) { addToast('DB stats error: ' + handleError(e), 'error'); }
    dbStatsLoading = false;
  }

  async function handleVacuumDb() {
    vacuumingDb = true;
    try {
      await vacuumDb();
      addToast('Database vacuumed successfully.', 'success');
      await loadDbStats();
    } catch (e) { addToast('Vacuum failed: ' + handleError(e), 'error'); }
    vacuumingDb = false;
  }

  async function loadNotifHistory() {
    notifHistoryLoading = true;
    try { notifHistory = await getNotificationHistory(); } catch { notifHistory = []; }
    notifHistoryLoading = false;
  }

  function toggleNotifHistoryOpen() {
    notifHistoryOpen = !notifHistoryOpen;
    if (notifHistoryOpen && notifHistory.length === 0) loadNotifHistory();
  }

  function toggleSystemInfoOpen() {
    systemInfoOpen = !systemInfoOpen;
  }

  function toggleDbSection() {
    if (!dbStats) loadDbStats();
  }

  function formatTimestamp(ts: string): string {
    try {
      const d = new Date(ts);
      return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });
    } catch { return ts; }
  }

  const LEVEL_LABELS: Record<string, string> = {
    task_complete: 'DONE',
    task_error: 'ERR',
    machine_offline: 'OFFL',
    cron_fired: 'CRON',
    info: 'INFO',
  };

  function handleVolumeChange(e: Event) {
    const val = parseInt((e.target as HTMLInputElement).value, 10);
    volumePct = val;
    soundVolume.set(val / 100);
  }

  function relativeTime(ts: number): string {
    const diffSecs = Math.floor((Date.now() - ts) / 1000);
    if (diffSecs < 60) return `${diffSecs}s ago`;
    const diffMins = Math.floor(diffSecs / 60);
    if (diffMins < 60) return `${diffMins}m ago`;
    const diffHrs = Math.floor(diffMins / 60);
    return `${diffHrs}h ago`;
  }

  const TYPE_LABELS: Record<string, string> = {
    success: 'OK',
    error: 'ERR',
    info: 'INFO',
    warning: 'WARN',
  };

  async function save() {
    saving = true;
    try {
      await saveJarvisConfig(cfg);
      addToast(t('settings.configSaved'), 'success');
    } catch (e) {
      addToast('Error: ' + handleError(e), 'error');
    }
    saving = false;
  }

  async function loadMemories() {
    try { memories = await getTeamMemories(); } catch { memories = []; }
  }

  async function handleSaveMemory() {
    if (!newMemory.trim()) { addToast(t('settings.enterContent'), 'error'); return; }
    try {
      const tags = newMemoryTags.split(',').map(t => t.trim()).filter(Boolean);
      await saveTeamMemory(newMemory.trim(), tags);
      newMemory = '';
      newMemoryTags = '';
      await loadMemories();
      addToast(t('settings.memorySaved'), 'success');
    } catch (e) { addToast('Error: ' + handleError(e), 'error'); }
  }

  async function handleDeleteMemory(id: number) {
    try {
      await deleteTeamMemory(id);
      await loadMemories();
    } catch (e) { addToast('Error: ' + handleError(e), 'error'); }
  }

  async function loadNotifSetting() {
    try { nativeNotifs = await getNotificationsEnabled(); } catch { /* ignore */ }
  }

  async function toggleNotifs() {
    try {
      nativeNotifs = !nativeNotifs;
      await setNotificationsEnabled(nativeNotifs);
      addToast(nativeNotifs ? t('settings.enableNotifications') : t('settings.disableNotifications'), 'info');
    } catch (e) { addToast('Error: ' + handleError(e), 'error'); }
  }

  function togglePref(key: keyof NotificationPrefs) {
    notifPrefs[key] = !notifPrefs[key];
    saveNotifPrefs(notifPrefs);
  }

  async function handleCheckForUpdate() {
    checkingUpdate = true;
    updateChecked = false;
    availableVersion = null;
    try {
      const version = await checkForUpdate();
      availableVersion = version;
      updateChecked = true;
      if (!version) addToast('Already up to date.', 'success');
    } catch (e) {
      addToast('Update check failed: ' + handleError(e), 'error');
    }
    checkingUpdate = false;
  }

  async function handleInstallUpdate() {
    installingUpdate = true;
    try {
      await installUpdate();
      addToast('Update installed. Restart JARVIS to apply.', 'success');
    } catch (e) {
      addToast('Install failed: ' + handleError(e), 'error');
    }
    installingUpdate = false;
  }

  async function handleExportConfig() {
    exportingConfig = true;
    try {
      const data = await exportConfigApi();
      const date = new Date().toISOString().slice(0, 10);
      exportJson(data, `jarvis-backup-${date}.json`);
      addToast('Config exported.', 'success');
    } catch (e) {
      addToast('Export failed: ' + handleError(e), 'error');
    }
    exportingConfig = false;
  }

  async function handleImportConfig(e: Event) {
    const file = (e.target as HTMLInputElement).files?.[0];
    if (!file) return;
    importingConfig = true;
    try {
      const text = await file.text();
      await importConfig(text);
      addToast('Config imported. Restart JARVIS or reload settings to apply.', 'success');
      await onConfigReload();
    } catch (err) {
      addToast('Import failed: ' + handleError(err), 'error');
    }
    importingConfig = false;
    // Reset the file input
    (e.target as HTMLInputElement).value = '';
  }

  async function resetConfig() {
    try {
      const freshConfig: JarvisConfig = {
        session: { id: '', rama: '', objetivo: '' },
        machines: []
      };
      await saveJarvisConfig(freshConfig);
      addToast(t('settings.configReset'), 'success');
      await onConfigReload();
    } catch (e) {
      addToast(t('common.error') + ': ' + handleError(e), 'error');
    }
    showResetConfirm = false;
  }

  async function clearHistory() {
    try {
      await executeAction('clear-history');
      addToast(t('settings.historyClearedMsg'), 'success');
    } catch (e) {
      addToast(t('common.error') + ': ' + handleError(e), 'error');
    }
    showClearHistoryConfirm = false;
  }

  async function killAllAgents() {
    try {
      await executeAction('kill-all');
      addToast(t('general.agentsKilled'), 'success');
    } catch (e) {
      addToast(t('common.error') + ': ' + handleError(e), 'error');
    }
    showKillAgentsConfirm = false;
  }
</script>

<div class="general-panel">
  <label class="jarvis-label">{$tr('general.sessionId')} <input class="jarvis-input" type="text" bind:value={cfg.session.id} /></label>
  <label class="jarvis-label">{$tr('general.branch')} <input class="jarvis-input" type="text" bind:value={cfg.session.rama} /></label>
  <label class="jarvis-label">{$tr('general.objective')} <textarea class="jarvis-input" bind:value={cfg.session.objetivo}></textarea></label>
  <button class="jarvis-btn jarvis-btn-primary" onclick={save} disabled={saving}>{saving ? $tr('common.saving') : $tr('common.save')}</button>

  <hr class="section-divider" />

  <div class="section-title">{$tr('general.appInfo')}</div>
  <div class="info-grid">
    <div class="info-item">
      <span class="info-label">{$tr('general.version')}</span>
      <span class="info-value">{appVersion ? `${appVersion.major}.${appVersion.minor}.${appVersion.patch}` : '—'}</span>
    </div>
    <div class="info-item">
      <span class="info-label">{$tr('general.machines')}</span>
      <span class="info-value">{machineCount}</span>
    </div>
    <div class="info-item">
      <span class="info-label">{$tr('general.repos')}</span>
      <span class="info-value">{repoCount}</span>
    </div>
    {#if envInfo}
      <div class="info-item">
        <span class="info-label">Platform</span>
        <span class="info-value">{envInfo.platform}</span>
      </div>
      <div class="info-item">
        <span class="info-label">Arch</span>
        <span class="info-value">{envInfo.arch}</span>
      </div>
      <div class="info-item">
        <span class="info-label">Host</span>
        <span class="info-value">{envInfo.hostname}</span>
      </div>
    {/if}
  </div>

  <hr class="section-divider" />

  <!-- Config file info -->
  <div class="section-title">Archivo de configuración</div>
  <div class="config-file-row">
    <span class="config-file-path">~/.config/jarvis/config.toml</span>
    <div class="config-file-actions">
      <button
        class="jarvis-btn jarvis-btn-sm"
        title="Copiar ruta al portapapeles"
        onclick={() => {
          navigator.clipboard.writeText('~/.config/jarvis/config.toml');
          addToast('Ruta copiada al portapapeles', 'info');
        }}
      >Copiar ruta</button>
      <button
        class="jarvis-btn jarvis-btn-sm"
        onclick={handleExportConfig}
        disabled={exportingConfig}
      >{exportingConfig ? 'Exportando...' : 'Exportar config'}</button>
    </div>
  </div>

  <hr class="section-divider" />

  <!-- Keyboard shortcuts reference -->
  <div class="section-title">Atajos de teclado</div>
  <div class="shortcuts-table">
    <div class="shortcut-row">
      <span class="shortcut-keys"><kbd>Ctrl+K</kbd> / <kbd>Cmd+K</kbd></span>
      <span class="shortcut-desc">Buscar</span>
    </div>
    <div class="shortcut-row">
      <span class="shortcut-keys"><kbd>Ctrl+P</kbd> / <kbd>Cmd+P</kbd></span>
      <span class="shortcut-desc">Planificación</span>
    </div>
    <div class="shortcut-row">
      <span class="shortcut-keys"><kbd>Ctrl+/</kbd></span>
      <span class="shortcut-desc">Atajos de teclado</span>
    </div>
    <div class="shortcut-row">
      <span class="shortcut-keys"><kbd>Escape</kbd></span>
      <span class="shortcut-desc">Cerrar modales</span>
    </div>
  </div>

  <hr class="section-divider" />

  <!-- About -->
  <div class="section-title">Acerca de</div>
  <div class="about-block">
    <span class="about-name">JARVIS</span>
    <span class="about-version">v{appVersion ? `${appVersion.major}.${appVersion.minor}.${appVersion.patch}` : '…'}</span>
    <span class="about-desc">Multi-Agent Mission Control</span>
    <button
      class="jarvis-btn jarvis-btn-sm expand-btn"
      onclick={toggleSystemInfoOpen}
      title="Toggle system info"
    >{systemInfoOpen ? '▲ Less' : '▼ System Info'}</button>
  </div>

  {#if systemInfoOpen}
    <div class="sysinfo-grid">
      <div class="sysinfo-row">
        <span class="sysinfo-label">Platform</span>
        <span class="sysinfo-value">{envInfo?.platform ?? '—'}</span>
      </div>
      <div class="sysinfo-row">
        <span class="sysinfo-label">Architecture</span>
        <span class="sysinfo-value">{envInfo?.arch ?? '—'}</span>
      </div>
      <div class="sysinfo-row">
        <span class="sysinfo-label">Hostname</span>
        <span class="sysinfo-value">{envInfo?.hostname ?? '—'}</span>
      </div>
      <div class="sysinfo-row">
        <span class="sysinfo-label">OS Version</span>
        <span class="sysinfo-value">{envInfo?.osVersion ?? '—'}</span>
      </div>
      <div class="sysinfo-row">
        <span class="sysinfo-label">Rust Edition</span>
        <span class="sysinfo-value">{envInfo?.rustEdition ?? '—'}</span>
      </div>
      <div class="sysinfo-row">
        <span class="sysinfo-label">App Version</span>
        <span class="sysinfo-value">{envInfo?.cargoVersion ?? '—'}</span>
      </div>
    </div>
  {/if}

  <hr class="section-divider" />

  <!-- Database -->
  <div class="section-title" onclick={toggleDbSection} style="cursor:pointer; user-select:none;">Database</div>
  <div class="db-section">
    <div class="db-actions">
      <button class="jarvis-btn" onclick={loadDbStats} disabled={dbStatsLoading}>
        {dbStatsLoading ? 'Loading…' : 'Refresh stats'}
      </button>
      <button class="jarvis-btn jarvis-btn-primary" onclick={handleVacuumDb} disabled={vacuumingDb || dbStatsLoading}>
        {vacuumingDb ? 'Vacuuming…' : 'Vacuum DB'}
      </button>
    </div>
    {#if dbStats}
      <div class="db-stats">
        <div class="db-stat-total">
          <span class="sysinfo-label">Total size</span>
          <span class="sysinfo-value">{dbStats.total_size_kb} KB</span>
        </div>
        <div class="db-tables">
          {#each dbStats.tables as tbl}
            <div class="db-table-row">
              <span class="db-table-name">{tbl.name}</span>
              <span class="db-table-rows">{tbl.row_count} rows</span>
            </div>
          {/each}
        </div>
        <div class="db-path">{dbStats.db_path}</div>
      </div>
    {:else if !dbStatsLoading}
      <div class="snapshots-empty">Click "Refresh stats" to load database info.</div>
    {/if}
  </div>

  <hr class="section-divider" />

  <!-- Notification History (backend ring buffer) -->
  <div class="section-title-collapsible">
    <span class="section-title" style="margin-bottom:0">Notification History (backend)</span>
    <button class="jarvis-btn jarvis-btn-sm expand-btn" onclick={toggleNotifHistoryOpen}>
      {notifHistoryOpen ? '▲ Hide' : '▼ Show'}
    </button>
  </div>
  {#if notifHistoryOpen}
    {#if notifHistoryLoading}
      <div class="snapshots-empty">Loading…</div>
    {:else if notifHistory.length === 0}
      <div class="snapshots-empty">No notifications recorded yet.</div>
    {:else}
      <div class="notif-history-list">
        {#each [...notifHistory].reverse().slice(0, 10) as entry (entry.id)}
          <div class="notif-history-row">
            <span class="notif-time">{formatTimestamp(entry.timestamp)}</span>
            <span class="notif-badge notif-badge-{entry.level}">{LEVEL_LABELS[entry.level] ?? entry.level}</span>
            <span class="notif-msg" title="{entry.body}">{entry.title}{entry.body ? ' — ' + entry.body : ''}</span>
          </div>
        {/each}
      </div>
      <button class="jarvis-btn jarvis-btn-sm" onclick={loadNotifHistory}>Refresh</button>
    {/if}
  {/if}

  <hr class="section-divider" />

  <SnapshotsSection {onConfigReload} />

  <hr class="section-divider" />

  <div class="section-title">{$tr('general.nativeNotifications')}</div>
  <div class="setting-row">
    <span class="setting-label">{$tr('general.osNotifications')}</span>
    <label class="toggle-switch">
      <input type="checkbox" checked={nativeNotifs} onchange={toggleNotifs} />
      <span class="toggle-slider"></span>
    </label>
  </div>

  <div class="notif-prefs-grid">
    <label class="notif-pref-item">
      <input type="checkbox" checked={notifPrefs.taskComplete} onchange={() => togglePref('taskComplete')} />
      <span class="notif-pref-label">{$tr('general.taskComplete')}</span>
    </label>
    <label class="notif-pref-item">
      <input type="checkbox" checked={notifPrefs.taskError} onchange={() => togglePref('taskError')} />
      <span class="notif-pref-label">{$tr('general.taskError')}</span>
    </label>
    <label class="notif-pref-item">
      <input type="checkbox" checked={notifPrefs.planningDone} onchange={() => togglePref('planningDone')} />
      <span class="notif-pref-label">{$tr('general.planningDone')}</span>
    </label>
    <label class="notif-pref-item">
      <input type="checkbox" checked={notifPrefs.conflictAlert} onchange={() => togglePref('conflictAlert')} />
      <span class="notif-pref-label">{$tr('general.repoConflict')}</span>
    </label>
    <label class="notif-pref-item">
      <input type="checkbox" checked={notifPrefs.soundEnabled} onchange={() => togglePref('soundEnabled')} />
      <span class="notif-pref-label">{$tr('general.sound')}</span>
    </label>
  </div>

  {#if notifPrefs.soundEnabled}
    <div class="volume-row">
      <span class="setting-label">Volume</span>
      <div class="volume-control">
        <input
          type="range"
          min="0"
          max="100"
          value={volumePct}
          oninput={handleVolumeChange}
          class="volume-slider"
          aria-label="Sound volume"
        />
        <span class="volume-pct">{volumePct}%</span>
      </div>
    </div>
  {/if}

  <div class="section-title notif-history-title">Notification History</div>
  {#if $toastHistory.length === 0}
    <div class="snapshots-empty">No notifications yet.</div>
  {:else}
    <div class="notif-history-list">
      {#each [...$toastHistory].reverse().slice(0, 10) as entry (entry.id)}
        <div class="notif-history-row">
          <span class="notif-time">{relativeTime(entry.timestamp)}</span>
          <span class="notif-badge notif-badge-{entry.type}">{TYPE_LABELS[entry.type] ?? entry.type}</span>
          <span class="notif-msg">{entry.message}</span>
        </div>
      {/each}
    </div>
    <button class="jarvis-btn jarvis-btn-sm" onclick={clearToastHistory}>Clear history</button>
  {/if}

  <div class="budget-row">
    <div class="budget-label-col">
      <span class="setting-label">Budget limit (USD)</span>
      <span class="budget-hint">Leave empty to disable</span>
    </div>
    <div class="budget-input-row">
      <span class="budget-dollar">$</span>
      <input
        class="jarvis-input budget-limit-input"
        type="number"
        min="0"
        step="1"
        placeholder="—"
        bind:value={budgetInput}
      />
      <button
        class="jarvis-btn jarvis-btn-primary"
        onclick={async () => {
          const val = budgetInput.trim() === '' ? null : parseFloat(budgetInput);
          await saveBudgetLimit(isNaN(val as number) ? null : val);
          addToast(val != null ? `Budget set to $${(val as number).toFixed(2)}` : 'Budget limit removed', 'success');
        }}
      >Set</button>
    </div>
  </div>

  <hr class="section-divider" />

  <RulesSection />

  <hr class="section-divider" />

  <div class="section-title memory-title">{$tr('general.teamMemory')}</div>
  <div class="memory-add-row">
    <input type="text" bind:value={newMemory} placeholder={$tr('general.memoryPlaceholder')} class="jarvis-input snapshot-input" />
    <input type="text" bind:value={newMemoryTags} placeholder={$tr('general.tagsPlaceholder')} class="jarvis-input memory-tags-input" style="max-width:120px" />
    <button class="jarvis-btn jarvis-btn-primary" onclick={handleSaveMemory}>{$tr('common.save')}</button>
  </div>
  {#if memories.length > 0}
    <div class="memories-list">
      {#each memories as mem}
        <div class="memory-row">
          <div class="memory-info">
            <span class="memory-content">{mem.content}</span>
            {#if mem.tags.length > 0}
              <span class="memory-tags">{mem.tags.join(', ')}</span>
            {/if}
          </div>
          <button class="jarvis-btn-remove" onclick={() => handleDeleteMemory(mem.id)}>x</button>
        </div>
      {/each}
    </div>
  {:else}
    <div class="snapshots-empty">{$tr('general.noMemories')}</div>
  {/if}

  <hr class="section-divider" />

  <WebhooksSection />

  <hr class="section-divider" />

  <div class="section-title">Updates</div>
  <div class="update-row">
    <button class="jarvis-btn" onclick={handleCheckForUpdate} disabled={checkingUpdate || installingUpdate}>
      {checkingUpdate ? 'Checking...' : 'Check for updates'}
    </button>
    {#if updateChecked && availableVersion === null}
      <span class="update-status update-ok">Up to date</span>
    {/if}
    {#if availableVersion !== null}
      <span class="update-status update-available">Version {availableVersion} available</span>
      <button class="jarvis-btn jarvis-btn-primary" onclick={handleInstallUpdate} disabled={installingUpdate}>
        {installingUpdate ? 'Installing...' : 'Install'}
      </button>
    {/if}
  </div>

  <hr class="section-divider" />

  <div class="section-title">Backup</div>
  <div class="action-row backup-row">
    <button class="jarvis-btn" onclick={handleExportConfig} disabled={exportingConfig}>
      {exportingConfig ? 'Exporting...' : 'Export config'}
    </button>
    <label class="jarvis-btn import-label" class:disabled={importingConfig}>
      {importingConfig ? 'Importing...' : 'Import config'}
      <input
        type="file"
        accept=".json"
        class="import-file-input"
        onchange={handleImportConfig}
        disabled={importingConfig}
      />
    </label>
    <button class="jarvis-btn jarvis-btn-danger" onclick={() => showResetConfirm = true}>{$tr('general.resetConfig')}</button>
  </div>

  <hr class="section-divider" />

  <div class="section-title danger-title">{$tr('general.dangerZone')}</div>
  <div class="danger-zone">
    <div class="danger-row">
      <div class="danger-info">
        <span class="danger-label">{$tr('general.clearHistoryTitle')}</span>
        <span class="danger-desc">{$tr('general.clearHistoryDesc')}</span>
      </div>
      <button class="jarvis-btn jarvis-btn-danger" onclick={() => showClearHistoryConfirm = true}>{$tr('general.clearHistory')}</button>
    </div>
    <div class="danger-row">
      <div class="danger-info">
        <span class="danger-label">{$tr('general.killAllAgentsTitle')}</span>
        <span class="danger-desc">{$tr('general.killAllAgentsDesc')}</span>
      </div>
      <button class="jarvis-btn jarvis-btn-danger" onclick={() => showKillAgentsConfirm = true}>{$tr('general.killAgents')}</button>
    </div>
  </div>
</div>

<ConfirmModal
  open={showResetConfirm}
  title={$tr('general.resetConfig')}
  message={$tr('general.resetConfigConfirm')}
  confirmText={$tr('common.confirm')}
  cancelText={$tr('common.cancel')}
  onConfirm={resetConfig}
  onCancel={() => showResetConfirm = false}
  variant="danger"
/>

<ConfirmModal
  open={showClearHistoryConfirm}
  title={$tr('general.clearHistory')}
  message={$tr('general.clearHistoryConfirm')}
  confirmText={$tr('common.confirm')}
  cancelText={$tr('common.cancel')}
  onConfirm={clearHistory}
  onCancel={() => showClearHistoryConfirm = false}
  variant="danger"
/>

<ConfirmModal
  open={showKillAgentsConfirm}
  title={$tr('general.killAgents')}
  message={$tr('general.killAgentsConfirm')}
  confirmText={$tr('common.confirm')}
  cancelText={$tr('common.cancel')}
  onConfirm={killAllAgents}
  onCancel={() => showKillAgentsConfirm = false}
  variant="danger"
/>

<style>
  .general-panel { padding: 10px 14px; display: flex; flex-direction: column; gap: 8px; overflow: auto; flex: 1; }
  .general-panel :global(textarea.jarvis-input) { min-height: 50px; resize: vertical; }

  .section-divider {
    border: none;
    border-top: 1px solid var(--border);
    margin: 4px 0;
  }
  .section-title {
    font-size: 9px;
    font-family: var(--font-display);
    text-transform: uppercase;
    letter-spacing: 1px;
    font-weight: 600;
    color: var(--text-2);
    margin-bottom: 2px;
  }
  .section-title.danger-title { color: #ef5350; }
  .info-grid {
    display: flex;
    gap: 16px;
  }
  .info-item {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .info-label {
    font-size: 9px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--text-3);
  }
  .info-value {
    font-size: 12px;
    font-family: var(--font-mono, monospace);
    color: var(--text-0);
    font-weight: 600;
  }
  .action-row {
    display: flex;
    gap: 8px;
  }
  .danger-zone {
    background: #f4433608;
    border: 1px solid #f4433622;
    border-radius: 6px;
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .danger-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }
  .danger-info {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .danger-label {
    font-size: 11px;
    font-weight: 600;
    color: var(--text-1);
  }
  .danger-desc {
    font-size: 9px;
    color: var(--text-3);
  }

  .snapshot-input { flex: 1; }
  .snapshots-empty {
    font-size: 10px;
    color: var(--text-3);
    padding: 8px 0;
  }

  /* Team Memory */
  .memory-title { color: var(--green); }
  .memory-add-row { display: flex; gap: 6px; align-items: center; }
  .memories-list { display: flex; flex-direction: column; gap: 3px; margin-top: 4px; }
  .memory-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 8px;
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 4px;
  }
  .memory-info { flex: 1; display: flex; flex-direction: column; gap: 1px; }
  .memory-content { font-size: 11px; color: var(--text-0); }
  .memory-tags { font-size: 8px; color: var(--text-3); }

  /* Notifications toggle */
  .setting-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 4px 0;
  }
  .setting-label { font-size: 11px; color: var(--text-1); }
  .toggle-switch { position: relative; width: 32px; height: 16px; display: inline-block; }
  .toggle-switch input { opacity: 0; width: 0; height: 0; }
  .toggle-slider {
    position: absolute;
    cursor: pointer;
    inset: 0;
    background: var(--bg-3);
    border-radius: 8px;
    transition: 0.2s;
  }
  .toggle-slider::before {
    content: '';
    position: absolute;
    width: 12px;
    height: 12px;
    left: 2px;
    bottom: 2px;
    background: var(--text-2);
    border-radius: 50%;
    transition: 0.2s;
  }
  .toggle-switch input:checked + .toggle-slider { background: var(--cyan-dim); }
  .toggle-switch input:checked + .toggle-slider::before { transform: translateX(16px); background: var(--cyan); }

  /* Notification preferences grid */
  .notif-prefs-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 4px 16px;
    margin-top: 4px;
    padding: 6px 8px;
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 4px;
  }
  .notif-pref-item {
    display: flex;
    align-items: center;
    gap: 6px;
    cursor: pointer;
    font-size: 10px;
    color: var(--text-1);
  }
  .notif-pref-item input[type="checkbox"] {
    accent-color: var(--cyan);
    width: 12px;
    height: 12px;
    cursor: pointer;
  }
  .notif-pref-label { user-select: none; }

  /* Budget Limit */
  .budget-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 4px 0;
  }
  .budget-label-col {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .budget-hint {
    font-size: 9px;
    color: var(--text-3);
  }
  .budget-input-row {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-shrink: 0;
  }
  .budget-dollar {
    font-size: 11px;
    color: var(--text-2);
    font-weight: 600;
  }
  .budget-limit-input {
    width: 70px;
    padding: 2px 6px;
    font-size: 11px;
  }

  /* Volume slider */
  .volume-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 4px 0 4px 8px;
  }
  .volume-control {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .volume-slider {
    -webkit-appearance: none;
    appearance: none;
    width: 100px;
    height: 3px;
    border-radius: 2px;
    background: var(--bg-3);
    outline: none;
    cursor: pointer;
    accent-color: var(--cyan);
  }
  .volume-slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--cyan);
    cursor: pointer;
  }
  .volume-pct {
    font-size: 10px;
    color: var(--text-2);
    font-family: var(--font-mono, monospace);
    min-width: 28px;
    text-align: right;
  }

  /* Notification history */
  .notif-history-title { margin-top: 4px; }
  .notif-history-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: 160px;
    overflow-y: auto;
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 4px;
  }
  .notif-history-row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 2px 4px;
    border-radius: 2px;
    font-size: 10px;
    min-width: 0;
  }
  .notif-history-row:hover { background: var(--bg-2); }
  .notif-time {
    color: var(--text-3);
    font-family: var(--font-mono, monospace);
    flex-shrink: 0;
    min-width: 44px;
    font-size: 9px;
  }
  .notif-badge {
    font-size: 8px;
    font-weight: 700;
    font-family: var(--font-mono, monospace);
    padding: 1px 4px;
    border-radius: 2px;
    flex-shrink: 0;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }
  .notif-badge-success { color: var(--green); background: color-mix(in srgb, var(--green) 15%, transparent); }
  .notif-badge-error   { color: var(--red);   background: color-mix(in srgb, var(--red) 15%, transparent); }
  .notif-badge-info    { color: var(--cyan);  background: color-mix(in srgb, var(--cyan) 15%, transparent); }
  .notif-badge-warning { color: #f59e0b;      background: color-mix(in srgb, #f59e0b 15%, transparent); }
  .notif-msg {
    flex: 1;
    color: var(--text-1);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }
  .jarvis-btn-sm {
    font-size: 10px;
    padding: 3px 8px;
    align-self: flex-start;
  }

  /* Updates */
  .update-row {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .update-status {
    font-size: 10px;
    font-family: var(--font-mono, monospace);
    padding: 2px 6px;
    border-radius: 3px;
  }
  .update-ok {
    color: var(--green);
    background: color-mix(in srgb, var(--green) 12%, transparent);
  }
  .update-available {
    color: var(--cyan);
    background: color-mix(in srgb, var(--cyan) 12%, transparent);
  }

  /* Backup */
  .backup-row {
    flex-wrap: wrap;
  }
  .import-label {
    position: relative;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
  }
  .import-label.disabled {
    opacity: 0.5;
    pointer-events: none;
  }
  .import-file-input {
    position: absolute;
    inset: 0;
    opacity: 0;
    cursor: pointer;
    width: 100%;
  }

  /* Config file info */
  .config-file-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 4px 8px;
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 4px;
  }
  .config-file-path {
    font-size: 10px;
    font-family: var(--font-mono, monospace);
    color: var(--cyan);
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .config-file-actions {
    display: flex;
    gap: 6px;
    flex-shrink: 0;
  }

  /* Keyboard shortcuts table */
  .shortcuts-table {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 6px 8px;
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 4px;
  }
  .shortcut-row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 3px 0;
  }
  .shortcut-keys {
    display: flex;
    align-items: center;
    gap: 4px;
    min-width: 160px;
    flex-shrink: 0;
  }
  .shortcut-keys kbd {
    display: inline-block;
    font-size: 9px;
    font-family: var(--font-mono, monospace);
    color: var(--text-1);
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: 3px;
    padding: 1px 5px;
    line-height: 1.4;
  }
  .shortcut-desc {
    font-size: 10px;
    color: var(--text-2);
  }

  /* About */
  .about-block {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 8px;
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 4px;
    flex-wrap: wrap;
  }
  .about-name {
    font-size: 13px;
    font-family: var(--font-display);
    font-weight: 700;
    color: var(--cyan);
    letter-spacing: 2px;
  }
  .about-version {
    font-size: 10px;
    font-family: var(--font-mono, monospace);
    color: var(--text-3);
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: 3px;
    padding: 1px 5px;
  }
  .about-desc {
    font-size: 10px;
    color: var(--text-2);
    flex: 1;
  }
  .expand-btn {
    margin-left: auto;
    flex-shrink: 0;
  }

  /* System Info expandable */
  .sysinfo-grid {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 6px 8px;
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 4px;
  }
  .sysinfo-row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 2px 0;
  }
  .sysinfo-label {
    font-size: 9px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--text-3);
    min-width: 100px;
    flex-shrink: 0;
  }
  .sysinfo-value {
    font-size: 11px;
    font-family: var(--font-mono, monospace);
    color: var(--text-0);
  }

  /* Database section */
  .db-section {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .db-actions {
    display: flex;
    gap: 8px;
  }
  .db-stats {
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 6px 8px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .db-stat-total {
    display: flex;
    align-items: center;
    gap: 12px;
    padding-bottom: 4px;
    border-bottom: 1px solid var(--border);
    margin-bottom: 2px;
  }
  .db-tables {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .db-table-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 2px 0;
  }
  .db-table-name {
    font-size: 10px;
    font-family: var(--font-mono, monospace);
    color: var(--text-1);
  }
  .db-table-rows {
    font-size: 9px;
    color: var(--text-3);
    font-family: var(--font-mono, monospace);
  }
  .db-path {
    font-size: 8px;
    color: var(--text-3);
    font-family: var(--font-mono, monospace);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    margin-top: 2px;
    padding-top: 4px;
    border-top: 1px solid var(--border);
  }

  /* Section title collapsible row */
  .section-title-collapsible {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
</style>
