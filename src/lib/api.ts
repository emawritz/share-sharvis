// JARVIS Mission Control - Tauri API Layer
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type {
  Task,
  MachineInfo,
  PipelineState,
  PR,
  Check,
  Config,
  SessionData,
  Activity,
  AgentInfo,
  AgentDetail,
  LogEntry,
  TimelineResponse,
  PipelinesResponse,
  PlanningState,
  RepoStatus,
  JarvisConfig,
  MachineConfigToml,
  ConnectionCheck,
  MachineConnections,
  SessionSnapshot,
  SnapshotSummary,
  TaskChainStep,
  TaskGraph,
  DiffResult,
  WebhookConfig,
  WebhookDelivery,
  AgentMessage,
  TeamMemory,
  AutoRule,
  RuleFireEvent,
  TaskHistoryEntry,
  BranchComparison,
  MachineCapabilities,
  CronJob,
  TokenStats,
  ConflictReport,
  AppLogEntry,
  PlanningHistoryEntry,
  PlanningMetrics,
  KnowledgeEntry,
} from './types';

// -- Session & Activity --

export async function fetchSessionData(): Promise<SessionData> {
  return invoke<SessionData>('get_session_data');
}

export async function fetchAtlasActivity(): Promise<Activity[]> {
  return invoke<Activity[]>('get_atlas_activity');
}

export async function fetchPixelActivity(): Promise<Activity[]> {
  return invoke<Activity[]>('get_pixel_activity_cmd');
}

export async function fetchAgentDetails(target: string): Promise<AgentDetail[]> {
  return invoke<AgentDetail[]>('get_agent_details', { target });
}

// -- Tasks --

export async function sendTask(target: string, prompt: string, orchestrate?: boolean, repo?: string): Promise<Task> {
  return invoke<Task>('send_task', { target, prompt, orchestrate, repo });
}

export function onPlanningChunk(callback: (data: { session_id: string; chunk: string }) => void): Promise<UnlistenFn> {
  return listen<{ session_id: string; chunk: string }>('planning-chunk', (event) => callback(event.payload));
}

export async function fetchTasks(): Promise<Task[]> {
  return invoke<Task[]>('get_tasks');
}

export async function sendTaskChain(steps: TaskChainStep[]): Promise<Task[]> {
  return invoke<Task[]>('send_task_chain', { steps });
}

export async function sendTaskGraph(graph: TaskGraph): Promise<number[]> {
  return invoke<number[]>('send_task_graph', { graph });
}

export async function executeAction(action: string): Promise<Record<string, unknown>> {
  return invoke<Record<string, unknown>>('execute_action', { action });
}

// -- Config --

export async function getConfig(): Promise<Config> {
  return invoke<Config>('get_config');
}

export async function saveConfig(config: Config): Promise<Config> {
  return invoke<Config>('set_config', { config });
}

// -- Machines --

export async function fetchMachines(): Promise<Record<string, MachineInfo>> {
  return invoke<Record<string, MachineInfo>>('get_machines');
}

export async function toggleMachine(id: string, enabled: boolean): Promise<boolean> {
  return invoke<boolean>('toggle_machine', { id, enabled });
}

export async function reconnectMachine(id: string): Promise<MachineInfo | null> {
  return invoke<MachineInfo | null>('reconnect_machine', { id });
}

// -- Pipelines --

export async function fetchPipelines(): Promise<PipelinesResponse> {
  return invoke<PipelinesResponse>('get_pipelines');
}

export async function runPipeline(name: string): Promise<string> {
  return invoke<string>('run_pipeline', { name });
}

export async function stopPipeline(id: string): Promise<void> {
  return invoke<void>('stop_pipeline', { id });
}

// -- GitHub --

export async function fetchGithubPRs(repo: string): Promise<PR[]> {
  return invoke<PR[]>('list_prs', { repo });
}

export async function mergePR(repo: string, number: number, method?: string): Promise<boolean> {
  return invoke<boolean>('merge_pr', { repo, number, method });
}

export async function fetchChecks(repo: string, branch: string): Promise<Check[]> {
  return invoke<Check[]>('get_checks', { repo, branch });
}

export async function compareBranches(repo: string, base: string, head: string): Promise<BranchComparison | null> {
  return invoke<BranchComparison | null>('compare_branches', { repo, base, head });
}

