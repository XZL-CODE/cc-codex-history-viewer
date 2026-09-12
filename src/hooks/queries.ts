// 基于 TanStack Query 的数据请求 hooks。

import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { api } from "@/lib/api";
import type { Agent, AgentFilter, ExportGroupBy, SortMode } from "@/lib/types";
import type { ResolvedRange } from "@/lib/statsRange";
import { useDebounce } from "./useDebounce";

const FIVE_MIN = 5 * 60 * 1000;

export function useProjects(agentFilter: AgentFilter) {
  return useQuery({
    queryKey: ["projects", agentFilter],
    queryFn: () => api.getProjects(agentFilter),
    staleTime: FIVE_MIN,
  });
}

/** 统计；切换范围时保留上一份数据（降低透明度）而不是闪成骨架屏。 */
export function useStats(agentFilter: AgentFilter, range?: ResolvedRange) {
  const start = range?.start ?? null;
  const end = range?.end ?? null;
  return useQuery({
    queryKey: ["stats", agentFilter, start, end],
    queryFn: () => api.getStats(agentFilter, start, end),
    staleTime: FIVE_MIN,
    placeholderData: keepPreviousData,
  });
}

export function useIndexMeta() {
  return useQuery({
    queryKey: ["index-meta"],
    queryFn: api.getIndexMeta,
    staleTime: FIVE_MIN,
  });
}

/** 设置（数据源目录）。每次打开都重新读取，保证 resolved 路径状态新鲜。 */
export function useSettings(enabled = true) {
  return useQuery({
    queryKey: ["settings"],
    queryFn: api.getSettings,
    enabled,
    staleTime: 0,
  });
}

export function useRecentPrompts(
  limit: number,
  includeCommands: boolean,
  agentFilter: AgentFilter
) {
  return useQuery({
    queryKey: ["recent-prompts", limit, includeCommands, agentFilter],
    queryFn: () => api.getRecentPrompts(limit, includeCommands, agentFilter),
    staleTime: FIVE_MIN,
  });
}

export function useProjectPrompts(
  project: string | null,
  sort: SortMode,
  includeCommands: boolean,
  agentFilter: AgentFilter
) {
  return useQuery({
    queryKey: ["project-prompts", project, sort, includeCommands, agentFilter],
    queryFn: () =>
      api.getProjectPrompts(project as string, sort, includeCommands, agentFilter),
    enabled: !!project,
    staleTime: FIVE_MIN,
  });
}

export function useProjectSessions(
  project: string | null,
  agentFilter: AgentFilter
) {
  return useQuery({
    queryKey: ["project-sessions", project, agentFilter],
    queryFn: () => api.getProjectSessions(project as string, agentFilter),
    enabled: !!project,
    staleTime: FIVE_MIN,
  });
}

export function useConversation(agent: Agent | null, sessionId: string | null) {
  return useQuery({
    queryKey: ["conversation", agent, sessionId],
    queryFn: () => api.getConversation(agent as Agent, sessionId as string),
    enabled: !!agent && !!sessionId,
    staleTime: FIVE_MIN,
  });
}

/** 导出预览：仅生成统计与截断预览，不写文件。输入变化时自动重算。 */
export function useExportPreview(params: {
  startDate: string;
  endDate: string;
  project: string | null;
  includeCommands: boolean;
  groupBy: ExportGroupBy;
  agentFilter: AgentFilter;
  lang: string;
  enabled: boolean;
}) {
  const {
    startDate,
    endDate,
    project,
    includeCommands,
    groupBy,
    agentFilter,
    lang,
    enabled,
  } = params;
  return useQuery({
    queryKey: [
      "export-preview",
      startDate,
      endDate,
      project,
      includeCommands,
      groupBy,
      agentFilter,
      lang,
    ],
    queryFn: () =>
      api.buildExport({
        startDate,
        endDate,
        project,
        includeCommands,
        groupBy,
        agentFilter,
        lang,
        write: false,
      }),
    enabled,
    staleTime: 30 * 1000,
  });
}

export function useSearch(
  query: string,
  projectFilter: string | null,
  includeCommands: boolean,
  agentFilter: AgentFilter
) {
  const debounced = useDebounce(query.trim(), 300);
  const result = useQuery({
    queryKey: ["search", debounced, projectFilter, includeCommands, agentFilter],
    queryFn: () =>
      api.searchPrompts(
        debounced,
        projectFilter,
        includeCommands,
        agentFilter
      ),
    enabled: debounced.length > 0,
    staleTime: 60 * 1000,
  });
  return { ...result, debouncedQuery: debounced };
}
