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
  ExportParams,
  ExportResult,
  IndexMeta,
  ProjectInfo,
  PromptEntry,
  SearchResult,
  SessionSummary,
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

  getStats: (agentFilter: AgentFilter) =>
    invoke<AppStats>("get_stats", { agentFilter }),

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
