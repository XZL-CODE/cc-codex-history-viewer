// 基于 TanStack Query 的数据请求 hooks。

import { useCallback } from "react";
import {
  keepPreviousData,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { api } from "@/lib/api";
import type {
  Agent,
  AgentFilter,
  ExportGroupBy,
  ExternalSessionList,
  ExternalView,
  SortMode,
} from "@/lib/types";
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

/** 外部会话列表（临时导入 + 已保留），按导入时间倒序 */
export function useExternalSessions() {
  return useQuery({
    queryKey: ["external-sessions"],
    queryFn: api.listExternalSessions,
    staleTime: FIVE_MIN,
  });
}

/** 外部会话详情；切换「完整 / 只看对话」时保留上一份内容，换到别的会话时不保留 */
export function useExternalConversation(id: string | null, view: ExternalView) {
  return useQuery({
    queryKey: ["external-session", id, view],
    queryFn: () => api.getExternalSession(id as string, view),
    enabled: !!id,
    staleTime: FIVE_MIN,
    placeholderData: (previous, previousQuery) =>
      previousQuery?.queryKey[1] === id ? previous : undefined,
  });
}

/**
 * 导入 / 保留 / 删除外部会话后刷新列表与详情。
 * removedId 是刚删除的条目：它的详情直接移出缓存，避免对已不存在的会话重新请求；
 * 缓存的列表也先去掉它，回到列表页时不会闪现已删除的条目。
 */
export function useRefreshExternalSessions() {
  const queryClient = useQueryClient();
  return useCallback(
    async (removedId?: string) => {
      if (removedId) {
        queryClient.removeQueries({ queryKey: ["external-session", removedId] });
        queryClient.setQueryData<ExternalSessionList>(
          ["external-sessions"],
          (list) =>
            list && {
              ...list,
              sessions: list.sessions.filter(
                (session) => session.id !== removedId
              ),
            }
        );
      }
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["external-sessions"] }),
        queryClient.invalidateQueries({ queryKey: ["external-session"] }),
      ]);
    },
    [queryClient]
  );
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

/** 全文搜索会话内容。扫描较重，用更长的防抖，并且只在启用时执行。 */
export function useConversationSearch(
  query: string,
  projectFilter: string | null,
  agentFilter: AgentFilter,
  enabled: boolean
) {
  const debounced = useDebounce(query.trim(), 600);
  const result = useQuery({
    queryKey: ["conversation-search", debounced, projectFilter, agentFilter],
    queryFn: () => api.searchConversations(debounced, projectFilter, agentFilter),
    enabled: enabled && debounced.length > 0,
    staleTime: 60 * 1000,
  });
  return { ...result, debouncedQuery: debounced };
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
