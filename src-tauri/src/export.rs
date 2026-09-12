//! 导出：Prompt 批量导出 与 整段对话导出，均为「在已有数据上过滤排序 + Markdown 拼接」。
//! 不重新解析任何 JSONL —— 数据全部来自 indexer 产出的 PromptEntry / parser 产出的 ConversationDetail。
//! 所有用户可见文案支持 zh / en（由 lang 参数控制，默认 zh）。

use crate::models::{
    Agent, AgentFilter, ContentBlock, ConversationDetail, PromptEntry, PromptOrigin,
};
use chrono::{Datelike, Local, NaiveDate, TimeZone};
use std::collections::HashMap;

/// 单条 prompt 正文超过此字符数仍原样保留（导出追求「完整」，不截断）。
/// 仅用于在「预览」里限制返回给前端的长度。
const PREVIEW_MAX_CHARS: usize = 12_000;

// ----------------------------- 语言 -----------------------------

/// 导出文案语言；前端传 "en" 走英文，其余一律中文。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Zh,
    En,
}

impl Lang {
    pub fn from_opt(s: Option<&str>) -> Self {
        match s {
            Some(x) if x.eq_ignore_ascii_case("en") => Lang::En,
            _ => Lang::Zh,
        }
    }

    fn weekday_name(self, ts: i64) -> &'static str {
        let idx = Local
            .timestamp_millis_opt(ts)
            .single()
            .map(|dt| dt.weekday().num_days_from_monday())
            .unwrap_or(0) as usize;
        const ZH: [&str; 7] = ["周一", "周二", "周三", "周四", "周五", "周六", "周日"];
        const EN: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
        match self {
            Lang::Zh => ZH[idx.min(6)],
            Lang::En => EN[idx.min(6)],
        }
    }
}

// ----------------------------- Prompt 导出 -----------------------------

/// 导出参数
pub struct ExportParams<'a> {
    pub start_ms: i64,
    pub end_ms: i64,
    pub project: Option<&'a str>,
    pub include_commands: bool,
    /// "project" | "day" | "none"
    pub group_by: &'a str,
    /// 展示用的日期范围（YYYY-MM-DD）
    pub start_date: &'a str,
    pub end_date: &'a str,
    pub lang: Lang,
    pub agent_filter: AgentFilter,
}

/// 导出产物
pub struct ExportData {
    pub markdown: String,
    pub prompt_count: usize,
    pub folder_count: usize,
    pub day_count: usize,
    lang: Lang,
}

impl ExportData {
    /// 截断到预览上限，超出时追加省略提示。
    pub fn preview(&self) -> String {
        truncate_preview(&self.markdown, self.lang)
    }
}

/// 预览截断（prompt 导出与对话导出共用）。
pub fn truncate_preview(markdown: &str, lang: Lang) -> String {
    if markdown.chars().count() <= PREVIEW_MAX_CHARS {
        return markdown.to_string();
    }
    let head: String = markdown.chars().take(PREVIEW_MAX_CHARS).collect();
    let note = match lang {
        Lang::Zh => "\n\n…（预览已截断，导出的文件包含全部内容）",
        Lang::En => "\n\n… (preview truncated; the exported file contains everything)",
    };
    format!("{head}{note}")
}

/// 把 YYYY-MM-DD 解析为本地时区当天 00:00:00 的毫秒时间戳。
pub fn day_start_ms(date: &str) -> Option<i64> {
    let d = NaiveDate::parse_from_str(date.trim(), "%Y-%m-%d").ok()?;
    let naive = d.and_hms_opt(0, 0, 0)?;
    Local
        .from_local_datetime(&naive)
        .single()
        .map(|dt| dt.timestamp_millis())
}

/// 把 YYYY-MM-DD 解析为本地时区当天 23:59:59.999 的毫秒时间戳。
pub fn day_end_ms(date: &str) -> Option<i64> {
    let d = NaiveDate::parse_from_str(date.trim(), "%Y-%m-%d").ok()?;
    let naive = d.and_hms_milli_opt(23, 59, 59, 999)?;
    Local
        .from_local_datetime(&naive)
        .single()
        .map(|dt| dt.timestamp_millis())
}

