// 与 Rust src-tauri/src/models.rs 一一对应的类型定义。

export type Agent = "claude" | "codex";
export type AgentFilter = Agent | "all";
export type PromptOrigin = "history" | "conversation" | "both";

export interface PromptEntry {
  id: string;
  agent: Agent;
  text: string;
  project: string;
  timestamp: number;
  origin: PromptOrigin;
  sessionId: string | null;
  /** sessionId 对应的会话文件存在于索引中，可以打开详情 */
  hasConversation: boolean;
  gitBranch: string | null;
  isCommand: boolean;
  pastedCount: number;
  charCount: number;
}

export interface ProjectInfo {
  path: string;
  name: string;
  agents: Agent[];
  promptCount: number;
  commandCount: number;
  sessionCount: number;
  firstActive: number;
  lastActive: number;
  hasConversations: boolean;
}

export interface SearchResult {
  entry: PromptEntry;
  matchRanges: [number, number][];
}

/** 全文搜索在会话文件中的一条命中 */
export interface ConversationHit {
  agent: Agent;
  sessionId: string;
  project: string;
  sessionTitle: string;
  sessionStartedAt: number;
  /** 与详情页 ChatMessage.uuid 对应；详情合并掉该消息时按时间戳回退定位 */
  messageUuid: string;
  timestamp: number;
  role: "user" | "assistant";
  kind: "text" | "thinking" | "tool_use";
  toolName: string | null;
  snippet: string;
  matchRanges: [number, number][];
}

export interface ConversationSearchResponse {
  hits: ConversationHit[];
  scannedFiles: number;
  matchedSessions: number;
  truncated: boolean;
  elapsedMs: number;
}

export interface SessionSummary {
  agent: Agent;
  sessionId: string;
  project: string;
  title: string;
  startedAt: number;
  endedAt: number;
  messageCount: number;
  gitBranch: string | null;
  cliVersion: string | null;
  source: string | null;
  models: string[];
  usage: SessionUsage;
}

export type BlockKind =
  | "text"
  | "thinking"
  | "tool_use"
  | "tool_result"
  | "image"
  | "attachment";

/** tool_result 的完整输出被 Claude Code 持久化到 projects 目录下的文件，本机能找到时才带此字段 */
export interface PersistedOutput {
  /** 相对 projects 目录的路径，`/` 分隔；传给 read_persisted_output */
  path: string;
  size: number;
  /** 导出路径下正文已替换为完整内容 */
  inlined: boolean;
}

export interface PersistedOutputText {
  text: string;
  truncated: boolean;
  size: number;
}

export interface ContentBlock {
  kind: BlockKind;
  /** attachment：文件内容 */
  text: string | null;
  /** tool_use / tool_result：工具名；attachment：原始文件路径 */
  toolName: string | null;
  toolInput: unknown | null;
  /** text 已按展示上限截断 */
  truncated: boolean;
  persistedOutput?: PersistedOutput;
}

export interface ChatMessage {
  uuid: string;
  agent: Agent;
  role: "user" | "assistant" | "system";
  timestamp: number;
  isSidechain: boolean;
  blocks: ContentBlock[];
  /** 仅承载 API 调用用量的助手消息带此字段 */
  usage?: MessageUsage;
}

export interface ConversationDetail {
  agent: Agent;
  sessionId: string;
  project: string;
  gitBranch: string | null;
  startedAt: number;
  endedAt: number;
  cliVersion: string | null;
  source: string | null;
  models: string[];
  messages: ChatMessage[];
  usage: SessionUsage;
  usageBreakdown: UsageBreakdown;
}

export interface DayCount {
  day: string;
  count: number;
}
export interface HourCount {
  hour: number;
  count: number;
}
export interface WeekdayCount {
  weekday: number;
  count: number;
}
export interface ProjectCount {
  path: string;
  name: string;
  count: number;
}

export interface CliVersion {
  agent: Agent;
  version: string;
}

/* ----------------------------- Token 用量统计 ----------------------------- */