export async function createPR(repo: string, title: string, body: string, head: string): Promise<string | null> {
  return invoke<string | null>('create_pr', { repo, title, body, head });
}

export interface PrComment {
  author: string;
  body: string;
  createdAt: string;
}

export async function getPrComments(repo: string, prNumber: number): Promise<PrComment[]> {
  return invoke<PrComment[]>('get_pr_comments', { repo, prNumber });
}

export async function addPrComment(repo: string, prNumber: number, body: string): Promise<void> {
  return invoke<void>('add_pr_comment', { repo, prNumber, body });
}

export async function getPrFiles(repo: string, prNumber: number): Promise<string[]> {
  return invoke<string[]>('get_pr_files', { repo, prNumber });
}

// -- Agent Log --

export async function fetchAgentLog(target: string, offset: number, limit: number): Promise<[LogEntry[], number]> {
  return invoke<[LogEntry[], number]>('get_agent_log', { target, offset, limit });
}

// -- Timeline --

export async function fetchTimeline(target: string): Promise<TimelineResponse> {
  return invoke<TimelineResponse>('get_timeline', { target });
}

// -- Planning --

export async function startPlanning(objetivo: string): Promise<PlanningState> {
  return invoke<PlanningState>('start_planning', { objetivo });
}

export async function getPlanningState(): Promise<PlanningState | null> {
  return invoke<PlanningState | null>('get_planning_state');
}

export async function approvePlan(): Promise<boolean> {
  return invoke<boolean>('approve_plan');
}

export async function addPlanningFeedback(feedback: string): Promise<boolean> {
  return invoke<boolean>('add_planning_feedback', { feedback });
}

export async function cancelPlanning(): Promise<boolean> {
  return invoke<boolean>('cancel_planning');
}

export async function retryFailedSteps(): Promise<boolean> {
  return invoke<boolean>('retry_failed_steps');
}

export async function getPlanningHistory(): Promise<PlanningHistoryEntry[]> {
  return invoke<PlanningHistoryEntry[]>('get_planning_history');
}

export async function clearPlanningHistory(): Promise<void> {
  return invoke<void>('clear_planning_history');
}

export async function exportPlanningSession(filename: string): Promise<string> {
  return invoke<string>('export_planning_session', { filename });
}

export async function getPlanningMetrics(): Promise<PlanningMetrics> {
  const history = await getPlanningHistory();
  const totalSessions = history.length;
  // Estimate avg steps from response length heuristic (words / 20)
  const avgSteps = totalSessions === 0
    ? 0
    : Math.round(history.reduce((sum, h) => sum + Math.max(1, Math.round(h.response.split(/\s+/).length / 20)), 0) / totalSessions);
  // Success rate: entries whose response doesn't contain error keywords
  const successCount = history.filter(h => !/error|failed|cancelled/i.test(h.response.substring(0, 100))).length;
  const successRate = totalSessions === 0 ? 0 : Math.round((successCount / totalSessions) * 100);
  return { totalSessions, avgSteps, successRate };
}

export async function duplicatePlanningSession(entry: PlanningHistoryEntry): Promise<PlanningHistoryEntry> {
  // Re-submits a history entry prompt and returns a new synthesized entry
  const now = Math.floor(Date.now() / 1000);
  return {
    id: now,
    timestamp: now,
    prompt: entry.prompt,
    response: entry.response,
    machine: entry.machine,
  };
}

// -- Event listeners --

export function onSessionUpdate(callback: (data: SessionData) => void): Promise<UnlistenFn> {
  return listen<SessionData>('session-update', (event) => callback(event.payload));
}

export function onActivityUpdate(callback: (data: { atlas: Activity[]; pixel: Activity[]; atlasAgentInfo?: AgentInfo; pixelAgentInfo?: AgentInfo }) => void): Promise<UnlistenFn> {
  return listen<{ atlas: Activity[]; pixel: Activity[]; atlasAgentInfo?: AgentInfo; pixelAgentInfo?: AgentInfo }>('activity-update', (event) => callback(event.payload));
}

export function onCommitsUpdate(callback: (data: { back: string[]; front: string[] }) => void): Promise<UnlistenFn> {
  return listen<{ back: string[]; front: string[] }>('commits-update', (event) => callback(event.payload));
}