/// 主入口：从全量 prompt 里过滤并生成 Markdown。
pub fn build(prompts: &[PromptEntry], p: &ExportParams) -> ExportData {
    let lang = p.lang;
    // 1. 过滤：时间范围 + 文件夹 + 命令开关
    let mut items: Vec<&PromptEntry> = prompts
        .iter()
        .filter(|e| e.timestamp >= p.start_ms && e.timestamp <= p.end_ms)
        .filter(|e| p.project.map_or(true, |pf| e.project == pf))
        .filter(|e| p.include_commands || !e.is_command)
        .filter(|e| p.agent_filter.includes(e.agent))
        .collect();
    items.sort_by_key(|e| e.timestamp);

    // 2. 统计
    let prompt_count = items.len();
    let mut day_set = std::collections::HashSet::new();
    let mut folder_set = std::collections::HashSet::new();
    for e in &items {
        day_set.insert(day_key(e.timestamp));
        folder_set.insert(e.project.clone());
    }
    let day_count = day_set.len();
    let folder_count = folder_set.len();

    // 3. 头部
    let mut md = String::new();
    match lang {
        Lang::Zh => {
            md.push_str("# Coding Agent Prompt 导出\n\n");
            md.push_str(&format!(
                "> **时间范围**　{} ~ {}\n",
                p.start_date, p.end_date
            ));
            md.push_str(&format!(
                "> **共**　{prompt_count} 条 prompt · {folder_count} 个文件夹 · 跨 {day_count} 天\n"
            ));
            md.push_str(&format!("> **导出于**　{}\n\n", now_label()));
        }
        Lang::En => {
            md.push_str("# Coding Agent Prompt Export\n\n");
            md.push_str(&format!(
                "> **Time range**　{} ~ {}\n",
                p.start_date, p.end_date
            ));
            md.push_str(&format!(
                "> **Total**　{prompt_count} prompts · {folder_count} folders · across {day_count} days\n"
            ));
            md.push_str(&format!("> **Exported at**　{}\n\n", now_label()));
        }
    }

    if items.is_empty() {
        md.push_str(match lang {
            Lang::Zh => "---\n\n_该范围内没有 prompt。_\n",
            Lang::En => "---\n\n_No prompts in this range._\n",
        });
        return ExportData {
            markdown: md,
            prompt_count,
            folder_count,
            day_count,
            lang,
        };
    }

    md.push_str("---\n\n");

    // 4. 正文
    match p.group_by {
        "day" => render_by_day(&mut md, &items, lang),
        "none" => render_flat(&mut md, &items, lang),
        _ => render_by_project(&mut md, &items, lang),
    }

    ExportData {
        markdown: md,
        prompt_count,
        folder_count,
        day_count,
        lang,
    }
}

// ----------------------------- 分组渲染 -----------------------------

/// 按文件夹分组：文件夹按 prompt 数量倒序，文件夹内按时间正序，正文完整保留。
fn render_by_project(md: &mut String, items: &[&PromptEntry], lang: Lang) {
    // 保持首次出现顺序收集分组，再按数量倒序
    let mut order: Vec<String> = Vec::new();
    let mut groups: HashMap<String, Vec<&PromptEntry>> = HashMap::new();
    for e in items {
        let key = e.project.clone();
        if !groups.contains_key(&key) {
            order.push(key.clone());
        }
        groups.entry(key).or_default().push(e);
    }
    order.sort_by(|a, b| groups[b].len().cmp(&groups[a].len()));

    for (i, proj) in order.iter().enumerate() {
        let list = &groups[proj];
        md.push_str(&format!("## 📁 {}\n\n", project_name(proj)));
        match lang {
            Lang::Zh => md.push_str(&format!("`{}` · {} 条\n\n", pretty_path(proj), list.len())),
            Lang::En => md.push_str(&format!(
                "`{}` · {} prompts\n\n",
                pretty_path(proj),
                list.len()
            )),
        }
        for e in list.iter() {
            // 文件夹内可能跨年，时间带上完整年份避免歧义
            push_prompt(md, e, "%Y-%m-%d %H:%M", lang);
        }
        if i + 1 < order.len() {
            md.push_str("---\n\n");
        }
    }
}

