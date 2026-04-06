// JARVIS Mission Control - TypeScript Interfaces

export interface AgentInfo {
  agentCount: number;
  skills: string[];
}

export interface AgentDetail {
  sessionId: string;
  lastTool?: string;
  lastDetail?: string;
  lastText?: string;
  secondsAgo: number;
}

export interface RepoStatus {
  branch: string;
  changed: number;
  staged: number;
  untracked: number;
  lastCommit: string;
  ahead: number;
  behind: number;
}

export interface PlanningMessage {
  sender: string;
  content: string;
  round: number;
  timestamp: string;
}

export interface PlanStep {
  index: number;
  target: string;
  description: string;
  status: string;
  taskId?: number;
  output?: string;
}

export interface PlanningState {
  id: string;
  objetivo: string;
  phase: string;
  messages: PlanningMessage[];
  planSteps: PlanStep[];
  currentRound: number;
  currentSpeaker: string;
  startedAt: string;
  finishedAt?: string;
  elapsedSecs: number;
  currentActivity: Activity[];
  branchBack?: string;
  branchFront?: string;
  repoBack?: RepoStatus;
  repoFront?: RepoStatus;
  streamingText?: string;
}

export interface Activity {
  type: 'tool' | 'text' | 'prompt';
  name?: string;
  detail?: string;
  content?: string;
  ts?: number; // epoch ms when this activity was read from disk
}

export interface RoundInfo {
  file: string;
  size: number;
  done: boolean;
}

export interface RoundSummary {
  file: string;
  summary: string;
  size: number;
}

export interface SessionData {
  active: boolean;
  sessionId: string;
  objetivo: string;
  rama: string;
  totalRounds: string;
  atlasRunning: boolean;
  pixelRunning: boolean;
  rounds: RoundInfo[];
  roundSummaries: RoundSummary[];
  commitsBack: string[];
  commitsFront: string[];
}

export interface MachineStats {
  cpu: string;
  mem: string;
  disk: string;
  gpu?: string;
  uptime: string;
  online: boolean;
}

export interface MachineHealth {
  online: boolean;
  latencyMs: number;
}

export interface MachineInfo {
  id: string;
  name: string;
  host: string;
  ip?: string;
  os: string;
  role: string;
  repo?: string;
  repoPath?: string;
  gpu?: string;
  enabled: boolean;
  tags: string[];
  repos?: RepoConfigToml[];
  health?: MachineHealth;
  stats?: MachineStats;
}

export interface Task {
  id: number;
  target: string;
  prompt: string;
  status: string;
  orchestrate: boolean;
  startedAt?: number;
  finishedAt?: number;
  output: string;
  pixelTaskId?: number;
  dependsOn?: number[];
  runCondition?: string;
}

export interface TaskChainStep {
  target: string;
  prompt: string;
  runCondition: string;
}

export interface TaskGraphNode {
  id: string;
  target: string;
  prompt: string;
  dependsOn: string[];
  onFailure: string; // "stop" | "continue" | "skip_dependents"
}

export interface TaskGraph {
  nodes: TaskGraphNode[];
}

export interface Config {
  sessionId: string;
  rama: string;
  objetivo: string;
}

// Pipeline types
export interface BuiltinInfo {
  name: string;
  description: string;
  steps: number;
}

export interface PipelineStepState {
  name: string;
  target: string;
  status: string;
  output?: string;
  startedAt?: string;
  finishedAt?: string;
  retries: number;
}

export interface PipelineState {
  id: string;
  name: string;
  description: string;
  status: string;
  currentStep: number;
  startedAt?: string;
  finishedAt?: string;
  steps: PipelineStepState[];
}

export interface PipelinesResponse {
  pipelines: PipelineState[];
  builtins: BuiltinInfo[];
}

// GitHub types
export interface PR {
  number: number;
  title: string;
  state: string;
  headRefName: string;
  author: unknown;
  createdAt: string;
  additions: number;
  deletions: number;
  reviews: unknown;
}

export interface Check {
  name: string;
  status: string;
  conclusion: string;
}

export interface CompareFile {
  filename: string;
  status: string;
  additions: number;
  deletions: number;
  changes: number;
}

export interface CompareCommit {
  sha: string;
  message: string;
  author: string;
  date: string;
}

export interface BranchComparison {
  aheadBy: number;
  behindBy: number;
  totalCommits: number;
  files: CompareFile[];
  commits: CompareCommit[];
}

// Timeline types
export interface TimelineSummary {
  totalInputTokens: number;
  totalOutputTokens: number;
  totalTokens: number;
  toolCalls: Record<string, number>;
  duration: number;
  durationHuman: string;
  errorCount: number;
  filesTouched: string[];
  commandsRun: string[];
}

export interface TimelineError {
  timestamp: string;
  tool: string;
  command: string;
  error: string;
}

export interface HeatmapEntry {
  minute: string;
  count: number;
  tools: Record<string, number>;
}

export interface FileChange {
  path: string;
  reads: number;
  edits: number;
  writes: number;
  total: number;
}

export interface TimelineResponse {
  summary: TimelineSummary;
  errors: TimelineError[];
  heatmap: HeatmapEntry[];
  files: FileChange[];
  eventCount: number;
}

export interface TokenStats {
  totalCostUsd: number;
  tokensIn: number;
  tokensOut: number;
  sessionsToday: number;
  costByModel: Record<string, number>;
}

// Agent log types
export interface LogEntry {
  timestamp: string;
  type: 'tool_use' | 'tool_result' | 'text' | 'prompt';
  toolName?: string;
  inputSummary?: string;
  outputSummary?: string;
  durationMs?: number;
  isError: boolean;
}