export function onTaskStarted(callback: (data: { id: number; target: string }) => void): Promise<UnlistenFn> {
  return listen<{ id: number; target: string }>('task-started', (event) => callback(event.payload));
}

export function onTaskDone(callback: (data: { id: number; target: string; output: string }) => void): Promise<UnlistenFn> {
  return listen<{ id: number; target: string; output: string }>('task-done', (event) => callback(event.payload));
}

export function onPipelineStep(callback: (data: { pipeline_id: string; step: number; status: string }) => void): Promise<UnlistenFn> {
  return listen<{ pipeline_id: string; step: number; status: string }>('pipeline-step', (event) => callback(event.payload));
}

// -- Repo branches --

export async function getRepoStatuses(): Promise<[RepoStatus, RepoStatus]> {
  return invoke<[RepoStatus, RepoStatus]>('get_repo_statuses');
}

export async function getRepoBranches(repo: 'back' | 'front'): Promise<string[]> {
  return invoke<string[]>('get_repo_branches', { repo });
}

export async function switchBranch(repo: 'back' | 'front', branch: string): Promise<RepoStatus> {
  return invoke<RepoStatus>('switch_branch', { repo, branch }) as Promise<RepoStatus>;
}

export function onPlanningUpdate(callback: (data: PlanningState) => void): Promise<UnlistenFn> {
  return listen<PlanningState>('planning-update', (event) => callback(event.payload));
}

// -- JARVIS Config (TOML) --

export async function getJarvisConfig(): Promise<JarvisConfig> {
  return invoke<JarvisConfig>('get_jarvis_config');
}

export async function saveJarvisConfig(config: JarvisConfig): Promise<boolean> {
  return invoke<boolean>('save_jarvis_config', { config });
}

export async function isFirstLaunch(): Promise<boolean> {
  return invoke<boolean>('is_first_launch');
}

export async function getDetectedLocal(): Promise<MachineConfigToml> {
  return invoke<MachineConfigToml>('get_detected_local');
}

export async function testSshConnection(host: string): Promise<ConnectionCheck> {
  return invoke<ConnectionCheck>('test_ssh_connection', { host });
}

export async function checkMachineConnections(machineId: string): Promise<MachineConnections> {
  return invoke<MachineConnections>('check_machine_connections', { machineId });
}

export async function runFixCommand(machineId: string, command: string): Promise<string> {
  return invoke<string>('run_fix_command', { machineId, command });
}

// -- Snapshots --

export async function saveSnapshot(name: string): Promise<SessionSnapshot> {
  return invoke<SessionSnapshot>('save_session_snapshot', { name });
}

export async function listSnapshots(): Promise<SnapshotSummary[]> {
  return invoke<SnapshotSummary[]>('list_session_snapshots');
}

export async function restoreSnapshot(name: string): Promise<SessionSnapshot> {
  return invoke<SessionSnapshot>('restore_session_snapshot', { name });
}

export async function deleteSnapshot(name: string): Promise<boolean> {
  return invoke<boolean>('delete_session_snapshot', { name });
}

export async function searchSnapshots(query: string): Promise<SessionSnapshot[]> {
  return invoke<SessionSnapshot[]>('search_snapshots', { query });
}

export async function tagSnapshot(id: string, tags: string[]): Promise<void> {
  return invoke<void>('tag_snapshot', { id, tags });
}

// -- Workspaces --

export async function listWorkspaces(): Promise<SnapshotSummary[]> {
  return invoke<SnapshotSummary[]>('list_workspaces');
}

export async function saveWorkspace(name: string): Promise<SessionSnapshot> {
  return invoke<SessionSnapshot>('save_workspace', { name });
}

export async function switchWorkspace(name: string): Promise<SessionSnapshot> {
  return invoke<SessionSnapshot>('switch_workspace', { name });
}

// -- Git Diff --

export async function fetchGitDiff(machineId: string): Promise<DiffResult[]> {
  return invoke<DiffResult[]>('get_git_diff', { machineId });
}

// -- Webhooks --

export async function getWebhookConfig(): Promise<WebhookConfig> {
  return invoke<WebhookConfig>('get_webhook_config');
}

export async function saveWebhookSettings(config: WebhookConfig): Promise<boolean> {
  return invoke<boolean>('save_webhook_settings', { config });
}