/// 按天分组：每天一个 ## 标题，天内按时间正序。
fn render_by_day(md: &mut String, items: &[&PromptEntry], lang: Lang) {
    let mut order: Vec<String> = Vec::new();
    let mut groups: HashMap<String, Vec<&PromptEntry>> = HashMap::new();
    for e in items {
        let key = day_key(e.timestamp);
        if !groups.contains_key(&key) {
            order.push(key.clone());
        }
        groups.entry(key).or_default().push(e);
    }
    // items 已按时间升序，order 自然是日期升序
    for (i, day) in order.iter().enumerate() {
        let list = &groups[day];
        let weekday = lang.weekday_name(list[0].timestamp);
        match lang {
            Lang::Zh => md.push_str(&format!("## {} {}（{} 条）\n\n", day, weekday, list.len())),
            Lang::En => md.push_str(&format!(
                "## {} {} ({} prompts)\n\n",
                day,
                weekday,
                list.len()
            )),
        }
        for e in list.iter() {
            push_prompt(md, e, "%H:%M", lang);
        }
        if i + 1 < order.len() {
            md.push_str("---\n\n");
        }
    }
}

/// 纯时间线：全局按时间正序。
fn render_flat(md: &mut String, items: &[&PromptEntry], lang: Lang) {
    for e in items {
        push_prompt(md, e, "%Y-%m-%d %H:%M", lang);
    }
}

/// 渲染单条 prompt：粗体时间 + 元信息标记，空行，完整正文。
fn push_prompt(md: &mut String, e: &PromptEntry, time_fmt: &str, lang: Lang) {
    let when = fmt_time(e.timestamp, time_fmt);
    md.push_str(&format!("**{when}**{}\n\n", meta_suffix(e, lang)));
    md.push_str(&format!("{}\n\n", e.text.trim()));
}

/// Product, origin, branch, command, and paste metadata.
fn meta_suffix(e: &PromptEntry, lang: Lang) -> String {
    let agent = match e.agent {
        Agent::Claude => "Claude Code",
        Agent::Codex => "Codex",
    };
    let origin = match (lang, e.origin) {
        (Lang::Zh, PromptOrigin::History) => "历史",
        (Lang::Zh, PromptOrigin::Conversation) => "会话",
        (Lang::Zh, PromptOrigin::Both) => "历史+会话",
        (Lang::En, PromptOrigin::History) => "history",
        (Lang::En, PromptOrigin::Conversation) => "conversation",
        (Lang::En, PromptOrigin::Both) => "history+conversation",
    };
    let mut s = format!(" · {agent} · {origin}");
    if let Some(b) = &e.git_branch {
        if !b.is_empty() {
            s.push_str(&format!(" · `{b}`"));
        }
    }
    if e.is_command {
        s.push_str(match lang {
            Lang::Zh => " · 命令",
            Lang::En => " · command",
        });
    }
    if e.pasted_count > 0 {
        match lang {
            Lang::Zh => s.push_str(&format!(" · 含 {} 段粘贴", e.pasted_count)),
            Lang::En => s.push_str(&format!(" · {} pasted", e.pasted_count)),
        }
    }
    s
}

// ----------------------------- 搜索结果导出 -----------------------------