// Message Bus types
export interface AgentMessage {
  id: number;
  from: string;
  to: string;
  category: string;
  content: string;
  timestamp: string;
  read: boolean;
  tags: string[];
  memoryCategory?: string | null;
  pin?: boolean;
}

export interface TeamMemory {
  id: number;
  from: string;
  content: string;
  timestamp: string;
  tags: string[];
  /** Organisational sub-category, e.g. "architecture", "decisions", "todo". */
  category?: string | null;
  pin: boolean;
}

// Automation Rules types
export interface RuleCondition {
  field: string;
  operator: string;
  value: string;
}

export interface RuleAction {
  actionType: string;
  target?: string;
  prompt?: string;
  pipelineName?: string;
  message?: string;
  to?: string;
}

export interface AutoRule {
  id: string;
  name: string;
  trigger: string;
  condition?: RuleCondition;
  action: RuleAction;
  enabled: boolean;
  lastFired?: string;
  fireCount: number;
  priority?: number;
}

export interface RuleFireEvent {
  ruleId: string;
  timestamp: string;
  trigger: string;
  result: string;
}

// Task History types
export interface TaskHistoryEntry {
  id: number;
  target: string;
  prompt: string;
  output: string;
  status: string;
  timestamp: string;
  durationSecs: number;
}

// UI types
export type ToastType = 'success' | 'error' | 'info' | 'warning';

export interface Toast {
  id: number;
  message: string;
  type: ToastType;
}

export interface Preset {
  name: string;
  target: string;
  prompt: string;
}

// Config types (TOML-backed)
export interface RepoConfigToml {
  name: string;
  path: string;
  github: string;
}

export interface MachineConfigToml {
  id: string;
  name: string;
  host: string;
  ip?: string;
  os: string;
  role: string;
  gpu?: string;
  enabled: boolean;
  tags: string[];
  repos: RepoConfigToml[];
}

export interface SessionConfigToml {
  id: string;
  rama: string;
  objetivo: string;
}

export interface JarvisConfig {
  session: SessionConfigToml;
  machines: MachineConfigToml[];
}

export interface ConnectionCheck {
  name: string;
  status: 'ok' | 'error' | 'warning';
  detail: string;
}

export interface MachineConnections {
  machineId: string;
  checks: ConnectionCheck[];
}

// Diff types
export interface DiffFile {
  path: string;
  status: string;
  additions: number;
  deletions: number;
  diffText: string;
}

export interface DiffResult {
  machineId: string;
  repoName: string;
  branch: string;
  files: DiffFile[];
  totalAdditions: number;
  totalDeletions: number;
}

// Webhook types
export interface WebhookConfig {
  enabled: boolean;
  url: string;
  webhookType: string;
  onTaskComplete: boolean;
  onTaskFail: boolean;
  onPipelineComplete: boolean;
  /** Optional allowlist of event types (empty = all events). */
  eventFilter: string[];
  /** ISO-8601 timestamp of the last delivery attempt. */
  lastDelivery: string | null;
  /** HTTP status code of the last delivery attempt. */
  lastStatusCode: number | null;
}

export interface WebhookDelivery {
  id: number;
  timestamp: string;
  url: string;
  eventType: string;
  statusCode: number | null;
  success: boolean;
  responseSnippet: string;
}

// Snapshot types
export interface SnapshotSummary {
  name: string;
  createdAt: string;
  objetivo: string;
  rama: string;
}

export interface SessionSnapshot {
  name: string;
  createdAt: string;
  objetivo: string;
  rama: string;
  sessionId: string;
  branches: { repoName: string; branch: string; lastCommit: string }[];
  pendingTasks: string[];
  machineCount: number;
}

// Capabilities types
export interface MachineCapabilities {
  machineId: string;
  machineName: string;
  plugins: PluginInfo[];
  agents: AgentFile[];
  mcps: string[];
  skillsUsed: string[];
}

export interface PluginInfo {
  name: string;
  enabled: boolean;
}

export interface AgentFile {
  filename: string;
  contentPreview: string;
}

// Conflict detection types
export interface ConflictReport {
  machineA: string;
  machineB: string;
  repo: string;
  overlappingFiles: string[];
  branchA: string;
  branchB: string;
  detectedAt: string;
}

// App log types
export interface AppLogEntry {
  level: string;
  message: string;
  timestamp: string;
}

export interface AppLogStats {
  total: number;
  by_level: Record<string, number>;
  oldest_ts: string | null;
}

// Session info (from Python voice agent via :3141/sessions)
export interface SessionInfo {
  name: string;
  message_count: number;
  active_task_id: number | null;
  task_count: number;
  project: string;
  machine: string;
  created_at: number;
}

// Daily stats / tool stats types
export interface DailyStat {
  date: string;
  tokens: number;
  costUsd: number;
  events: number;
}

export interface ToolStat {
  toolName: string;
  calls: number;
}

// Planning history types
export interface PlanningHistoryEntry {
  id: number;
  timestamp: number;
  prompt: string;
  response: string;
  machine: string;
}

export interface PlanningMetrics {
  totalSessions: number;
  avgSteps: number;
  successRate: number;
}

// Knowledge types
export interface KnowledgeEntry {
  id: number;
  title: string;
  content: string;
  sourceUrl?: string;
  sourceType: string;
  tags: string[];
  createdAt: string;
  starred: boolean;
}

// Cron types
export interface CronJob {
  id: string;
  name: string;
  cronExpr: string;
  target: string;
  prompt: string;
  enabled: boolean;
  lastRun?: string;
  nextRun?: string;
  runCount: number;
}
