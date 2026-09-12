import { useMemo, useState, type ReactNode } from "react";
import { NavLink } from "react-router-dom";
import { Download, Folder, Home, Search, X } from "lucide-react";
import { useProjects } from "@/hooks/queries";
import { useStore } from "@/store";
import { useT } from "@/i18n";
import { cn, encodePath, formatNumber } from "@/lib/utils";
import { Skeleton } from "./ui";
import { AgentBadge, AgentFilterControl } from "./AgentBadge";

function NavigationItem({
  to,
  end,
  icon,
  children,
}: {
  to: string;
  end?: boolean;
  icon: ReactNode;
  children: ReactNode;
}) {
  return (
    <NavLink
      to={to}
      end={end}
      className={({ isActive }) =>
        cn(
          "relative flex h-9 items-center gap-2.5 rounded-lg px-3 text-[13px] font-medium transition-colors",
          isActive
            ? "bg-accent/15 text-accent"
            : "text-foreground hover:bg-surface-2/70"
        )
      }
    >
      {({ isActive }) => (
        <>
          {isActive && (
            <span
              aria-hidden
              className="absolute bottom-1.5 left-0 top-1.5 w-[3px] rounded-r bg-accent"
            />
          )}
          {icon}
          <span>{children}</span>
        </>
      )}
    </NavLink>
  );
}

export function Sidebar() {
  const { sidebarAgentFilter, setSidebarAgentFilter, setProjectAgentFilter } =
    useStore();
  const { data: projects, isLoading } = useProjects(sidebarAgentFilter);
  const t = useT();
  const [filter, setFilter] = useState("");

  const filtered = useMemo(() => {
    if (!projects) return [];
    const value = filter.trim().toLowerCase();
    if (!value) return projects;
    return projects.filter(
      (project) =>
        project.name.toLowerCase().includes(value) ||
        project.path.toLowerCase().includes(value)
    );
  }, [projects, filter]);

  return (
    <aside className="flex min-h-0 min-w-0 flex-col border-r border-border bg-surface">
      <nav className="grid shrink-0 gap-1 px-2.5 pb-2 pt-2.5">
        <NavigationItem to="/" end icon={<Home size={16} />}>
          {t("navHome")}
        </NavigationItem>
        <NavigationItem to="/export" icon={<Download size={16} />}>
          {t("navExport")}
        </NavigationItem>
      </nav>

      <div className="grid shrink-0 gap-2.5 px-2.5 pb-2.5">
        <div className="grid gap-1.5">
          <span className="px-0.5 text-[11px] font-medium text-muted">
            {t("sidebarAgentSource")}
          </span>
          <AgentFilterControl
            value={sidebarAgentFilter}
            onChange={setSidebarAgentFilter}
            compact
            ariaLabel={t("sidebarAgentSource")}
            className="w-full [&>button]:flex-1 [&>button]:px-2"
          />
        </div>

        <div className="relative">
          <Search
            size={14}
            className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-muted"
          />
          <input
            value={filter}
            onChange={(event) => setFilter(event.target.value)}
            placeholder={t("filterFoldersPlaceholder")}
            aria-label={t("filterFoldersPlaceholder")}
            className="h-8 w-full rounded-lg border border-border bg-background pl-8 pr-7 text-xs text-foreground outline-none transition-colors placeholder:text-muted focus:border-accent focus:ring-2 focus:ring-ring/20"
          />
          {filter && (
            <button
              type="button"
              onClick={() => setFilter("")}
              title={t("clearFolderFilter")}
              className="absolute right-2 top-1/2 flex h-5 w-5 -translate-y-1/2 items-center justify-center rounded text-muted hover:bg-surface-2 hover:text-foreground"
            >
              <X size={13} />
            </button>
          )}
        </div>
      </div>

      <div className="flex shrink-0 items-center justify-between px-3.5 pb-1 text-[11px] font-semibold text-muted">
        <span>{t("foldersSection")}</span>
        <span>{formatNumber(filtered.length)}</span>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-1.5 pb-3">
        {isLoading ? (
          <div className="space-y-1.5 p-1">
            {Array.from({ length: 12 }).map((_, index) => (
              <Skeleton key={index} className="h-[58px] w-full" />
            ))}
          </div>
        ) : filtered.length === 0 ? (
          <div className="px-3 py-8 text-center text-xs text-muted">
            {t("noMatchingFolders")}
          </div>
        ) : (
          filtered.map((project) => (
            <NavLink
              key={project.path}
              to={`/project/${encodePath(project.path)}`}
              onClick={() => setProjectAgentFilter("all")}
              title={project.path}
              className={({ isActive }) =>
                cn(
                  "relative block rounded-lg px-3 py-2 transition-colors",
                  isActive ? "bg-accent/15" : "hover:bg-surface-2/70"
                )
              }
            >
              {({ isActive }) => (
                <>
                  {isActive && (
                    <span
                      aria-hidden
                      className="absolute bottom-1.5 left-0 top-1.5 w-[3px] rounded-r bg-accent"
                    />
                  )}
                  <div className="flex min-w-0 items-center gap-2">
                    <Folder size={14} className="shrink-0 text-muted" />
                    <span className="truncate text-[13px] font-medium text-foreground">
                      {project.name}
                    </span>
                  </div>
                  <div className="mt-0.5 flex items-center gap-1.5 pl-[22px] text-[11px] text-muted">
                    <span>
                      {t("promptCountLabel", {
                        count: formatNumber(project.promptCount),
                      })}
                    </span>
                    {project.hasConversations && (
                      <span>
                        · {t("sessionCountLabel", { count: project.sessionCount })}
                      </span>
                    )}
                  </div>
                  {sidebarAgentFilter === "all" && (
                    <div className="mt-1 flex flex-wrap gap-1 pl-[22px]">
                      {project.agents.map((agent) => (
                        <AgentBadge key={agent} agent={agent} />
                      ))}
                    </div>
                  )}
                </>
              )}
            </NavLink>
          ))
        )}
      </div>
    </aside>
  );
}