/// 把一组搜索命中的 prompt 导出为 Markdown（按文件夹分组，组内按时间正序）。
/// 用于分析「某个关键词 / 命令都在哪些场景下被使用」。
pub fn build_search_export(
    items: &[&PromptEntry],
    query: &str,
    scope: Option<&str>,
    lang: Lang,
) -> ExportData {
    let mut sorted: Vec<&PromptEntry> = items.to_vec();
    sorted.sort_by_key(|e| e.timestamp);

    let prompt_count = sorted.len();
    let mut day_set = std::collections::HashSet::new();
    let mut folder_set = std::collections::HashSet::new();
    for e in &sorted {
        day_set.insert(day_key(e.timestamp));
        folder_set.insert(e.project.clone());
    }
    let day_count = day_set.len();
    let folder_count = folder_set.len();

    let mut md = String::new();
    match lang {
        Lang::Zh => {
            md.push_str("# 搜索结果导出\n\n");
            md.push_str(&format!("> **关键词**　`{query}`\n"));
            md.push_str(&format!(
                "> **范围**　{}\n",
                scope
                    .map(pretty_path)
                    .unwrap_or_else(|| "全部文件夹".to_string())
            ));
            md.push_str(&format!(
                "> **共**　{prompt_count} 条 prompt · {folder_count} 个文件夹 · 跨 {day_count} 天\n"
            ));
            md.push_str(&format!("> **导出于**　{}\n\n", now_label()));
        }
        Lang::En => {
            md.push_str("# Search Results Export\n\n");
            md.push_str(&format!("> **Keyword**　`{query}`\n"));
            md.push_str(&format!(
                "> **Scope**　{}\n",
                scope
                    .map(pretty_path)
                    .unwrap_or_else(|| "all folders".to_string())
            ));
            md.push_str(&format!(
                "> **Total**　{prompt_count} prompts · {folder_count} folders · across {day_count} days\n"
            ));
            md.push_str(&format!("> **Exported at**　{}\n\n", now_label()));
        }
    }

    if sorted.is_empty() {
        md.push_str(match lang {
            Lang::Zh => "---\n\n_没有命中的 prompt。_\n",
            Lang::En => "---\n\n_No matching prompts._\n",
        });
    } else {
        md.push_str("---\n\n");
        render_by_project(&mut md, &sorted, lang);
    }

    ExportData {
        markdown: md,
        prompt_count,
        folder_count,
        day_count,
        lang,
    }
}

// ----------------------------- 对话导出 -----------------------------

/// 把单个会话的完整对话渲染为 Markdown。
/// include_tools=false 时省略 thinking / tool_use / tool_result 块；
/// 一条消息若没有任何可渲染的块则整条跳过。
pub fn build_conversation_markdown(
    detail: &ConversationDetail,
    include_tools: bool,
    lang: Lang,
) -> String {
    let mut md = String::new();

    // 头部
    md.push_str(match lang {
        Lang::Zh => "# 对话导出\n\n",
        Lang::En => "# Conversation Export\n\n",
    });
    let label = |zh: &'static str, en: &'static str| match lang {
        Lang::Zh => zh,
        Lang::En => en,
    };
    if !detail.project.is_empty() {
        md.push_str(&format!(
            "> **{}**　`{}`\n",
            label("项目", "Project"),
            pretty_path(&detail.project)
        ));
    }
    md.push_str(&format!(
        "> **{}**　{}\n",
        label("产品", "Agent"),
        match detail.agent {
            Agent::Claude => "Claude Code",
            Agent::Codex => "OpenAI Codex",
        }
    ));
    if let Some(b) = detail.git_branch.as_deref().filter(|b| !b.is_empty()) {
        md.push_str(&format!("> **{}**　{}\n", label("分支", "Branch"), b));
    }
    if let Some(v) = detail.cli_version.as_deref().filter(|v| !v.is_empty()) {
        md.push_str(&format!(
            "> **{}**　{}\n",
            label("CLI 版本", "CLI version"),
            v
        ));
    }
    md.push_str(&format!(
        "> **{}**　{} ~ {}\n",
        label("时间", "Time"),
        fmt_time(detail.started_at, "%Y-%m-%d %H:%M"),
        fmt_time(detail.ended_at, "%Y-%m-%d %H:%M"),
    ));
    md.push_str(&format!(
        "> **{}**　{}\n",
        label("消息数", "Messages"),
        detail.messages.len()
    ));
    md.push_str(&format!(
        "> **{}**　`{}`\n\n---\n\n",
        label("会话 ID", "Session ID"),
        detail.session_id
    ));

    // 正文
    for m in &detail.messages {
        let mut body = String::new();
        for b in &m.blocks {
            render_block(&mut body, b, include_tools, lang);
        }
        if body.is_empty() {
            continue; // 整条消息没有可渲染内容（如仅含工具块且未勾选包含工具）
        }
        let who = if m.role == "user" {
            label("🧑 用户", "🧑 User")
        } else {
            match detail.agent {
                Agent::Claude => label("🤖 Claude", "🤖 Claude"),
                Agent::Codex => label("🤖 Codex", "🤖 Codex"),
            }
        };
        let side = if m.is_sidechain {
            label("（子代理）", " (subagent)")
        } else {
            ""
        };
        md.push_str(&format!(
            "## {} · {}{}\n\n",
            who,
            fmt_time(m.timestamp, "%Y-%m-%d %H:%M"),
            side
        ));
        md.push_str(&body);
    }
    md
}