export async function testWebhook(): Promise<boolean> {
  return invoke<boolean>('test_webhook');
}

export async function getWebhookDeliveries(): Promise<WebhookDelivery[]> {
  return invoke<WebhookDelivery[]>('get_webhook_deliveries');
}

// -- Messages --

export async function sendAgentMessage(from: string, to: string, category: string, content: string, tags: string[] = []): Promise<AgentMessage> {
  return invoke<AgentMessage>('send_agent_message', { from, to, category, content, tags });
}

export async function getAgentMessages(target?: string, unreadOnly?: boolean, category?: string): Promise<AgentMessage[]> {
  return invoke<AgentMessage[]>('get_agent_messages', { target, unreadOnly, category });
}

export async function markMessagesRead(target: string): Promise<boolean> {
  return invoke<boolean>('mark_messages_read', { target });
}

export async function clearMessages(category?: string): Promise<boolean> {
  return invoke<boolean>('clear_messages', { category });
}

export async function saveTeamMemory(content: string, tags: string[] = [], category?: string): Promise<TeamMemory> {
  return invoke<TeamMemory>('save_team_memory', { content, tags, category });
}

export async function getTeamMemories(): Promise<TeamMemory[]> {
  return invoke<TeamMemory[]>('get_team_memories');
}

export async function deleteTeamMemory(id: number): Promise<boolean> {
  return invoke<boolean>('delete_team_memory', { id });
}

export async function searchTeamMemories(query: string, category?: string): Promise<TeamMemory[]> {
  return invoke<TeamMemory[]>('search_team_memories', { query, category });
}

export async function getMemoryCategories(): Promise<string[]> {
  return invoke<string[]>('get_memory_categories');
}

export async function pinMemory(id: number, pinned: boolean): Promise<void> {
  return invoke<void>('pin_memory', { id, pinned });
}

// -- Rules --

export async function getRules(): Promise<AutoRule[]> {
  return invoke<AutoRule[]>('get_rules');
}

export async function saveRule(rule: AutoRule): Promise<AutoRule> {
  return invoke<AutoRule>('save_rule', { rule });
}

export async function deleteRule(id: string): Promise<boolean> {
  return invoke<boolean>('delete_rule', { id });
}

export async function toggleRule(id: string, enabled: boolean): Promise<boolean> {
  return invoke<boolean>('toggle_rule', { id, enabled });
}

export async function getRuleHistory(id?: string): Promise<RuleFireEvent[]> {
  return invoke<RuleFireEvent[]>('get_rule_history', { id });
}

export async function reorderRules(ids: string[]): Promise<boolean> {
  return invoke<boolean>('reorder_rules', { ids });
}

export async function dryRunRule(id: string, prompt: string): Promise<boolean> {
  return invoke<boolean>('dry_run_rule', { id, prompt });
}

// -- Task History --

export async function getTaskHistory(target?: string, status?: string, limit?: number, offset?: number): Promise<TaskHistoryEntry[]> {
  return invoke<TaskHistoryEntry[]>('get_task_history', { target, status, limit, offset });
}

export async function countTaskHistory(target?: string, status?: string): Promise<number> {
  return invoke<number>('count_task_history', { target, status });
}

export async function clearTaskHistory(): Promise<boolean> {
  return invoke<boolean>('clear_task_history');
}

export async function searchTaskHistory(query: string, limit?: number): Promise<TaskHistoryEntry[]> {
  return invoke<TaskHistoryEntry[]>('search_task_history', { query, limit });
}

export async function getTaskHistoryByMachine(machineId: string, limit?: number): Promise<TaskHistoryEntry[]> {
  return invoke<TaskHistoryEntry[]>('get_task_history_by_machine', { machineId, limit });
}

// -- Notifications --

export async function getNotificationsEnabled(): Promise<boolean> {
  return invoke<boolean>('get_notifications_enabled');
}

export async function setNotificationsEnabled(enabled: boolean): Promise<boolean> {
  return invoke<boolean>('set_notifications_enabled', { enabled });
}

// -- Budget --

export async function getBudgetLimit(): Promise<number | null> {
  return invoke<number | null>('get_budget_limit');
}