/** 一次调用或若干调用之和的归一化 Token（与 Rust NormalizedUsage 平铺后一致） */
export interface CallTokenFields {
  uncachedInput: number;
  cacheRead: number;
  cacheCreation: number;
  /** cacheCreation 中按 1 小时档计费的部分（Claude），其余为 5 分钟档；Codex 恒为 0 */
  cacheCreation1h: number;
  output: number;
  /** output 的子集，仅展示 */
  reasoningOutput: number;
}

export interface TokenUsageFields extends CallTokenFields {
  totalTokensIncludingCache: number;
  estCostUsd: number | null;
  unknownModelTokens: number;
}

export interface ModelUsage extends TokenUsageFields {
  agent: Agent;
  model: string;
  messages: number;
}

export interface DayUsage extends TokenUsageFields {
  day: string;
}

export interface ProjectUsage extends TokenUsageFields {
  path: string;
  name: string;
  agents: Agent[];
}

export interface UsageStats extends TokenUsageFields {
  assistantMessages: number;
  byModel: ModelUsage[];
  byDay: DayUsage[];
  byProject: ProjectUsage[];
}

/** 归属到单个会话的用量；fork/resume 复制的调用只计入最早的会话 */
export interface SessionUsage extends Omit<TokenUsageFields, "estCostUsd"> {
  estCostUsd: number;
  assistantMessages: number;
}

/**
 * 单次 API 调用的用量，挂在承载它的助手消息上：Claude 一次调用按内容块拆成多条记录，只有
 * 首条带用量；Codex 的 token_count 事件归到它前面那条模型输出的消息。
 */
export interface MessageUsage extends CallTokenFields {
  /** 与索引共用的调用指纹 */
  callKey: string;
  model: string;
  /** 未知定价模型为 null */
  estCostUsd: number | null;
  /** false：索引把这次调用归到了别的会话（resume/fork 复制进来的），不计入本会话 */
  attributed: boolean;
}

/** 索引归属到本会话的一次调用（含 Claude 子代理文件里的调用，它们在本对话里没有消息） */
export interface UsageCall extends CallTokenFields {
  callKey: string;
  timestamp: number;
  model: string;
  estCostUsd: number | null;
  subagent: boolean;
}

export interface SessionModelUsage extends CallTokenFields {
  model: string;
  calls: number;
  totalTokensIncludingCache: number;
  estCostUsd: number | null;
  /** 全部输入按非缓存价计的成本；未知定价为 null */
  estCostNoCacheUsd: number | null;
}

/** 估算成本按计费类目拆分，四项之和等于估算成本 */
export interface CostParts {
  uncachedInput: number;
  cacheRead: number;
  cacheCreation: number;
  output: number;
}

/** 会话用量明细：calls 与 byModel 的 Token 之和都等于 ConversationDetail.usage */
export interface UsageBreakdown {
  /** 按时间顺序，含子代理调用 */
  calls: UsageCall[];
  /** 按总 Token 降序 */
  byModel: SessionModelUsage[];
  costParts: CostParts;
  /** 已知定价模型完全不用缓存时的成本 */
  estCostNoCacheUsd: number;
  /** 本对话里由 resume/fork 复制进来、已计入原会话的调用数 */
  unattributedCalls: number;
}

export interface AppStats {
  totalPrompts: number;
  totalProjects: number;
  totalSessions: number;
  totalMessages: number;
  historyPrompts: number;
  conversationPrompts: number;
  commandCount: number;
  firstUse: number;
  lastUse: number;
  byDay: DayCount[];
  byHour: HourCount[];
  byWeekday: WeekdayCount[];
  topProjects: ProjectCount[];
  cliVersions: CliVersion[];
  usage: UsageStats;
}

export interface IndexMeta {
  builtAt: number;
  fromCache: boolean;
  sourceFiles: number;
  reparsedFiles: number;
}

export type IndexPhase = "scanning" | "parsing" | "assembling" | "done";

/** 后端 `index-progress` 事件的载荷 */
export interface IndexProgress {
  phase: IndexPhase;
  done: number;
  total: number;
}

export type SortMode = "newest" | "oldest" | "longest";

export type ExportGroupBy = "project" | "day" | "none";

export interface ExportParams {
  startDate: string; // YYYY-MM-DD
  endDate: string; // YYYY-MM-DD
  project: string | null; // null = 全部文件夹
  includeCommands: boolean;
  groupBy: ExportGroupBy;
  agentFilter: AgentFilter;
  write: boolean;
  lang?: string; // 导出文案语言："zh" | "en"，跟随界面语言
}