/// 渲染单个内容块；不可见（未勾选工具）时不输出任何内容。
fn render_block(md: &mut String, b: &ContentBlock, include_tools: bool, lang: Lang) {
    let label = |zh: &'static str, en: &'static str| match lang {
        Lang::Zh => zh,
        Lang::En => en,
    };
    match b.kind.as_str() {
        "text" => {
            if let Some(t) = b.text.as_deref().filter(|t| !t.trim().is_empty()) {
                md.push_str(t.trim());
                md.push_str("\n\n");
            }
        }
        "image" => {
            md.push_str(&format!(
                "🖼 {}\n\n",
                b.text.as_deref().unwrap_or(label("[图片]", "[image]"))
            ));
        }
        "thinking" if include_tools => {
            md.push_str(&format!("**{}**\n\n", label("💭 思考过程", "💭 Thinking")));
            if let Some(t) = b.text.as_deref() {
                for line in t.trim().lines() {
                    md.push_str("> ");
                    md.push_str(line);
                    md.push('\n');
                }
                md.push('\n');
            }
        }
        "tool_use" if include_tools => {
            md.push_str(&format!(
                "**{} · {}**\n\n",
                label("🔧 工具调用", "🔧 Tool call"),
                b.tool_name.as_deref().unwrap_or("tool")
            ));
            let input = b
                .tool_input
                .as_ref()
                .and_then(|v| serde_json::to_string_pretty(v).ok())
                .unwrap_or_else(|| "{}".to_string());
            // 用四个反引号围栏，避免与内容里的 ``` 冲突
            md.push_str(&format!("````json\n{input}\n````\n\n"));
        }
        "tool_result" if include_tools => {
            md.push_str(&format!("**{}**\n\n", label("↩ 工具结果", "↩ Tool result")));
            let t = b.text.as_deref().unwrap_or("");
            md.push_str(&format!("````\n{}\n````\n\n", t.trim()));
        }
        _ => {}
    }
}

// ----------------------------- 时间 / 路径工具 -----------------------------

fn fmt_time(ts: i64, fmt: &str) -> String {
    Local
        .timestamp_millis_opt(ts)
        .single()
        .map(|dt| dt.format(fmt).to_string())
        .unwrap_or_default()
}

fn day_key(ts: i64) -> String {
    fmt_time(ts, "%Y-%m-%d")
}

fn now_label() -> String {
    Local::now().format("%Y-%m-%d %H:%M").to_string()
}

/// 取路径末级目录名作为展示名（与索引层共用同一规则，兼容 Windows 分隔符）。
fn project_name(path: &str) -> String {
    crate::indexer::project_name(path)
}

