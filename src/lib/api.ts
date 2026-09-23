// Tauri Commands 的前端封装。
// 注意：invoke 的参数键用 camelCase，Tauri 会自动映射到 Rust 的 snake_case 参数。

import { invoke } from "@tauri-apps/api/core";
import { translate } from "@/i18n";
import type {
  AppStats,
  Agent,
  AgentFilter,
  ConversationDetail,
  ConversationExportResult,
  ConflictDecision,
  ConversationSearchResponse,
  ExportParams,
  ExportResult,
  ImportInspection,
  ImportPlan,
  ImportResult,
  IndexMeta,
  PersistedOutputText,
  ProjectInfo,
  ProjectMapping,
  PromptEntry,
  SearchResult,
  SessionRef,
  SessionSummary,
  SessionsExportResult,
  SettingsInput,
  SettingsView,
  SortMode,
} from "./types";

export const api = {
  getProjects: (agentFilter: AgentFilter) =>
    invoke<ProjectInfo[]>("get_projects", { agentFilter }),

  getProjectPrompts: (
    project: string,
    sort: SortMode,
    includeCommands: boolean,
    agentFilter: AgentFilter
  ) =>
    invoke<PromptEntry[]>("get_project_prompts", {
      project,
      sort,
      includeCommands,
      agentFilter,
    }),

  getRecentPrompts: (
    limit: number,
    includeCommands: boolean,
    agentFilter: AgentFilter
  ) =>
    invoke<PromptEntry[]>("get_recent_prompts", {
      limit,
      includeCommands,
      agentFilter,
    }),

  searchPrompts: (
    query: string,
    projectFilter: string | null,
    includeCommands: boolean,
    agentFilter: AgentFilter
  ) =>
    invoke<SearchResult[]>("search_prompts", {
      query,
      projectFilter,
      includeCommands,
      agentFilter,
    }),

  /** 不传日期返回全量统计；传 YYYY-MM-DD 起止日期时按本地时区闭区间即时计算 */
  /** 全文搜索会话内容：并行流式扫描会话文件，不落索引 */
  searchConversations: (
    query: string,
    projectFilter: string | null,
    agentFilter: AgentFilter,
    limit?: number
  ) =>
    invoke<ConversationSearchResponse>("search_conversations", {
      query,
      projectFilter,
      agentFilter,
      limit,
    }),

  getStats: (
    agentFilter: AgentFilter,
    startDate: string | null = null,
    endDate: string | null = null
  ) => invoke<AppStats>("get_stats", { agentFilter, startDate, endDate }),

  getProjectSessions: (project: string, agentFilter: AgentFilter) =>
    invoke<SessionSummary[]>("get_project_sessions", { project, agentFilter }),

  getConversation: (agent: Agent, sessionId: string) =>
    invoke<ConversationDetail>("get_conversation", { agent, sessionId }),

  getIndexMeta: () => invoke<IndexMeta>("get_index_meta"),

  /** 刷新索引：默认增量（只重解析变化文件）；full=true 忽略缓存全量重建 */
  refreshIndex: (full = false) => invoke<IndexMeta>("refresh_index", { full }),

  buildExport: (p: ExportParams) =>
    invoke<ExportResult>("build_prompt_export", {
      startDate: p.startDate,
      endDate: p.endDate,
      project: p.project,
      includeCommands: p.includeCommands,
      groupBy: p.groupBy,
      agentFilter: p.agentFilter,
      write: p.write,
      lang: p.lang,
    }),

  exportSearchResults: (p: {
    query: string;
    projectFilter: string | null;
    includeCommands: boolean;
    agentFilter: AgentFilter;
    write: boolean;
    lang?: string;
  }) =>
    invoke<ExportResult>("export_search_results", {
      query: p.query,
      projectFilter: p.projectFilter,
      includeCommands: p.includeCommands,
      agentFilter: p.agentFilter,
      write: p.write,
      lang: p.lang,
    }),

  exportConversation: (p: {
    agent: Agent;
    sessionId: string;
    includeTools: boolean;
    write: boolean;
    lang?: string;
  }) =>
    invoke<ConversationExportResult>("export_conversation", {
      sessionId: p.sessionId,
      agent: p.agent,
      includeTools: p.includeTools,
      write: p.write,
      lang: p.lang,
    }),

  /** 批量导出多个会话：merge=true 合成一份，否则每个会话一份并附 index.md；总是写入 ~/Downloads，进度走 `export-progress` 事件 */
  exportSessions: (p: {
    project: string;
    sessions: SessionRef[];
    merge: boolean;
    includeTools: boolean;
    lang?: string;
  }) =>
    invoke<SessionsExportResult>("export_sessions", {
      project: p.project,
      sessions: p.sessions,
      merge: p.merge,
      includeTools: p.includeTools,
      lang: p.lang,
    }),

  /** 读取详情里某个持久化的超大工具输出；path 是详情返回的相对路径，后端校验它在 projects 目录内 */
  readPersistedOutput: (path: string) =>
    invoke<PersistedOutputText>("read_persisted_output", { path }),

  /** 导入第一步（只读）：列出 zip 里的云端项目、会话数与映射建议 */
  inspectSessionImport: (zipPath: string) =>
    invoke<ImportInspection>("inspect_session_import", { zipPath }),

  /** 导入第二步（只读）：按映射生成计划，逐会话给出新增 / 更新 / 跳过 / 冲突 */
  planSessionImport: (zipPath: string, mappings: ProjectMapping[]) =>
    invoke<ImportPlan>("plan_session_import", { zipPath, mappings }),

  /** 导入第三步：按计划与冲突决定写入 projects 目录，记住映射并增量重建索引 */
  applySessionImport: (
    zipPath: string,
    mappings: ProjectMapping[],
    decisions: ConflictDecision[]
  ) =>
    invoke<ImportResult>("apply_session_import", {
      zipPath,
      mappings,
      decisions,
    }),

  getSettings: () => invoke<SettingsView>("get_settings"),

  setSettings: (settings: SettingsInput) =>
    invoke<SettingsView>("set_settings", { settings }),

  revealPath: (path: string) => invoke<void>("reveal_path", { path }),
};

/** 把后端返回的错误统一转成可读字符串 */
export function errMessage(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return translate("unknownError");
}
