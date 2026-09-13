// 搜索页的 URL 状态：搜索词、范围、文件夹与 Agent 筛选都放在查询参数里，
// 这样返回键能回到结果列表，刷新或分享链接也不丢失上下文。

import type { AgentFilter } from "./types";

export type SearchScope = "global" | "folder";
export type SearchMode = "prompts" | "content";

export interface SearchState {
  q: string;
  scope: SearchScope;
  /** scope=folder 时的文件夹真实路径 */
  project: string | null;
  agent: AgentFilter;
  /** prompts：只搜 Prompt；content：全文扫描会话内容 */
  mode: SearchMode;
}

export const SEARCH_PATH = "/search";

export function parseSearchParams(search: string): SearchState {
  const params = new URLSearchParams(search);
  const q = params.get("q") ?? "";
  const project = params.get("project") || null;
  const scope: SearchScope =
    params.get("scope") === "folder" && project ? "folder" : "global";
  const agentRaw = params.get("agent");
  const agent: AgentFilter =
    agentRaw === "claude" || agentRaw === "codex" ? agentRaw : "all";
  const mode: SearchMode = params.get("mode") === "content" ? "content" : "prompts";
  return { q, scope, project, agent, mode };
}

export function buildSearchUrl(state: SearchState): string {
  const params = new URLSearchParams();
  params.set("q", state.q);
  if (state.scope === "folder" && state.project) {
    params.set("scope", "folder");
    params.set("project", state.project);
  }
  if (state.agent !== "all") params.set("agent", state.agent);
  if (state.mode === "content") params.set("mode", "content");
  return `${SEARCH_PATH}?${params.toString()}`;
}

/** react-router 在 history.state 里记录的序号；0 表示没有可返回的上一页。 */
export function historyIndex(): number {
  try {
    const idx = (window.history.state as { idx?: number } | null)?.idx;
    return typeof idx === "number" ? idx : 0;
  } catch {
    return 0;
  }
}