/// /Users/xxx/... → ~/...
fn pretty_path(path: &str) -> String {
    if let Some(home) = dirs::home_dir() {
        let home = home.to_string_lossy().to_string();
        if let Some(rest) = path.strip_prefix(&home) {
            return format!("~{rest}");
        }
    }
    path.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ChatMessage;

    fn mk(project: &str, ts: i64, text: &str, is_command: bool) -> PromptEntry {
        mk_agent(Agent::Claude, project, ts, text, is_command)
    }

    fn mk_agent(agent: Agent, project: &str, ts: i64, text: &str, is_command: bool) -> PromptEntry {
        PromptEntry {
            id: format!("{ts}"),
            agent,
            text: text.to_string(),
            project: project.to_string(),
            timestamp: ts,
            origin: PromptOrigin::Conversation,
            session_id: None,
            has_conversation: false,
            git_branch: None,
            is_command,
            pasted_count: 0,
            char_count: text.chars().count(),
        }
    }

    fn params<'a>(
        start: &'a str,
        end: &'a str,
        include_commands: bool,
        lang: Lang,
    ) -> ExportParams<'a> {
        ExportParams {
            start_ms: day_start_ms(start).unwrap(),
            end_ms: day_end_ms(end).unwrap(),
            project: None,
            include_commands,
            group_by: "project",
            start_date: start,
            end_date: end,
            lang,
            agent_filter: AgentFilter::All,
        }
    }

    #[test]
    fn date_bounds_cover_full_local_day() {
        let s = day_start_ms("2026-05-16").unwrap();
        let e = day_end_ms("2026-05-16").unwrap();
        assert_eq!(e - s, 86_399_999, "一天应覆盖 23:59:59.999");
        // 次日 00:00 应紧接当日末尾之后 1ms
        let next = day_start_ms("2026-05-17").unwrap();
        assert_eq!(next - e, 1);
    }

    #[test]
    fn filters_range_and_commands_then_groups_by_count() {
        let d16 = day_start_ms("2026-05-16").unwrap();
        let d17 = day_start_ms("2026-05-17").unwrap();
        let d18 = day_start_ms("2026-05-18").unwrap();
        let prompts = vec![
            mk("/p/alpha", d16 + 3_600_000, "你好，自我介绍一下", false),
            mk("/p/alpha", d16 + 7_200_000, "抽出 parser", false),
            mk("/p/beta", d17 + 1_000, "跑测试", false),
            mk("/p/alpha", d18 + 1_000, "范围外不应出现", false), // 越界
            mk("/p/alpha", d16 + 100, "/clear", true),            // 命令
        ];

        // 默认不含命令
        let out = build(
            &prompts,
            &params("2026-05-16", "2026-05-17", false, Lang::Zh),
        );
        assert_eq!(out.prompt_count, 3);
        assert_eq!(out.folder_count, 2);
        assert_eq!(out.day_count, 2);
        assert!(out.markdown.contains("# Coding Agent Prompt 导出"));
        assert!(out.markdown.contains("你好，自我介绍一下"));
        assert!(
            !out.markdown.contains("范围外不应出现"),
            "越界 prompt 不应导出"
        );
        assert!(!out.markdown.contains("/clear"), "默认不含命令");
        // alpha(2 条) 应排在 beta(1 条) 之前
        let ia = out.markdown.find("alpha").unwrap();
        let ib = out.markdown.find("beta").unwrap();
        assert!(ia < ib, "数量多的文件夹应在前");

        // 打开命令开关
        let out2 = build(
            &prompts,
            &params("2026-05-16", "2026-05-17", true, Lang::Zh),
        );
        assert_eq!(out2.prompt_count, 4);
        assert!(out2.markdown.contains("/clear"));
        assert!(out2.markdown.contains("命令"));
    }

    #[test]
    fn empty_range_reports_zero() {
        let prompts = vec![mk("/p/a", day_start_ms("2026-01-01").unwrap(), "x", false)];
        let out = build(
            &prompts,
            &params("2026-05-16", "2026-05-17", false, Lang::Zh),
        );
        assert_eq!(out.prompt_count, 0);
        assert!(out.markdown.contains("没有 prompt"));
    }

    #[test]
    fn agent_filter_and_export_metadata_preserve_product_identity() {
        let day = day_start_ms("2026-05-16").unwrap();
        let prompts = vec![
            mk_agent(
                Agent::Claude,
                "/p/shared",
                day + 1_000,
                "claude-only",
                false,
            ),
            mk_agent(Agent::Codex, "/p/shared", day + 2_000, "codex-only", false),
        ];
        let mut parameters = params("2026-05-16", "2026-05-16", true, Lang::En);
        parameters.agent_filter = AgentFilter::Codex;
        let out = build(&prompts, &parameters);
        assert_eq!(out.prompt_count, 1);
        assert!(out.markdown.contains("codex-only"));
        assert!(out.markdown.contains("Codex · conversation"));
        assert!(!out.markdown.contains("claude-only"));
    }

    #[test]
    fn english_labels_render() {
        let d16 = day_start_ms("2026-05-16").unwrap();
        let prompts = vec![
            mk("/p/alpha", d16 + 1_000, "hello world", false),
            mk("/p/alpha", d16 + 2_000, "/clear", true),
        ];
        let out = build(
            &prompts,
            &params("2026-05-16", "2026-05-16", true, Lang::En),
        );
        assert!(out.markdown.contains("# Coding Agent Prompt Export"));
        assert!(out.markdown.contains("Time range"));
        assert!(out.markdown.contains("prompts"));
        assert!(out.markdown.contains("command"));
        assert!(!out.markdown.contains("导出"));
    }

    fn text_block(t: &str) -> ContentBlock {
        ContentBlock {
            kind: "text".into(),
            text: Some(t.to_string()),
            tool_name: None,
            tool_input: None,
            truncated: false,
        }
    }

    #[test]
    fn conversation_markdown_respects_include_tools() {
        let detail = ConversationDetail {
            agent: Agent::Claude,
            session_id: "sess-1".into(),
            project: "/p/alpha".into(),
            git_branch: Some("main".into()),
            started_at: day_start_ms("2026-05-16").unwrap(),
            ended_at: day_start_ms("2026-05-16").unwrap() + 60_000,
            cli_version: Some("2.0.0".into()),
            source: Some("cli".into()),
            models: vec!["claude-test".into()],
            messages: vec![
                ChatMessage {
                    agent: Agent::Claude,
                    uuid: "u1".into(),
                    role: "user".into(),
                    timestamp: day_start_ms("2026-05-16").unwrap(),
                    is_sidechain: false,
                    blocks: vec![text_block("帮我重构")],
                },
                ChatMessage {
                    agent: Agent::Claude,
                    uuid: "a1".into(),
                    role: "assistant".into(),
                    timestamp: day_start_ms("2026-05-16").unwrap() + 1_000,
                    is_sidechain: false,
                    blocks: vec![
                        ContentBlock {
                            kind: "thinking".into(),
                            text: Some("想一想".into()),
                            tool_name: None,
                            tool_input: None,
                            truncated: false,
                        },
                        ContentBlock {
                            kind: "tool_use".into(),
                            text: None,
                            tool_name: Some("Bash".into()),
                            tool_input: Some(serde_json::json!({"command": "ls"})),
                            truncated: false,
                        },
                    ],
                },
                ChatMessage {
                    agent: Agent::Claude,
                    uuid: "a2".into(),
                    role: "assistant".into(),
                    timestamp: day_start_ms("2026-05-16").unwrap() + 2_000,
                    is_sidechain: false,
                    blocks: vec![text_block("已完成")],
                },
            ],
        };

        // 不含工具：纯工具消息整条消失
        let md = build_conversation_markdown(&detail, false, Lang::Zh);
        assert!(md.contains("# 对话导出"));
        assert!(md.contains("帮我重构"));
        assert!(md.contains("已完成"));
        assert!(!md.contains("思考过程"));
        assert!(!md.contains("Bash"));
        // 仅 2 条消息标题（工具消息被跳过）
        assert_eq!(md.matches("## ").count(), 2);

        // 含工具：thinking 引用块 + tool_use 围栏
        let md2 = build_conversation_markdown(&detail, true, Lang::Zh);
        assert!(md2.contains("💭 思考过程"));
        assert!(md2.contains("> 想一想"));
        assert!(md2.contains("🔧 工具调用 · Bash"));
        assert!(md2.contains("````json"));
        assert_eq!(md2.matches("## ").count(), 3);

        // 英文文案
        let md3 = build_conversation_markdown(&detail, true, Lang::En);
        assert!(md3.contains("# Conversation Export"));
        assert!(md3.contains("🧑 User"));
        assert!(md3.contains("💭 Thinking"));

        let mut codex_detail = detail.clone();
        codex_detail.agent = Agent::Codex;
        for message in &mut codex_detail.messages {
            message.agent = Agent::Codex;
        }
        let codex = build_conversation_markdown(&codex_detail, true, Lang::En);
        assert!(codex.contains("> **Agent**　OpenAI Codex"));
        assert!(codex.contains("## 🤖 Codex"));
        assert!(codex.contains("Tool call · Bash"));
    }
}