export async function setBudgetLimit(limit: number | null): Promise<boolean> {
  return invoke<boolean>('set_budget_limit', { limit });
}

// -- Capabilities --

export async function fetchMachineCapabilities(): Promise<MachineCapabilities[]> {
  return invoke<MachineCapabilities[]>('get_machine_capabilities');
}

export async function fetchSingleMachineCapabilities(machineId: string): Promise<MachineCapabilities> {
  return invoke<MachineCapabilities>('get_single_machine_capabilities', { machineId });
}

// -- App Version & Environment --

export interface AppVersion {
  major: number;
  minor: number;
  patch: number;
}

export interface EnvironmentInfo {
  platform: string;
  arch: string;
  hostname: string;
  osVersion: string;
  cargoVersion: string;
  rustEdition: string;
}

export async function getAppVersion(): Promise<AppVersion> {
  return invoke<AppVersion>('get_app_version');
}

export async function getEnvironmentInfo(): Promise<EnvironmentInfo> {
  return invoke<EnvironmentInfo>('get_environment_info');
}

// -- Database Admin --

export interface DbTableStat {
  name: string;
  row_count: number;
}

export interface DbStats {
  tables: DbTableStat[];
  total_size_kb: number;
  db_path: string;
}

export async function getDbStats(): Promise<DbStats> {
  return invoke<DbStats>('get_db_stats');
}

export async function vacuumDb(): Promise<void> {
  return invoke<void>('vacuum_db');
}

// -- Notification History (backend ring buffer) --

export interface NotifHistoryEntry {
  id: number;
  timestamp: string;
  title: string;
  body: string;
  level: string;
}

export async function getNotificationHistory(): Promise<NotifHistoryEntry[]> {
  return invoke<NotifHistoryEntry[]>('get_notification_history');
}

// -- System Stats --

export interface SystemStats {
  cpuUsage: number;
  memoryUsed: number;
  memoryTotal: number;
  diskUsed: number;
  diskTotal: number;
  uptimeSecs: number;
}

export async function getSystemStats(): Promise<SystemStats> {
  return invoke<SystemStats>('get_system_stats');
}

export interface DirEntry { name: string; path: string; is_dir: boolean; }
export async function listDir(path: string): Promise<DirEntry[]> {
  return invoke<DirEntry[]>('list_dir', { path });
}

// -- Machine Metrics, Remote Exec & Logs --

export interface MachineMetrics {
  cpu_percent: number;
  ram_percent: number;
  disk_percent: number;
  network_rx_kb: number;
  network_tx_kb: number;
  load_average: number;
}

export async function getMachineMetrics(machineId: string): Promise<MachineMetrics> {
  return invoke<MachineMetrics>('get_machine_metrics', { machineId });
}

export async function executeOnMachine(machineId: string, command: string): Promise<string> {
  return invoke<string>('execute_on_machine', { machineId, command });
}

export async function getMachineLogs(machineId: string, lines: number): Promise<string[]> {
  return invoke<string[]>('get_machine_logs', { machineId, lines });
}

// -- Crons --

export async function getCrons(): Promise<CronJob[]> {
  return invoke<CronJob[]>('get_crons');
}

export async function saveCron(job: CronJob): Promise<CronJob> {
  return invoke<CronJob>('save_cron', { job });
}

export async function deleteCron(id: string): Promise<boolean> {
  return invoke<boolean>('delete_cron', { id });
}

export async function toggleCron(id: string, enabled: boolean): Promise<boolean> {
  return invoke<boolean>('toggle_cron', { id, enabled });
}

export async function validateCronExpr(expr: string): Promise<string> {
  return invoke<string>('validate_cron_expr_cmd', { expr });
}

// -- Token Stats --

export async function getTokenStats(): Promise<TokenStats> {
  return invoke<TokenStats>('get_token_stats');
}

// -- Daily Stats & Top Tools --

export async function getDailyStats(days: number): Promise<import('./types').DailyStat[]> {
  return invoke<import('./types').DailyStat[]>('get_daily_stats', { days });
}

export async function getTopTools(limit: number): Promise<import('./types').ToolStat[]> {
  return invoke<import('./types').ToolStat[]>('get_top_tools', { limit });
}

export async function getCronNextRuns(): Promise<{ id: string; nextRun: string | null }[]> {
  return invoke<{ id: string; nextRun: string | null }[]>('get_cron_next_runs');
}

