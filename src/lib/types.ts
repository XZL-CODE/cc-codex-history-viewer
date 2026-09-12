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
  | "image";

export interface ContentBlock {
  kind: BlockKind;
  text: string | null;
  toolName: string | null;
  toolInput: unknown | null;
  /** text 已按展示上限截断 */
  truncated: boolean;
}

export interface ChatMessage {
  uuid: string;
  agent: Agent;
  role: "user" | "assistant" | "system";
  timestamp: number;
  isSidechain: boolean;
  blocks: ContentBlock[];
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

export interface TokenUsageFields {
  uncachedInput: number;
  cacheRead: number;
  cacheCreation: number;
  output: number;
  reasoningOutput: number;
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

/* ----------------------------- 设置 ----------------------------- */

export interface SettingsInput {
  claudeDataDir: string;
  codexDataDir: string;
  historyFile: string;
  projectsDir: string;
  sessionsDir: string;
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