export interface ExportResult {
  preview: string;
  path: string | null;
  promptCount: number;
  folderCount: number;
  dayCount: number;
}

/* ----------------------------- 对话导出 ----------------------------- */

export interface ConversationExportResult {
  preview: string;
  path: string | null;
  messageCount: number;
}

/* ----------------------------- 批量会话导出 ----------------------------- */

/** 批量导出的一个目标会话；同一 session ID 可能同时存在于两个产品，必须带 agent */
export interface SessionRef {
  agent: Agent;
  sessionId: string;
}

/** 后端 `export-progress` 事件的载荷：已解析 / 总数 */
export interface ExportProgress {
  done: number;
  total: number;
}

export interface SessionsExportResult {
  /** 合并模式是 .md 文件路径；多文件模式是文件夹路径（每个会话一份 .md，另附 index.md） */
  path: string;
  merged: boolean;
  sessionCount: number;
  messageCount: number;
  /** 写入的 Markdown 文件数；多文件模式含 index.md */
  fileCount: number;
}

/* ----------------------------- 导入 Claude Code 会话 ----------------------------- */

export interface ImportProject {
  /** zip 内 projects/ 下的目录名 */
  cloudDir: string;
  /** 会话记录里的根工作目录（编码后等于 cloudDir） */
  cloudCwd: string;
  sessionCount: number;
  suggestedLocal: string | null;
  suggestionSource: "remembered" | "name" | null;
}

export interface ImportInspection {
  zipPath: string;
  projects: ImportProject[];
  sessionCount: number;
  skippedEntries: string[];
}

/** localCwd 为空 / null 表示保持原路径 */
export interface ProjectMapping {
  cloudCwd: string;
  localCwd: string | null;
}

export type ImportAction =
  | "add"
  | "update"
  | "skip_identical"
  | "skip_older"
  | "conflict";

export interface SessionSide {
  lines: number;
  /** 最后一条带时间戳记录的毫秒时间；0 表示没有 */
  lastTimestamp: number;
  bytes: number;
}

export interface PlannedSession {
  sessionId: string;
  cloudDir: string;
  cloudCwd: string;
  targetDir: string;
  targetCwd: string;
  title: string;
  action: ImportAction;
  imported: SessionSide;
  local: SessionSide | null;
  sideFiles: number;
  existingElsewhere: string | null;
}

export interface ImportPlanCounts {
  add: number;
  update: number;
  skip: number;
  conflict: number;
}

export interface ImportPlan {
  zipPath: string;
  sessions: PlannedSession[];
  counts: ImportPlanCounts;
  skippedEntries: string[];
}

export interface ConflictDecision {
  cloudDir: string;
  sessionId: string;
  keepLocal: boolean;
}

export interface ImportedProject {
  path: string;
  name: string;
}

export interface ImportResult {
  added: number;
  updated: number;
  skipped: number;
  keptLocal: number;
  overwritten: number;
  filesWritten: number;
  skippedEntries: string[];
  projects: ImportedProject[];
  index: IndexMeta;
}

/* ----------------------------- 设置 ----------------------------- */

export interface SettingsInput {
  claudeDataDir: string;
  codexDataDir: string;
  historyFile: string;
  projectsDir: string;
  sessionsDir: string;
  /** 导入时记住的云端根目录 → 本机目录 */
  importPathMappings: Record<string, string>;
}

export interface ResolvedClaudePaths {
  history: string;
  projects: string;
  sessions: string;
  historyExists: boolean;
  projectsExists: boolean;
  sessionsExists: boolean;
}

export interface ResolvedCodexPaths {
  root: string;
  history: string;
  sessions: string;
  archivedSessions: string;
  rootExists: boolean;
  historyExists: boolean;
  sessionsExists: boolean;
  archivedSessionsExists: boolean;
}

export interface ResolvedPaths {
  claude: ResolvedClaudePaths;
  codex: ResolvedCodexPaths;
}

export interface SettingsView extends SettingsInput {
  configPath: string;
  resolved: ResolvedPaths;
}