// -- Conflict Detection --

export async function detectConflicts(): Promise<ConflictReport[]> {
  return invoke<ConflictReport[]>('detect_conflicts');
}

export async function resolveConflict(repoPath: string, file: string, resolution: 'ours' | 'theirs' | 'manual'): Promise<string> {
  return invoke<string>('resolve_conflict', { repoPath, file, resolution });
}

export async function autoResolveConflicts(repoPath: string): Promise<string[]> {
  return invoke<string[]>('auto_resolve_conflicts', { repoPath });
}

export async function getConflictDiff(repoPath: string, file: string): Promise<string> {
  return invoke<string>('get_conflict_diff', { repoPath, file });
}

// -- Updater --

export async function checkForUpdate(): Promise<string | null> {
  return invoke<string | null>('check_for_update');
}

export async function installUpdate(): Promise<void> {
  return invoke<void>('install_update');
}

// -- Config Backup --

export async function exportConfig(): Promise<unknown> {
  return invoke<unknown>('export_config');
}

export async function importConfig(data: string): Promise<void> {
  return invoke<void>('import_config', { data });
}

// -- Machine Routing --

export async function getBestMachineForTask(prompt: string): Promise<string> {
  return invoke<string>('best_machine_for_task', { prompt });
}

// executeMachineCommand is an alias for executeOnMachine (same Tauri command)
export async function executeMachineCommand(machineId: string, command: string): Promise<string> {
  return invoke<string>('execute_on_machine', { machineId, command });
}

// -- Voice-Agent Sessions --

export async function getSessions(): Promise<import('./types').SessionInfo[]> {
  try {
    const resp = await fetch('http://localhost:3141/sessions');
    const data = await resp.json();
    return data.sessions || [];
  } catch {
    return [];
  }
}

// -- App Logs --

export async function getAppLogs(since?: string): Promise<AppLogEntry[]> {
  return invoke<AppLogEntry[]>('get_app_logs', { since });
}

export async function clearAppLogs(): Promise<void> {
  return invoke<void>('clear_app_logs');
}

export async function getAppLogStats(): Promise<import('./types').AppLogStats> {
  return invoke<import('./types').AppLogStats>('get_app_log_stats');
}

// -- Team Context --

export async function getTeamContext(): Promise<string> {
  return invoke<string>('get_team_context');
}

// -- Knowledge --

export async function saveKnowledge(title: string, content: string, sourceUrl?: string, sourceType?: string, tags?: string[]): Promise<number> {
  return invoke<number>('save_knowledge', { title, content, sourceUrl, sourceType, tags });
}

export async function getKnowledge(limit?: number, offset?: number, sourceTypeFilter?: string, searchQuery?: string): Promise<KnowledgeEntry[]> {
  return invoke<KnowledgeEntry[]>('get_knowledge', { limit, offset, sourceTypeFilter, searchQuery });
}

export async function deleteKnowledge(id: number): Promise<boolean> {
  return invoke<boolean>('delete_knowledge', { id });
}

export async function starKnowledge(id: number): Promise<boolean> {
  return invoke<boolean>('star_knowledge', { id });
}

export async function searchKnowledge(query: string, limit?: number): Promise<KnowledgeEntry[]> {
  return invoke<KnowledgeEntry[]>('search_knowledge', { query, limit });
}

// -- Event Listeners --

export function onWebhookFailed(cb: (payload: { url: string; error: string; attempts: number }) => void): Promise<UnlistenFn> {
  return listen<{ url: string; error: string; attempts: number }>('webhook-failed', e => cb(e.payload));
}

export function onRuleAlert(cb: (payload: { rule: string; message: string }) => void): Promise<UnlistenFn> {
  return listen<{ rule: string; message: string }>('rule-alert', e => cb(e.payload));
}

export function onRepoConflict(cb: (payload: { repo: string; machineA: string; machineB: string }) => void): Promise<UnlistenFn> {
  return listen<{ repo: string; machineA: string; machineB: string }>('repo-conflict', e => cb(e.payload));
}

export function onWorkspaceSwitched(cb: (payload: { name: string }) => void): Promise<UnlistenFn> {
  return listen<{ name: string }>('workspace-switched', e => cb(e.payload));
}


