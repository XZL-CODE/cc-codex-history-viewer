//! 外部会话：临时查看别人用本应用导出的单会话 Markdown（`# 对话导出` / `# Conversation Export`）。
//!
//! 导出文件被解析回 [`ConversationDetail`]，交给现有的对话详情界面展示。边界：
//! - 只读取用户选中的 Markdown 文件，绝不写入 ~/.claude 或 ~/.codex，也不进入索引、
//!   统计、Prompt 搜索与全文搜索。
//! - 导入默认只保存在本次运行的内存里，退出即消失；用户点「保留」后才把原始 Markdown
//!   复制到应用数据目录 `external_sessions/<id>/`，下次启动时懒加载；删除只移除该目录。
//! - 同一会话（产品 + 会话 ID）的详细版（含执行过程）与简洁版合并为一个条目。
//!
//! 解析是 [`crate::export::build_conversation_markdown`] 的逆过程，全部为纯函数，不依赖 Tauri。

use crate::export;
use crate::models::{
    Agent, ChatMessage, ContentBlock, ConversationDetail, ExternalImportItem, ExternalImportResult,
    ExternalSessionDetail, ExternalSessionList, ExternalSessionSummary, ExternalVariant,
    ExternalView,
};
use chrono::{Local, NaiveDateTime, TimeZone};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::VecDeque;
use std::fs::{self, File};
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

// ----------------------------- 解析 -----------------------------

/// 单个导入文件的大小上限。
const MAX_IMPORT_BYTES: u64 = 128 * 1024 * 1024;
/// 会话 ID 直接用作条目 ID 时的最大长度。
const MAX_SESSION_ID_CHARS: usize = 128;
/// 条目 ID（`{agent}-{sessionId}`）的最大长度。
const MAX_ID_CHARS: usize = 160;

const TITLES: [&str; 2] = ["# 对话导出", "# Conversation Export"];
const FENCE: &str = "````";
const JSON_FENCE: &str = "````json";
const TRUNCATED_NOTES: [&str; 2] = ["_（内容过长，已截断）_", "_(content truncated)_"];
const FULL_OUTPUT_PREFIXES: [&str; 2] = ["_完整输出来自 `", "_Full output from `"];
const IMAGE_PREFIX: &str = "🖼 ";
/// 导出器给没有文字的图片写的占位文案，解析时还原成 None，由前端显示自己的占位。
const IMAGE_FALLBACKS: [&str; 2] = ["[图片]", "[image]"];
const SIDECHAIN_SUFFIXES: [&str; 2] = ["（子代理）", " (subagent)"];
/// Codex 注入在提问前的上下文包装标题（去掉末尾冒号后比较）。
const CONTEXT_WRAPPERS: [&str; 2] = [
    "# Context from my IDE setup",
    "# Files mentioned by the user",
];
/// 包装之后真正提问的标题（去掉末尾冒号后比较）。
const REQUEST_HEADINGS: [&str; 2] = ["## My request for Codex", "## My request"];
/// 本地 Codex 解析器为这些调用只生成工具调用块、不生成结果（image_generation 只有带图时才有）。
const RESULTLESS_TOOLS: [&str; 2] = ["web_search", "image_generation"];
/// 标题只取一行，长度兜底；前端再做纯文本摘要与截短。
const TITLE_MAX_CHARS: usize = 300;
/// 消息标题里的说话人：(文案, role, 助手消息的产品)。
const SPEAKERS: [(&str, &str, Option<Agent>); 4] = [
    ("🧑 用户", "user", None),
    ("🧑 User", "user", None),
    ("🤖 Claude", "assistant", Some(Agent::Claude)),
    ("🤖 Codex", "assistant", Some(Agent::Codex)),
];

const ERR_NOT_EXPORT: &str = "不是本应用导出的单会话 Markdown（缺少「# 对话导出」标题）。";
const ERR_BATCH_EXPORT: &str = "这是批量导出的 Markdown（会话导出），目前只支持单会话导出的文件。";
const ERR_PROMPT_EXPORT: &str = "这是 Prompt 或搜索结果导出，不包含完整对话。";
const ERR_UNKNOWN_AGENT: &str = "无法识别导出文件的产品（Claude Code / OpenAI Codex）。";
const ERR_NOT_FOUND: &str = "外部会话不存在或已被删除。";
const ERR_INVALID_ID: &str = "外部会话 ID 无效。";

/// 一份单会话导出的解析结果。
#[derive(Debug, Clone)]
pub struct ParsedExport {
    /// source 为 None、models 为空、用量为默认值；块不做展示截断，`truncated` 只反映导出时的标注。
    pub detail: ConversationDetail,
    /// 首条真正的用户 Prompt（跳过 `<environment_context>` 之类的注入包装）；可能为空。
    pub title: String,
    /// 头部「消息数」：原会话的全部消息数，不是本文件里的消息数。
    pub meta_messages: Option<usize>,
    pub meta_tokens: Option<u64>,
    pub meta_cost_usd: Option<f64>,
    /// 含 thinking / tool_use / tool_result / attachment 任一块，即详细版。
    pub has_process: bool,
}

/// 头部引用块里的元信息。
#[derive(Debug, Default)]
struct Meta {
    project: String,
    agent: Option<Agent>,
    git_branch: Option<String>,
    cli_version: Option<String>,
    started_at: Option<i64>,
    ended_at: Option<i64>,
    messages: Option<usize>,
    tokens: Option<u64>,
    cost_usd: Option<f64>,
    session_id: String,
}

/// 一行消息标题 `## 🧑 用户 · 2026-01-01 10:00（子代理）`。
#[derive(Debug, Clone, Copy)]
struct Heading {
    role: &'static str,
    /// 只有助手消息的标题带产品
    agent: Option<Agent>,
    timestamp: Option<i64>,
    is_sidechain: bool,
}

/// 块标记行（不含图片与文本）。
enum Marker {
    Thinking,
    ToolUse(String),
    /// 新版导出在结果标题里带工具名；旧文件没有
    ToolResult(Option<String>),
    Attachment(String),
}

struct RawMessage {
    heading: Heading,
    blocks: Vec<ContentBlock>,
}

/// 解析单会话导出的 Markdown。
pub fn parse_markdown(text: &str) -> Result<ParsedExport, String> {
    parse_normalized(&normalize(text))
}

/// 去掉 BOM，把 CRLF 统一为 LF。单独的 CR 属于正文（导出时 `lines()` 不在 CR 处断行，
/// 思考块的一行可能含 CR），只有整个文件没有 LF 时才把 CR 当作换行。
fn normalize(text: &str) -> String {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let text = text.replace("\r\n", "\n");
    if text.contains('\n') {
        text
    } else {
        text.replace('\r', "\n")
    }
}

fn parse_normalized(text: &str) -> Result<ParsedExport, String> {
    let lines: Vec<&str> = text.split('\n').collect();
    let title_index = lines
        .iter()
        .position(|line| !line.trim().is_empty())
        .ok_or_else(|| ERR_NOT_EXPORT.to_string())?;
    check_title(lines[title_index].trim())?;
    let (meta, body_start) = parse_meta(&lines, title_index + 1);
    let raw = parse_body(&lines, body_start);
    let agent = meta
        .agent
        .or_else(|| raw.iter().find_map(|message| message.heading.agent))
        .ok_or_else(|| ERR_UNKNOWN_AGENT.to_string())?;

    // 标题里没有时间时沿用上一条消息的时间，第一条则用头部的开始时间
    let mut previous = meta.started_at;
    let mut messages = Vec::with_capacity(raw.len());
    for (index, message) in raw.into_iter().enumerate() {
        let timestamp = message.heading.timestamp.or(previous).unwrap_or(0);
        previous = Some(timestamp);
        messages.push(ChatMessage {
            agent,
            uuid: format!("ext-{index}"),
            role: message.heading.role.to_string(),
            timestamp,
            is_sidechain: message.heading.is_sidechain,
            blocks: message.blocks,
            usage: None,
        });
    }

    pair_tool_results(&mut messages);

    let started_at = meta
        .started_at
        .or_else(|| messages.first().map(|message| message.timestamp))
        .unwrap_or(0);
    let ended_at = meta
        .ended_at
        .or_else(|| messages.last().map(|message| message.timestamp))
        .unwrap_or(started_at);
    let has_process = messages
        .iter()
        .flat_map(|message| &message.blocks)
        .any(|block| is_process_kind(&block.kind));
    let title = session_title(&messages);
    Ok(ParsedExport {
        detail: ConversationDetail {
            agent,
            session_id: meta.session_id,
            project: meta.project,
            git_branch: meta.git_branch,
            started_at,
            ended_at,
            cli_version: meta.cli_version,
            source: None,
            models: Vec::new(),
            messages,
            usage: Default::default(),
            usage_breakdown: Default::default(),
        },
        title,
        meta_messages: meta.messages,
        meta_tokens: meta.tokens,
        meta_cost_usd: meta.cost_usd,
        has_process,
    })
}

/// 只接受单会话导出；其他导出给出明确原因。
fn check_title(title: &str) -> Result<(), String> {
    if TITLES.contains(&title) {
        return Ok(());
    }
    if title.starts_with("# 会话导出") || title.starts_with("# Sessions Export") {
        return Err(ERR_BATCH_EXPORT.to_string());
    }
    if title.starts_with("# Coding Agent Prompt")
        || title.starts_with("# 搜索结果导出")
        || title.starts_with("# Search Results Export")
    {
        return Err(ERR_PROMPT_EXPORT.to_string());
    }
    Err(ERR_NOT_EXPORT.to_string())
}

/// 解析标题后的 `> **标签**　值` 引用块，返回元信息与正文起始行（已跳过空行和一行 `---`）。
fn parse_meta(lines: &[&str], from: usize) -> (Meta, usize) {
    let mut meta = Meta::default();
    let mut i = next_non_blank(lines, from);
    while i < lines.len() {
        let Some(rest) = lines[i].trim_end().strip_prefix("> **") else {
            break;
        };
        // 标签后面是全角空格「　」；编辑器改成普通空格也接受（trim 同时去掉两者）
        if let Some((label, value)) = rest.split_once("**") {
            apply_meta(&mut meta, label.trim(), value.trim());
        }
        i += 1;
    }
    i = next_non_blank(lines, i);
    if i < lines.len() && lines[i].trim() == "---" {
        i += 1;
    }
    (meta, i)
}

/// 中英文标签都接受；未知标签忽略。
fn apply_meta(meta: &mut Meta, label: &str, value: &str) {
    match label {
        "项目" | "Project" => meta.project = strip_backticks(value).to_string(),
        "产品" | "Agent" => meta.agent = agent_from_label(value),
        "分支" | "Branch" => meta.git_branch = non_empty(value),
        "CLI 版本" | "CLI version" => meta.cli_version = non_empty(value),
        "时间" | "Time" => {
            let (start, end) = value.split_once('~').unwrap_or((value, ""));
            meta.started_at = parse_minute(start.trim());
            meta.ended_at = parse_minute(end.trim());
        }
        "消息数" | "Messages" => meta.messages = value.parse().ok(),
        "Token（含缓存）" | "Tokens (incl. cache)" => meta.tokens = value.parse().ok(),
        "API 等价估算成本" | "API-equivalent estimated cost" => {
            meta.cost_usd = value.trim_start_matches('$').trim().parse().ok();
        }
        "会话 ID" | "Session ID" => meta.session_id = strip_backticks(value).to_string(),
        _ => {}
    }
}

fn agent_from_label(value: &str) -> Option<Agent> {
    match value {
        "Claude Code" => Some(Agent::Claude),
        "OpenAI Codex" => Some(Agent::Codex),
        _ => {
            let lower = value.to_ascii_lowercase();
            if lower.contains("codex") {
                Some(Agent::Codex)
            } else if lower.contains("claude") {
                Some(Agent::Claude)
            } else {
                None
            }
        }
    }
}

fn strip_backticks(value: &str) -> &str {
    value
        .strip_prefix('`')
        .and_then(|inner| inner.strip_suffix('`'))
        .unwrap_or(value)
}

fn non_empty(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_string())
}

/// 导出里的时间是导出者本地时间 `%Y-%m-%d %H:%M`，按本机时区解释，界面上显示的就是文件里印的时间。
fn parse_minute(text: &str) -> Option<i64> {
    let naive = NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M").ok()?;
    Local
        .from_local_datetime(&naive)
        .earliest()
        .map(|time| time.timestamp_millis())
}

/// `YYYY-MM-DD HH:MM` 的严格形状检查（标题语法的一部分）。
fn is_minute_text(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 16
        && bytes.iter().enumerate().all(|(index, byte)| match index {
            4 | 7 => *byte == b'-',
            10 => *byte == b' ',
            13 => *byte == b':',
            _ => byte.is_ascii_digit(),
        })
}

/// 消息标题：`## ` + 说话人 + ` · ` + 时间（可为空）+ 可选的子代理后缀，必须是整行。
/// 用户正文里的普通 `## ...` 标题不会匹配。
fn parse_heading(line: &str) -> Option<Heading> {
    let rest = line.trim_end().strip_prefix("## ")?;
    let &(speaker, role, agent) = SPEAKERS
        .iter()
        .find(|(speaker, ..)| rest.starts_with(*speaker))?;
    let rest = &rest[speaker.len()..];
    // 时间为空且没有后缀时，行尾空格可能被编辑器去掉，` · ` 只剩 ` ·`
    let rest = if rest == " ·" {
        ""
    } else {
        rest.strip_prefix(" · ")?
    };
    let (time, is_sidechain) = match SIDECHAIN_SUFFIXES
        .iter()
        .find_map(|suffix| rest.strip_suffix(*suffix))
    {
        Some(time) => (time, true),
        None => (rest, false),
    };
    if !time.is_empty() && !is_minute_text(time) {
        return None;
    }
    Some(Heading {
        role,
        agent,
        timestamp: parse_minute(time),
        is_sidechain,
    })
}

fn is_heading(line: &str) -> bool {
    parse_heading(line).is_some()
}

/// 块标记行：`**💭 思考过程**`、`**🔧 工具调用 · NAME**`、`**↩ 工具结果**`（新版带 ` · NAME`）、
/// `**📎 附件 · NAME**`（含英文）。
fn parse_marker(line: &str) -> Option<Marker> {
    let inner = line.trim_end().strip_prefix("**")?.strip_suffix("**")?;
    match inner {
        "💭 思考过程" | "💭 Thinking" => return Some(Marker::Thinking),
        "↩ 工具结果" | "↩ Tool result" => return Some(Marker::ToolResult(None)),
        _ => {}
    }
    for prefix in ["↩ 工具结果 · ", "↩ Tool result · "] {
        if let Some(name) = inner.strip_prefix(prefix) {
            return Some(Marker::ToolResult(Some(name.to_string())));
        }
    }
    for prefix in ["🔧 工具调用 · ", "🔧 Tool call · "] {
        if let Some(name) = inner.strip_prefix(prefix) {
            return Some(Marker::ToolUse(name.to_string()));
        }
    }
    for prefix in ["📎 附件 · ", "📎 Attachment · "] {
        if let Some(name) = inner.strip_prefix(prefix) {
            return Some(Marker::Attachment(name.to_string()));
        }
    }
    None
}

fn is_fence(line: &str) -> bool {
    line.trim_end() == FENCE
}

fn is_truncated_note(line: &str) -> bool {
    TRUNCATED_NOTES.contains(&line.trim())
}

fn is_full_output_note(line: &str) -> bool {
    let line = line.trim();
    line.ends_with("`_")
        && FULL_OUTPUT_PREFIXES
            .iter()
            .any(|prefix| line.starts_with(*prefix))
}

/// 能紧跟在一个块之后的结构行：消息标题、块标记、截断说明、持久化输出来源说明。
fn is_structural(line: &str) -> bool {
    is_heading(line)
        || parse_marker(line).is_some()
        || is_truncated_note(line)
        || is_full_output_note(line)
}

fn is_process_kind(kind: &str) -> bool {
    matches!(kind, "thinking" | "tool_use" | "tool_result" | "attachment")
}

fn next_non_blank(lines: &[&str], from: usize) -> usize {
    (from..lines.len())
        .find(|&index| !lines[index].trim().is_empty())
        .unwrap_or(lines.len())
}

fn new_block(kind: &str, text: Option<String>, tool_name: Option<String>) -> ContentBlock {
    ContentBlock {
        kind: kind.to_string(),
        text,
        tool_name,
        tool_input: None,
        truncated: false,
        persisted_output: None,
    }
}

/// 逐行切分消息并解析消息体。消息体是一个带「待定文本缓冲」的状态机：
/// 块标记只有在后续结构成立时才算数，否则当作普通文本；相邻文本无法区分，合并成一块。
fn parse_body(lines: &[&str], start: usize) -> Vec<RawMessage> {
    let mut messages: Vec<RawMessage> = Vec::new();
    let mut current: Option<RawMessage> = None;
    let mut text: Vec<&str> = Vec::new();
    let mut i = start;
    while i < lines.len() {
        let line = lines[i];
        if let Some(heading) = parse_heading(line) {
            finish_message(current.take(), &mut text, &mut messages);
            current = Some(RawMessage {
                heading,
                blocks: Vec::new(),
            });
            i += 1;
            continue;
        }
        // 第一条消息之前的内容不属于任何消息
        let Some(message) = current.as_mut() else {
            i += 1;
            continue;
        };
        if let Some((block, next)) = parse_block_at(lines, i) {
            flush_text(&mut text, &mut message.blocks, false);
            message.blocks.push(block);
            i = next;
            continue;
        }
        let starts_paragraph = text.last().map_or(true, |last| last.trim().is_empty());
        let ends_paragraph =
            i + 1 >= lines.len() || lines[i + 1].trim().is_empty() || is_heading(lines[i + 1]);
        if starts_paragraph && ends_paragraph {
            if let Some(rest) = line.trim_end().strip_prefix(IMAGE_PREFIX) {
                flush_text(&mut text, &mut message.blocks, false);
                let label = (!IMAGE_FALLBACKS.contains(&rest)).then(|| rest.to_string());
                message.blocks.push(new_block("image", label, None));
                i += 1;
                continue;
            }
            // 文本后面独立成段的截断说明：结束当前文本块并标记截断
            if is_truncated_note(line) && text.iter().any(|buffered| !buffered.trim().is_empty()) {
                flush_text(&mut text, &mut message.blocks, true);
                i += 1;
                continue;
            }
        }
        text.push(line);
        i += 1;
    }
    finish_message(current, &mut text, &mut messages);
    messages
}

/// 收尾一条消息；导出器跳过了没有可渲染块的消息，这里同样丢弃空消息。
fn finish_message(
    current: Option<RawMessage>,
    text: &mut Vec<&str>,
    messages: &mut Vec<RawMessage>,
) {
    match current {
        Some(mut message) => {
            flush_text(text, &mut message.blocks, false);
            if !message.blocks.is_empty() {
                messages.push(message);
            }
        }
        None => text.clear(),
    }
}

fn flush_text(text: &mut Vec<&str>, blocks: &mut Vec<ContentBlock>, truncated: bool) {
    let joined = text.join("\n");
    text.clear();
    let trimmed = joined.trim();
    if !trimmed.is_empty() {
        let mut block = new_block("text", Some(trimmed.to_string()), None);
        block.truncated = truncated;
        blocks.push(block);
    }
}

/// 在第 i 行尝试解析一个块；成功时返回块与下一行位置（总是大于 i）。
fn parse_block_at(lines: &[&str], i: usize) -> Option<(ContentBlock, usize)> {
    match parse_marker(lines[i])? {
        Marker::Thinking => parse_thinking(lines, i),
        Marker::ToolUse(name) => {
            let open = next_non_blank(lines, i + 1);
            if lines.get(open)?.trim_end() != JSON_FENCE {
                return None;
            }
            let (end, next) = close_json_fence(lines, open);
            let raw = lines[open + 1..end].join("\n");
            let input = serde_json::from_str::<Value>(&raw).unwrap_or(Value::String(raw));
            let mut block = new_block("tool_use", None, Some(name));
            block.tool_input = Some(input);
            Some((block, next))
        }
        Marker::ToolResult(name) => parse_output_block(lines, i, "tool_result", name),
        Marker::Attachment(name) => parse_output_block(lines, i, "attachment", Some(name)),
    }
}

/// 思考块：标记后（可隔空行）是 `>` 引用行；或标记后只有空行、紧接结构行（空的思考块）。
/// 空的思考块导出为「标记 + 两个空行」，后面同一条消息里的文本也能接上。
fn parse_thinking(lines: &[&str], i: usize) -> Option<(ContentBlock, usize)> {
    let first = next_non_blank(lines, i + 1);
    let mut next = first;
    let text = if lines.get(first).is_some_and(|line| line.starts_with('>')) {
        let mut parts: Vec<&str> = Vec::new();
        while next < lines.len() && lines[next].starts_with('>') {
            let line = lines[next];
            // 空行导出为 `> `，行尾空格被编辑器去掉时只剩 `>`
            parts.push(
                line.strip_prefix("> ")
                    .or_else(|| line.strip_prefix('>'))
                    .unwrap_or(line),
            );
            next += 1;
        }
        parts.join("\n").trim().to_string()
    } else if first == i + 3
        || (first > i + 1 && (first >= lines.len() || is_structural(lines[first])))
    {
        String::new()
    } else {
        return None;
    };
    let truncated = consume_line(lines, &mut next, is_truncated_note);
    let mut block = new_block("thinking", Some(text), None);
    block.truncated = truncated;
    Some((block, next))
}

/// 工具结果与附件：四反引号围栏包住原文，之后可能跟截断说明与持久化输出来源（丢弃）。
fn parse_output_block(
    lines: &[&str],
    i: usize,
    kind: &str,
    tool_name: Option<String>,
) -> Option<(ContentBlock, usize)> {
    let open = next_non_blank(lines, i + 1);
    if !is_fence(lines.get(open)?) {
        return None;
    }
    let (end, mut next) = close_output_fence(lines, open);
    let body = lines[open + 1..end].join("\n");
    let truncated = consume_line(lines, &mut next, is_truncated_note);
    consume_line(lines, &mut next, is_full_output_note);
    let mut block = new_block(kind, Some(body), tool_name);
    block.truncated = truncated;
    Some((block, next))
}

/// 工具调用的 JSON 不可能出现单独一行 "````"，第一个即为结束围栏。
/// 返回 (内容结束位置, 下一行位置)；缺少结束围栏时内容延续到下一条消息标题。
fn close_json_fence(lines: &[&str], open: usize) -> (usize, usize) {
    for (index, line) in lines.iter().enumerate().skip(open + 1) {
        if is_fence(line) {
            return (index, index + 1);
        }
        if is_heading(line) {
            return (index, index);
        }
    }
    (lines.len(), lines.len())
}

/// 工具结果 / 附件正文是任意文本，自身可能含 "````" 行。候选结束围栏按顺序检查：
/// 1. 其后第一个非空行是结构行（或文件结尾）→ 选中；
/// 2. 否则（同一条消息里后面跟着正文），若从它到下一个消息标题或块标记之间没有别的候选 → 选中；
/// 3. 否则继续看下一个候选。
///
/// 没有任何候选时内容延续到下一条消息标题。返回 (内容结束位置, 下一行位置)。
fn close_output_fence(lines: &[&str], open: usize) -> (usize, usize) {
    for candidate in (open + 1..lines.len()).filter(|&index| is_fence(lines[index])) {
        let after = next_non_blank(lines, candidate + 1);
        if after >= lines.len() || is_structural(lines[after]) {
            return (candidate, candidate + 1);
        }
        if !fence_before_boundary(lines, candidate + 1) {
            return (candidate, candidate + 1);
        }
    }
    let end = (open + 1..lines.len())
        .find(|&index| is_heading(lines[index]))
        .unwrap_or(lines.len());
    (end, end)
}

/// 从 from 起、遇到消息标题或块标记之前，是否还有 "````" 行。
fn fence_before_boundary(lines: &[&str], from: usize) -> bool {
    for line in &lines[from..] {
        if is_fence(line) {
            return true;
        }
        if is_heading(line) || parse_marker(line).is_some() {
            return false;
        }
    }
    false
}

/// 跳过空行后若下一行满足条件则消费它。
fn consume_line(lines: &[&str], next: &mut usize, matches: fn(&str) -> bool) -> bool {
    let candidate = next_non_blank(lines, *next);
    if candidate < lines.len() && matches(lines[candidate]) {
        *next = candidate + 1;
        true
    } else {
        false
    }
}

/// 旧版导出不记录工具结果来自哪个工具（新版写在结果标题里，文件里只要出现过就不再推断）：
/// 按顺序把结果配给前面尚未配对的工具调用（Claude 与 Codex 都在调用之后紧接着写结果）。
/// 主线与子代理各用一个队列（旧版 Claude 会话把子代理消息穿插在主线里）；中间出现文本或思考
/// 说明这一轮调用已结束，清空该线的队列；从不产生结果的调用不入队。都是为了避免缺一个
/// 结果就让后面全部错位。
fn pair_tool_results(messages: &mut [ChatMessage]) {
    let named = messages
        .iter()
        .flat_map(|message| &message.blocks)
        .any(|block| block.kind == "tool_result" && block.tool_name.is_some());
    if named {
        return;
    }
    let mut pending: [VecDeque<String>; 2] = [VecDeque::new(), VecDeque::new()];
    for message in messages.iter_mut() {
        let queue = &mut pending[usize::from(message.is_sidechain)];
        for block in &mut message.blocks {
            match block.kind.as_str() {
                "tool_use" => {
                    let name = block.tool_name.clone().unwrap_or_default();
                    if !RESULTLESS_TOOLS.contains(&name.as_str()) {
                        queue.push_back(name);
                    }
                }
                "tool_result" => {
                    block.tool_name = queue.pop_front().filter(|name| !name.is_empty());
                }
                "text" | "thinking" => queue.clear(),
                _ => {}
            }
        }
    }
}

/// Codex 把 IDE 上下文与提到的文件包在提问前面（`# Context from my IDE setup:` /
/// `# Files mentioned by the user:`），真正的提问在最后一个 `## My request…:` 之后。
fn request_text(text: &str) -> &str {
    let first = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("");
    if !CONTEXT_WRAPPERS.contains(&first.trim_end_matches(':')) {
        return text;
    }
    let mut offset = 0;
    let mut request = None;
    for line in text.split_inclusive('\n') {
        offset += line.len();
        let heading = line.trim().trim_end_matches(':');
        if REQUEST_HEADINGS.contains(&heading) {
            request = Some(offset);
        }
    }
    request.map_or(text, |start| &text[start..])
}

/// 标题：第一个首行不以 `<` 开头的用户文本块（跳过注入的包装），否则第一个用户文本块；
/// 取真正提问的第一行，不在这里截短与去标记（前端统一做纯文本摘要，避免截断落在 `**` 中间）。
fn session_title(messages: &[ChatMessage]) -> String {
    let user_texts = || {
        messages
            .iter()
            .filter(|message| message.role == "user")
            .flat_map(|message| &message.blocks)
            .filter(|block| block.kind == "text")
            .filter_map(|block| block.text.as_deref())
    };
    let first_line = |text: &str| {
        text.lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or("")
            .starts_with('<')
    };
    let text = user_texts()
        .find(|text| !first_line(text))
        .or_else(|| user_texts().next())
        .unwrap_or("");
    export::shorten_title(request_text(text), TITLE_MAX_CHARS)
}

/// 「只看对话」：与导出 include_tools=false 相同，只保留文本与图片块、丢弃变空的消息。
/// 相邻文本块在简洁版里无法区分，这里同样合并（被截断的文本块之后另起一块），
/// 使结果与直接解析简洁版一致。
pub fn dialogue_only(detail: &ConversationDetail) -> ConversationDetail {
    let messages = detail
        .messages
        .iter()
        .filter_map(|message| {
            let mut blocks: Vec<ContentBlock> = Vec::new();
            for block in &message.blocks {
                match block.kind.as_str() {
                    "text" => match blocks.last_mut() {
                        Some(last) if last.kind == "text" && !last.truncated => {
                            let merged = format!(
                                "{}\n\n{}",
                                last.text.as_deref().unwrap_or(""),
                                block.text.as_deref().unwrap_or("")
                            );
                            last.text = Some(merged.trim().to_string());
                            last.truncated = block.truncated;
                        }
                        _ => blocks.push(block.clone()),
                    },
                    "image" => blocks.push(block.clone()),
                    _ => {}
                }
            }
            (!blocks.is_empty()).then(|| ChatMessage {
                agent: message.agent,
                uuid: message.uuid.clone(),
                role: message.role.clone(),
                timestamp: message.timestamp,
                is_sidechain: message.is_sidechain,
                blocks,
                usage: message.usage.clone(),
            })
        })
        .collect();
    ConversationDetail {
        agent: detail.agent,
        session_id: detail.session_id.clone(),
        project: detail.project.clone(),
        git_branch: detail.git_branch.clone(),
        started_at: detail.started_at,
        ended_at: detail.ended_at,
        cli_version: detail.cli_version.clone(),
        source: detail.source.clone(),
        models: detail.models.clone(),
        messages,
        usage: detail.usage.clone(),
        usage_breakdown: detail.usage_breakdown.clone(),
    }
}

// ----------------------------- 存储 -----------------------------

/// 条目里的一个文件（详细版或简洁版）。
#[derive(Debug, Clone)]
struct Variant {
    /// 导入时的原文件名
    file_name: String,
    /// 读入时的原文，「保留」时原样写盘
    markdown: String,
    parsed: ParsedExport,
}

#[derive(Debug, Clone)]
struct Entry {
    id: String,
    agent: Agent,
    session_id: String,
    kept: bool,
    imported_at: i64,
    full: Option<Variant>,
    brief: Option<Variant>,
}

impl Entry {
    fn slot_mut(&mut self, variant: ExternalVariant) -> &mut Option<Variant> {
        match variant {
            ExternalVariant::Full => &mut self.full,
            ExternalVariant::Brief => &mut self.brief,
        }
    }
}

/// 保留目录里的 entry.json。
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct EntryRecord {
    imported_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    full_file_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    brief_file_name: Option<String>,
}

/// 已读取并解析、尚未放进存储的导入文件：解析在锁外完成。
pub struct PreparedImport {
    id: String,
    variant: ExternalVariant,
    file: Variant,
}

/// 一个文件放进存储后的结果。
#[derive(Debug, Clone)]
pub struct InsertOutcome {
    pub id: String,
    pub variant: ExternalVariant,
    pub replaced: bool,
    pub kept: bool,
}

/// 外部会话存储：内存里的条目，加上「保留」条目在应用数据目录下的副本。
/// 第一次被访问时才从磁盘懒加载已保留的条目。
#[derive(Debug, Default)]
pub struct ExternalStore {
    loaded: bool,
    entries: Vec<Entry>,
    load_errors: Vec<String>,
}

/// 加锁；某次操作 panic 留下的中毒锁照常使用（数据只是内存缓存）。
pub fn lock(store: &Mutex<ExternalStore>) -> MutexGuard<'_, ExternalStore> {
    store
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// 读取待导入的文件：限制大小，要求 UTF-8。
pub fn read_markdown_file(path: &Path) -> Result<String, String> {
    let too_large = || format!("文件超过 {} MB，无法导入。", MAX_IMPORT_BYTES / 1024 / 1024);
    let metadata = fs::metadata(path).map_err(|error| format!("无法读取文件：{error}"))?;
    if !metadata.is_file() {
        return Err("不是文件。".to_string());
    }
    if metadata.len() > MAX_IMPORT_BYTES {
        return Err(too_large());
    }
    let mut bytes = Vec::new();
    File::open(path)
        .and_then(|file| file.take(MAX_IMPORT_BYTES + 1).read_to_end(&mut bytes))
        .map_err(|error| format!("无法读取文件：{error}"))?;
    if bytes.len() as u64 > MAX_IMPORT_BYTES {
        return Err(too_large());
    }
    String::from_utf8(bytes).map_err(|_| "文件不是 UTF-8 文本。".to_string())
}

/// 解析一个文件并决定它所属的条目与槽位（含执行过程 → 详细版，否则简洁版）。
pub fn prepare_import(file_name: &str, markdown: String) -> Result<PreparedImport, String> {
    let normalized = normalize(&markdown);
    let parsed = parse_normalized(&normalized)?;
    let id = entry_id(parsed.detail.agent, &parsed.detail.session_id, &normalized);
    let variant = if parsed.has_process {
        ExternalVariant::Full
    } else {
        ExternalVariant::Brief
    };
    Ok(PreparedImport {
        id,
        variant,
        file: Variant {
            file_name: file_name.to_string(),
            markdown,
            parsed,
        },
    })
}

/// 逐个导入文件：读取与解析在锁外进行，放进存储时才加锁。单个文件的错误记在结果项里。
pub fn import_paths(
    store: &Mutex<ExternalStore>,
    base: &Path,
    paths: &[String],
    now_ms: i64,
) -> ExternalImportResult {
    let items = paths
        .iter()
        .map(|path| {
            let file_name = Path::new(path)
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| path.clone());
            let outcome = read_markdown_file(Path::new(path))
                .and_then(|markdown| prepare_import(&file_name, markdown))
                .and_then(|prepared| lock(store).insert(base, prepared, now_ms));
            match outcome {
                Ok(outcome) => ExternalImportItem {
                    path: path.clone(),
                    file_name,
                    error: None,
                    id: Some(outcome.id),
                    variant: Some(outcome.variant),
                    replaced: outcome.replaced,
                    kept: outcome.kept,
                },
                Err(error) => ExternalImportItem {
                    path: path.clone(),
                    file_name,
                    error: Some(error),
                    id: None,
                    variant: None,
                    replaced: false,
                    kept: false,
                },
            }
        })
        .collect();
    ExternalImportResult { items }
}

/// 条目 ID：会话 ID 是安全字符时为 `{agent}-{sessionId}`，否则用规范化原文的 FNV-1a 哈希。
fn entry_id(agent: Agent, session_id: &str, normalized: &str) -> String {
    if is_safe_token(session_id, MAX_SESSION_ID_CHARS) {
        format!("{}-{session_id}", agent.as_str())
    } else {
        format!("{}-md-{:016x}", agent.as_str(), fnv1a64(normalized))
    }
}

/// 1..=max 个 [A-Za-z0-9_-] 字符：可直接用作目录名与 URL 片段。
fn is_safe_token(text: &str, max: usize) -> bool {
    !text.is_empty()
        && text.len() <= max
        && text
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

fn fnv1a64(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// 校验 ID 后才拼出 `<base>/<id>`，保证不会落到 base 之外。
fn entry_dir(base: &Path, id: &str) -> Result<PathBuf, String> {
    if is_safe_token(id, MAX_ID_CHARS) {
        Ok(base.join(id))
    } else {
        Err(ERR_INVALID_ID.to_string())
    }
}

fn variant_file(variant: ExternalVariant) -> &'static str {
    match variant {
        ExternalVariant::Full => "full.md",
        ExternalVariant::Brief => "brief.md",
    }
}

/// 先写 `*.tmp` 再改名，避免留下半截文件；改名会覆盖同名文件。
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    let temporary = path.with_file_name(format!("{file_name}.tmp"));
    let written = File::create(&temporary)
        .and_then(|mut file| file.write_all(bytes).and_then(|_| file.sync_all()))
        .map_err(|error| format!("写入文件失败：{error}"));
    if let Err(error) = written {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(format!("替换文件失败：{error}"));
    }
    Ok(())
}

fn write_record(dir: &Path, entry: &Entry) -> Result<(), String> {
    let record = EntryRecord {
        imported_at: entry.imported_at,
        full_file_name: entry.full.as_ref().map(|variant| variant.file_name.clone()),
        brief_file_name: entry
            .brief
            .as_ref()
            .map(|variant| variant.file_name.clone()),
    };
    let json = serde_json::to_vec_pretty(&record)
        .map_err(|error| format!("无法序列化 entry.json：{error}"))?;
    write_atomic(&dir.join("entry.json"), &json)
}

/// 写入条目的全部文件（保留）。目录里遗留的、本条目没有的版本文件一并删除：
/// 否则读不回来的旧文件会让下次启动跳过整个条目。
fn write_entry(dir: &Path, entry: &Entry) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|error| format!("创建目录失败：{error}"))?;
    for (variant, slot) in [
        (ExternalVariant::Full, &entry.full),
        (ExternalVariant::Brief, &entry.brief),
    ] {
        let path = dir.join(variant_file(variant));
        match slot {
            Some(file) => write_atomic(&path, file.markdown.as_bytes())?,
            None => match fs::remove_file(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == ErrorKind::NotFound => {}
                Err(error) => return Err(format!("删除旧文件失败：{error}")),
            },
        }
    }
    write_record(dir, entry)
}

/// 已保留条目收到新文件时只写这个槽位和 entry.json。
fn write_slot(dir: &Path, entry: &Entry, variant: ExternalVariant) -> Result<(), String> {
    let slot = match variant {
        ExternalVariant::Full => &entry.full,
        ExternalVariant::Brief => &entry.brief,
    };
    fs::create_dir_all(dir).map_err(|error| format!("创建目录失败：{error}"))?;
    if let Some(file) = slot {
        write_atomic(&dir.join(variant_file(variant)), file.markdown.as_bytes())?;
    }
    write_record(dir, entry)
}

/// 读回一个保留目录；没有任何 Markdown 时返回 None。
fn load_entry(dir: &Path, id: &str) -> Result<Option<Entry>, String> {
    let record: EntryRecord = fs::read_to_string(dir.join("entry.json"))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    let read_variant =
        |variant: ExternalVariant, file_name: Option<String>| -> Result<Option<Variant>, String> {
            let path = dir.join(variant_file(variant));
            if !path.is_file() {
                return Ok(None);
            }
            let markdown = read_markdown_file(&path)?;
            let parsed = parse_markdown(&markdown)?;
            Ok(Some(Variant {
                file_name: file_name.unwrap_or_else(|| variant_file(variant).to_string()),
                markdown,
                parsed,
            }))
        };
    let full = read_variant(ExternalVariant::Full, record.full_file_name)?;
    let brief = read_variant(ExternalVariant::Brief, record.brief_file_name)?;
    let Some(primary) = full.as_ref().or(brief.as_ref()) else {
        return Ok(None);
    };
    Ok(Some(Entry {
        id: id.to_string(),
        agent: primary.parsed.detail.agent,
        session_id: primary.parsed.detail.session_id.clone(),
        kept: true,
        imported_at: record.imported_at,
        full,
        brief,
    }))
}

fn summarize(entry: &Entry) -> ExternalSessionSummary {
    let variants: Vec<&Variant> = [entry.full.as_ref(), entry.brief.as_ref()]
        .into_iter()
        .flatten()
        .collect();
    let detail = variants.first().map(|variant| &variant.parsed.detail);
    let title = variants
        .iter()
        .map(|variant| variant.parsed.title.as_str())
        .find(|title| !title.is_empty())
        .unwrap_or_default()
        .to_string();
    ExternalSessionSummary {
        id: entry.id.clone(),
        agent: entry.agent,
        session_id: entry.session_id.clone(),
        title,
        project: detail.map(|d| d.project.clone()).unwrap_or_default(),
        started_at: detail.map_or(0, |d| d.started_at),
        ended_at: detail.map_or(0, |d| d.ended_at),
        message_count: detail.map_or(0, |d| d.messages.len()),
        kept: entry.kept,
        has_full: entry.full.is_some(),
        has_brief: entry.brief.is_some(),
        imported_at: entry.imported_at,
        full_file_name: entry.full.as_ref().map(|variant| variant.file_name.clone()),
        brief_file_name: entry
            .brief
            .as_ref()
            .map(|variant| variant.file_name.clone()),
        git_branch: detail.and_then(|d| d.git_branch.clone()),
        cli_version: detail.and_then(|d| d.cli_version.clone()),
        tokens: variants
            .iter()
            .find_map(|variant| variant.parsed.meta_tokens),
        cost_usd: variants
            .iter()
            .find_map(|variant| variant.parsed.meta_cost_usd),
    }
}

impl ExternalStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// 第一次访问时读回 `<base>/*/` 下保留的条目；base 不存在不算错误，也不会被创建。
    fn ensure_loaded(&mut self, base: &Path) {
        if self.loaded {
            return;
        }
        self.loaded = true;
        let read = match fs::read_dir(base) {
            Ok(read) => read,
            Err(error) if error.kind() == ErrorKind::NotFound => return,
            Err(error) => {
                self.load_errors
                    .push(format!("无法读取外部会话目录：{error}"));
                return;
            }
        };
        for item in read.flatten() {
            if !item.file_type().is_ok_and(|kind| kind.is_dir()) {
                continue;
            }
            let name = item.file_name().to_string_lossy().to_string();
            if !is_safe_token(&name, MAX_ID_CHARS) {
                continue;
            }
            match load_entry(&item.path(), &name) {
                Ok(Some(entry)) => self.entries.push(entry),
                Ok(None) => {}
                Err(error) => self.load_errors.push(format!("{name}：{error}")),
            }
        }
    }

    fn find(&self, id: &str) -> Result<&Entry, String> {
        self.entries
            .iter()
            .find(|entry| entry.id == id)
            .ok_or_else(|| ERR_NOT_FOUND.to_string())
    }

    /// 放进对应条目的槽位（同槽位替换）；条目已保留时立即写盘，写盘失败则不改内存。
    pub fn insert(
        &mut self,
        base: &Path,
        prepared: PreparedImport,
        now_ms: i64,
    ) -> Result<InsertOutcome, String> {
        self.ensure_loaded(base);
        let PreparedImport { id, variant, file } = prepared;
        let position = match self.entries.iter().position(|entry| entry.id == id) {
            Some(position) => position,
            None => {
                self.entries.push(Entry {
                    id: id.clone(),
                    agent: file.parsed.detail.agent,
                    session_id: file.parsed.detail.session_id.clone(),
                    kept: false,
                    imported_at: now_ms,
                    full: None,
                    brief: None,
                });
                self.entries.len() - 1
            }
        };
        let entry = &mut self.entries[position];
        let previous = entry.slot_mut(variant).replace(file);
        let previous_imported_at = std::mem::replace(&mut entry.imported_at, now_ms);
        if entry.kept {
            let written =
                entry_dir(base, &entry.id).and_then(|dir| write_slot(&dir, entry, variant));
            if let Err(error) = written {
                *entry.slot_mut(variant) = previous;
                entry.imported_at = previous_imported_at;
                return Err(error);
            }
        }
        Ok(InsertOutcome {
            id,
            variant,
            replaced: previous.is_some(),
            kept: entry.kept,
        })
    }

    /// 全部条目，最近导入的在前。
    pub fn list(&mut self, base: &Path) -> ExternalSessionList {
        self.ensure_loaded(base);
        let mut entries: Vec<&Entry> = self.entries.iter().collect();
        entries.sort_by(|a, b| {
            b.imported_at
                .cmp(&a.imported_at)
                .then_with(|| a.id.cmp(&b.id))
        });
        ExternalSessionList {
            sessions: entries.into_iter().map(summarize).collect(),
            load_errors: self.load_errors.clone(),
        }
    }

    /// 完整视图优先详细版（没有时退回简洁版并标为 dialogue）；
    /// 只看对话优先简洁版，没有时把详细版过滤成对话。
    pub fn get(
        &mut self,
        base: &Path,
        id: &str,
        view: Option<ExternalView>,
    ) -> Result<ExternalSessionDetail, String> {
        self.ensure_loaded(base);
        entry_dir(base, id)?;
        let entry = self.find(id)?;
        let (view, detail) = match (
            view.unwrap_or(ExternalView::Full),
            &entry.full,
            &entry.brief,
        ) {
            (ExternalView::Full, Some(full), _) => (ExternalView::Full, full.parsed.detail.clone()),
            (_, _, Some(brief)) => (ExternalView::Dialogue, brief.parsed.detail.clone()),
            (ExternalView::Dialogue, Some(full), None) => {
                (ExternalView::Dialogue, dialogue_only(&full.parsed.detail))
            }
            (_, None, None) => return Err(ERR_NOT_FOUND.to_string()),
        };
        Ok(ExternalSessionDetail {
            summary: summarize(entry),
            view,
            detail,
        })
    }

    /// 保留：把全部原文与 entry.json 写进 `<base>/<id>/`。重复调用会重写同样的文件。
    pub fn keep(&mut self, base: &Path, id: &str) -> Result<ExternalSessionSummary, String> {
        self.ensure_loaded(base);
        let dir = entry_dir(base, id)?;
        let entry = self
            .entries
            .iter_mut()
            .find(|entry| entry.id == id)
            .ok_or_else(|| ERR_NOT_FOUND.to_string())?;
        let existed = dir.exists();
        if let Err(error) = write_entry(&dir, entry) {
            if !existed {
                let _ = fs::remove_dir_all(&dir);
            }
            return Err(error);
        }
        entry.kept = true;
        let summary = summarize(entry);
        // 这个目录先前读不回来的报错已被本次写入取代
        let prefix = format!("{id}：");
        self.load_errors.retain(|error| !error.starts_with(&prefix));
        Ok(summary)
    }

    /// 删除：移出内存；已保留的条目同时删除 `<base>/<id>` 目录。
    pub fn delete(&mut self, base: &Path, id: &str) -> Result<(), String> {
        self.ensure_loaded(base);
        let dir = entry_dir(base, id)?;
        let position = self
            .entries
            .iter()
            .position(|entry| entry.id == id)
            .ok_or_else(|| ERR_NOT_FOUND.to_string())?;
        if self.entries[position].kept {
            match fs::remove_dir_all(&dir) {
                Ok(()) => {}
                Err(error) if error.kind() == ErrorKind::NotFound => {}
                Err(error) => return Err(format!("删除外部会话目录失败：{error}")),
            }
        }
        self.entries.remove(position);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::{build_conversation_markdown, day_start_ms, Lang};
    use crate::models::{AgentFilter, PersistedOutput, SessionUsage, UsageBreakdown};
    use serde_json::json;

    const SID: &str = "11111111-2222-4333-8444-555555555555";
    /// 不在 home 之下，导出时 pretty_path 不会改写
    const PROJECT: &str = "/synthetic/project";

    /// 本地时区 2026-03-04 09:15 起第 n 分钟。
    fn minute(n: i64) -> i64 {
        day_start_ms("2026-03-04").unwrap() + 9 * 3_600_000 + 15 * 60_000 + n * 60_000
    }

    fn block(kind: &str, text: Option<&str>, tool_name: Option<&str>) -> ContentBlock {
        new_block(
            kind,
            text.map(str::to_string),
            tool_name.map(str::to_string),
        )
    }

    fn text(t: &str) -> ContentBlock {
        block("text", Some(t), None)
    }

    fn truncated(mut b: ContentBlock) -> ContentBlock {
        b.truncated = true;
        b
    }

    fn tool_use(name: &str, input: Value) -> ContentBlock {
        let mut b = block("tool_use", None, Some(name));
        b.tool_input = Some(input);
        b
    }

    fn message(
        agent: Agent,
        role: &str,
        timestamp: i64,
        is_sidechain: bool,
        blocks: Vec<ContentBlock>,
    ) -> ChatMessage {
        ChatMessage {
            agent,
            uuid: format!("orig-{timestamp}"),
            role: role.to_string(),
            timestamp,
            is_sidechain,
            blocks,
            usage: None,
        }
    }

    /// 覆盖导出器各种块的合成会话。Claude 的工具结果在用户消息里，Codex 的单独成为助手消息。
    fn sample(agent: Agent) -> ConversationDetail {
        let result_role = match agent {
            Agent::Claude => "user",
            Agent::Codex => "assistant",
        };
        let mut persisted = truncated(block(
            "tool_result",
            Some("完整的输出\n第二行"),
            Some("Bash"),
        ));
        persisted.persisted_output = Some(PersistedOutput {
            path: "-synthetic/sess/tool-results/a.txt".into(),
            size: 10,
            inlined: true,
        });
        let messages = vec![
            message(
                agent,
                "user",
                minute(0),
                false,
                vec![text(
                    "<environment_context>\n  <cwd>/synthetic/project</cwd>\n</environment_context>",
                )],
            ),
            message(
                agent,
                "user",
                minute(1) + 7_000,
                false,
                vec![text(
                    "  请帮我检查这个函数   \n\n## 背景\n\n```rust\nfn main() {\n    println!(\"hi\");\n}\n```\n\n## 🤖 Claude · 不是标题\n## My request:\n修一下",
                )],
            ),
            message(
                agent,
                "assistant",
                minute(2) + 59_000,
                false,
                vec![
                    block("thinking", Some("第一步\n\n第二步：检查\n> 引用的一行"), None),
                    tool_use(
                        "exec_command",
                        json!({
                            "cmd": "ls -la",
                            "options": {
                                "cwd": "/synthetic/project",
                                "flags": ["-a", "-l"],
                                "nested": {"depth": 2, "ok": true, "none": null}
                            }
                        }),
                    ),
                    text("我先看看目录。"),
                ],
            ),
            message(
                agent,
                result_role,
                minute(3),
                false,
                vec![
                    block(
                        "tool_result",
                        Some("total 2\n## 🧑 用户 · 2026-01-01 10:00\n````\n嵌套的围栏\n````\ntail line"),
                        Some("exec_command"),
                    ),
                    persisted,
                ],
            ),
            message(
                agent,
                result_role,
                minute(4),
                false,
                vec![
                    block("tool_result", Some("ok"), None),
                    text("[Request interrupted by user]"),
                ],
            ),
            message(
                agent,
                "assistant",
                minute(5),
                true,
                vec![truncated(text("子代理的回复")), block("image", None, None)],
            ),
            message(
                agent,
                "user",
                minute(6),
                false,
                vec![
                    text("看看附件"),
                    block(
                        "attachment",
                        Some("附件第一行\n\n附件第二行"),
                        Some("/synthetic/uploads/note.md"),
                    ),
                    block("image", Some("截图.png"), None),
                ],
            ),
            message(
                agent,
                "assistant",
                minute(7),
                false,
                vec![
                    truncated(block("thinking", Some("很长的思考"), None)),
                    text("第一段"),
                    text("第二段"),
                    truncated(text("第三段")),
                    text("第四段"),
                ],
            ),
            // 只有工具块：简洁版里整条消失
            message(
                agent,
                "assistant",
                minute(8),
                false,
                vec![tool_use("noop", json!([]))],
            ),
        ];
        ConversationDetail {
            agent,
            session_id: SID.into(),
            project: PROJECT.into(),
            git_branch: Some("feature/x".into()),
            started_at: minute(0),
            ended_at: minute(8) + 30_000,
            cli_version: Some("9.9.9".into()),
            source: Some("cli".into()),
            models: vec!["synthetic-model".into()],
            messages,
            usage: SessionUsage {
                total_tokens_including_cache: 12_345,
                est_cost_usd: 1.5,
                ..Default::default()
            },
            usage_breakdown: UsageBreakdown::default(),
        }
    }

    /// 按导出约定推演解析结果：不含工具时去掉执行过程块；相邻文本块合并（截断块之后另起）；
    /// 正文 trim；工具结果丢掉持久化标记；空消息消失；时间截到分钟。
    fn expected(detail: &ConversationDetail, include_tools: bool) -> Vec<ChatMessage> {
        let mut out = Vec::new();
        for m in &detail.messages {
            let mut blocks: Vec<ContentBlock> = Vec::new();
            for original in &m.blocks {
                if !include_tools && is_process_kind(&original.kind) {
                    continue;
                }
                let mut b = original.clone();
                b.persisted_output = None;
                let trimmed = b.text.as_deref().map(|t| t.trim().to_string());
                match b.kind.as_str() {
                    "text" => {
                        let t = trimmed.unwrap_or_default();
                        if let Some(last) = blocks
                            .last_mut()
                            .filter(|last| last.kind == "text" && !last.truncated)
                        {
                            last.text = Some(format!("{}\n\n{t}", last.text.as_deref().unwrap()));
                            last.truncated = b.truncated;
                            continue;
                        }
                        b.text = Some(t);
                    }
                    "thinking" | "attachment" => b.text = trimmed,
                    // 工具名写在结果标题里，原样还原
                    "tool_result" => b.text = trimmed,
                    _ => {}
                }
                blocks.push(b);
            }
            if !blocks.is_empty() {
                out.push(ChatMessage {
                    timestamp: m.timestamp - m.timestamp.rem_euclid(60_000),
                    blocks,
                    ..m.clone()
                });
            }
        }
        out
    }

    fn assert_same_messages(actual: &[ChatMessage], expected: &[ChatMessage], context: &str) {
        assert_eq!(actual.len(), expected.len(), "{context}: message count");
        for (index, (a, e)) in actual.iter().zip(expected).enumerate() {
            let at = format!("{context} #{index}");
            assert_eq!(a.role, e.role, "{at} role");
            assert_eq!(a.is_sidechain, e.is_sidechain, "{at} sidechain");
            assert_eq!(a.timestamp, e.timestamp, "{at} timestamp");
            assert_eq!(a.blocks.len(), e.blocks.len(), "{at} block count");
            for (j, (x, y)) in a.blocks.iter().zip(&e.blocks).enumerate() {
                assert_eq!(x.kind, y.kind, "{at} block {j} kind");
                assert_eq!(x.text, y.text, "{at} block {j} text");
                assert_eq!(x.tool_name, y.tool_name, "{at} block {j} tool name");
                assert_eq!(x.tool_input, y.tool_input, "{at} block {j} tool input");
                assert_eq!(x.truncated, y.truncated, "{at} block {j} truncated");
            }
        }
    }

    #[test]
    fn round_trips_detailed_and_brief_exports_in_both_languages() {
        for agent in [Agent::Claude, Agent::Codex] {
            let original = sample(agent);
            for lang in [Lang::Zh, Lang::En] {
                for include_tools in [true, false] {
                    let context = format!("{agent:?} {lang:?} tools={include_tools}");
                    let markdown = build_conversation_markdown(&original, include_tools, lang);
                    let parsed = parse_markdown(&markdown).unwrap();
                    let detail = &parsed.detail;
                    assert_eq!(detail.agent, agent, "{context}");
                    assert_eq!(detail.session_id, SID, "{context}");
                    assert_eq!(detail.project, PROJECT, "{context}");
                    assert_eq!(detail.git_branch.as_deref(), Some("feature/x"));
                    assert_eq!(detail.cli_version.as_deref(), Some("9.9.9"));
                    assert_eq!(detail.started_at, minute(0), "{context}");
                    assert_eq!(detail.ended_at, minute(8), "{context}");
                    assert!(detail.source.is_none() && detail.models.is_empty());
                    assert_eq!(parsed.meta_messages, Some(original.messages.len()));
                    assert_eq!(parsed.meta_tokens, Some(12_345));
                    assert_eq!(parsed.meta_cost_usd, Some(1.5));
                    assert_eq!(parsed.has_process, include_tools, "{context}");
                    assert_eq!(parsed.title, "请帮我检查这个函数", "{context}");
                    for (index, m) in detail.messages.iter().enumerate() {
                        assert_eq!(m.agent, agent);
                        assert_eq!(m.uuid, format!("ext-{index}"));
                        assert!(m.usage.is_none());
                    }
                    assert_same_messages(
                        &detail.messages,
                        &expected(&original, include_tools),
                        &context,
                    );
                }
            }
        }
    }

    #[test]
    fn dialogue_only_of_detailed_matches_brief() {
        for agent in [Agent::Claude, Agent::Codex] {
            let original = sample(agent);
            for lang in [Lang::Zh, Lang::En] {
                let context = format!("{agent:?} {lang:?}");
                let detailed =
                    parse_markdown(&build_conversation_markdown(&original, true, lang)).unwrap();
                let brief =
                    parse_markdown(&build_conversation_markdown(&original, false, lang)).unwrap();
                let filtered = dialogue_only(&detailed.detail);
                assert!(filtered.messages.len() < detailed.detail.messages.len());
                assert_eq!(filtered.session_id, brief.detail.session_id);
                assert_eq!(filtered.started_at, brief.detail.started_at);
                assert_eq!(filtered.ended_at, brief.detail.ended_at);
                assert_same_messages(&filtered.messages, &brief.detail.messages, &context);
                assert_eq!(detailed.title, brief.title);
            }
        }
    }

    #[test]
    fn rejects_other_exports_and_random_markdown() {
        let detail = sample(Agent::Claude);
        let items = vec![export::SessionExportItem {
            detail: detail.clone(),
            title: String::new(),
        }];
        for lang in [Lang::Zh, Lang::En] {
            let params = export::SessionsExportParams {
                project: PROJECT,
                include_tools: true,
                lang,
            };
            let merged = export::build_sessions_merged(&items, &params);
            assert_eq!(parse_markdown(&merged).unwrap_err(), ERR_BATCH_EXPORT);
            let folder = export::build_sessions_folder(&items, &params);
            assert_eq!(
                parse_markdown(&folder.index_markdown).unwrap_err(),
                ERR_BATCH_EXPORT
            );
            // 多文件模式里每个会话文件就是单会话导出
            assert!(parse_markdown(&folder.files[0].markdown).is_ok());

            let prompts = export::build(
                &[],
                &export::ExportParams {
                    start_ms: 0,
                    end_ms: 1,
                    project: None,
                    include_commands: false,
                    group_by: "project",
                    start_date: "2026-01-01",
                    end_date: "2026-01-01",
                    lang,
                    agent_filter: AgentFilter::All,
                },
            );
            assert_eq!(
                parse_markdown(&prompts.markdown).unwrap_err(),
                ERR_PROMPT_EXPORT
            );
            let search = export::build_search_export(&[], "query", None, lang);
            assert_eq!(
                parse_markdown(&search.markdown).unwrap_err(),
                ERR_PROMPT_EXPORT
            );
        }
        assert_eq!(
            parse_markdown("# 笔记\n\n随便写点东西\n").unwrap_err(),
            ERR_NOT_EXPORT
        );
        assert_eq!(parse_markdown("").unwrap_err(), ERR_NOT_EXPORT);
        assert_eq!(parse_markdown("\u{feff}\n \n").unwrap_err(), ERR_NOT_EXPORT);
        // 没有产品行也没有助手消息：无法判断产品
        assert_eq!(
            parse_markdown("# 对话导出\n\n## 🧑 用户 · 2026-01-01 10:00\n\n你好\n").unwrap_err(),
            ERR_UNKNOWN_AGENT
        );
    }

    #[test]
    fn crlf_and_bom_input_parses_identically() {
        let markdown = build_conversation_markdown(&sample(Agent::Codex), true, Lang::Zh);
        let lf = parse_markdown(&markdown).unwrap();
        let crlf = parse_markdown(&format!("\u{feff}{}", markdown.replace('\n', "\r\n"))).unwrap();
        assert_eq!(crlf.detail.agent, lf.detail.agent);
        assert_eq!(crlf.detail.session_id, lf.detail.session_id);
        assert_eq!(crlf.detail.project, lf.detail.project);
        assert_eq!(crlf.detail.started_at, lf.detail.started_at);
        assert_eq!(crlf.title, lf.title);
        assert_eq!(crlf.has_process, lf.has_process);
        assert_same_messages(&crlf.detail.messages, &lf.detail.messages, "crlf");
    }

    #[test]
    fn lenient_input_and_inferred_fields() {
        // 普通空格分隔、未知标签、无时间的标题（行尾空格被去掉）、`>` 空引用行、
        // 持久化输出来源行、正文里的伪标记与伪标题
        let markdown = [
            "# Conversation Export",
            "",
            "> **Session ID** `weird id/with spaces`",
            "> **Unknown label**　whatever",
            "> **Time**　2026-03-04 09:15 ~ ",
            "",
            "---",
            "",
            "## 🧑 User ·",
            "",
            "**💭 Thinking**",
            "not a quote, so this marker is plain text",
            "",
            "## Not a message heading",
            "",
            "## 🤖 Codex · 2026-03-04 09:20 (subagent)",
            "",
            "**💭 Thinking**",
            "",
            "> first",
            ">",
            "> third",
            "",
            "**↩ Tool result**",
            "",
            "````",
            "output",
            "````",
            "",
            "_Full output from `-p/a.txt`_",
            "",
            "after",
            "",
        ]
        .join("\n");
        let parsed = parse_markdown(&markdown).unwrap();
        let detail = &parsed.detail;
        // 产品从助手消息标题推断
        assert_eq!(detail.agent, Agent::Codex);
        assert_eq!(detail.session_id, "weird id/with spaces");
        assert_eq!(detail.started_at, minute(0));
        // 结束时间为空时退回最后一条消息
        assert_eq!(detail.ended_at, minute(5));
        assert_eq!(detail.messages.len(), 2);

        let first = &detail.messages[0];
        assert_eq!(first.role, "user");
        assert_eq!(first.timestamp, minute(0), "无时间时用头部开始时间");
        assert_eq!(first.blocks.len(), 1);
        assert_eq!(
            first.blocks[0].text.as_deref(),
            Some("**💭 Thinking**\nnot a quote, so this marker is plain text\n\n## Not a message heading")
        );

        let second = &detail.messages[1];
        assert_eq!(second.role, "assistant");
        assert!(second.is_sidechain);
        let kinds: Vec<&str> = second.blocks.iter().map(|b| b.kind.as_str()).collect();
        assert_eq!(kinds, ["thinking", "tool_result", "text"]);
        assert_eq!(second.blocks[0].text.as_deref(), Some("first\n\nthird"));
        assert_eq!(second.blocks[1].text.as_deref(), Some("output"));
        assert_eq!(second.blocks[2].text.as_deref(), Some("after"));
        assert!(parsed.has_process);
        assert_eq!(parsed.title, "**💭 Thinking**");

        // 不安全的会话 ID 用内容哈希作条目 ID，且稳定
        let a = prepare_import("a.md", markdown.clone()).unwrap();
        let b = prepare_import("b.md", markdown.replace('\n', "\r\n")).unwrap();
        assert!(a.id.starts_with("codex-md-"), "{}", a.id);
        assert_eq!(a.id, b.id);
        assert!(is_safe_token(&a.id, MAX_ID_CHARS));
    }

    // ---------- 存储 ----------

    fn temp_root(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("cc-history-external-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_file(dir: &Path, name: &str, bytes: &[u8]) -> String {
        let path = dir.join(name);
        fs::write(&path, bytes).unwrap();
        path.to_string_lossy().to_string()
    }

    #[test]
    fn store_merges_variants_keeps_reloads_and_deletes() {
        let root = temp_root("store");
        let base = root.join("external_sessions");
        let original = sample(Agent::Claude);
        let detailed_md = build_conversation_markdown(&original, true, Lang::Zh);
        let brief_md = build_conversation_markdown(&original, false, Lang::En);
        let detailed = write_file(&root, "detailed.md", detailed_md.as_bytes());
        let brief = write_file(&root, "brief.md", brief_md.as_bytes());
        let id = format!("claude-{SID}");

        let store = Mutex::new(ExternalStore::new());
        let result = import_paths(&store, &base, &[detailed.clone(), brief.clone()], 1_000);
        let variants: Vec<_> = result.items.iter().map(|item| item.variant).collect();
        assert_eq!(
            variants,
            [Some(ExternalVariant::Full), Some(ExternalVariant::Brief)]
        );
        assert!(result.items.iter().all(|item| item.error.is_none()
            && item.id.as_deref() == Some(id.as_str())
            && !item.replaced
            && !item.kept));
        assert_eq!(result.items[0].file_name, "detailed.md");
        assert!(!base.exists(), "临时导入不写盘");

        let list = lock(&store).list(&base);
        assert_eq!(list.sessions.len(), 1);
        let summary = &list.sessions[0];
        assert!(summary.has_full && summary.has_brief && !summary.kept);
        assert_eq!(summary.title, "请帮我检查这个函数");
        assert_eq!(summary.project, PROJECT);
        assert_eq!(summary.tokens, Some(12_345));
        assert_eq!(summary.cost_usd, Some(1.5));
        assert_eq!(summary.full_file_name.as_deref(), Some("detailed.md"));
        assert_eq!(summary.brief_file_name.as_deref(), Some("brief.md"));
        assert_eq!(
            summary.message_count,
            parse_markdown(&detailed_md).unwrap().detail.messages.len()
        );

        // 同槽位再次导入：替换并刷新导入时间；坏文件逐个报错
        let bad = write_file(
            &root,
            "index.md",
            b"# \xe4\xbc\x9a\xe8\xaf\x9d\xe5\xaf\xbc\xe5\x87\xba\n",
        );
        let binary = write_file(&root, "binary.md", &[0xff, 0xfe, 0x00]);
        let result = import_paths(&store, &base, &[detailed.clone(), bad, binary], 2_000);
        assert!(result.items[0].replaced);
        assert_eq!(result.items[1].error.as_deref(), Some(ERR_BATCH_EXPORT));
        assert_eq!(
            result.items[2].error.as_deref(),
            Some("文件不是 UTF-8 文本。")
        );
        assert_eq!(lock(&store).list(&base).sessions[0].imported_at, 2_000);

        // 视图：完整用详细版；只看对话优先简洁版
        let full = lock(&store).get(&base, &id, None).unwrap();
        assert_eq!(full.view, ExternalView::Full);
        assert!(full
            .detail
            .messages
            .iter()
            .any(|m| m.blocks.iter().any(|b| b.kind == "tool_use")));
        let dialogue = lock(&store)
            .get(&base, &id, Some(ExternalView::Dialogue))
            .unwrap();
        assert_eq!(dialogue.view, ExternalView::Dialogue);
        assert_same_messages(
            &dialogue.detail.messages,
            &parse_markdown(&brief_md).unwrap().detail.messages,
            "dialogue prefers brief",
        );
        assert_eq!(
            lock(&store).get(&base, "claude-missing", None).unwrap_err(),
            ERR_NOT_FOUND
        );

        // 保留：写入原文与 entry.json
        let kept = lock(&store).keep(&base, &id).unwrap();
        assert!(kept.kept);
        let dir = base.join(&id);
        assert_eq!(
            fs::read_to_string(dir.join("full.md")).unwrap(),
            detailed_md
        );
        assert_eq!(fs::read_to_string(dir.join("brief.md")).unwrap(), brief_md);
        let record: Value =
            serde_json::from_str(&fs::read_to_string(dir.join("entry.json")).unwrap()).unwrap();
        assert_eq!(record["importedAt"], 2_000);
        assert_eq!(record["fullFileName"], "detailed.md");
        assert_eq!(record["briefFileName"], "brief.md");

        // 已保留条目收到新文件时立即写盘
        let brief_zh_md = build_conversation_markdown(&original, false, Lang::Zh);
        let brief_zh = write_file(&root, "brief-zh.md", brief_zh_md.as_bytes());
        let result = import_paths(&store, &base, &[brief_zh], 3_000);
        assert!(result.items[0].replaced && result.items[0].kept);
        assert_eq!(
            fs::read_to_string(dir.join("brief.md")).unwrap(),
            brief_zh_md
        );

        // 无效目录与坏文件：跳过并报告，不影响其他条目
        fs::create_dir_all(base.join("not valid")).unwrap();
        fs::create_dir_all(base.join("claude-broken")).unwrap();
        fs::write(base.join("claude-broken").join("full.md"), "# 随便\n").unwrap();
        fs::create_dir_all(base.join("claude-empty")).unwrap();

        let fresh = Mutex::new(ExternalStore::new());
        let list = lock(&fresh).list(&base);
        assert_eq!(list.sessions.len(), 1);
        let reloaded = &list.sessions[0];
        assert_eq!(reloaded.id, id);
        assert!(reloaded.kept && reloaded.has_full && reloaded.has_brief);
        assert_eq!(reloaded.imported_at, 3_000);
        assert_eq!(reloaded.brief_file_name.as_deref(), Some("brief-zh.md"));
        assert_eq!(list.load_errors.len(), 1);
        assert!(list.load_errors[0].starts_with("claude-broken"));

        // 无效 ID 一律拒绝，不触碰文件系统
        for bad_id in ["..", "../x", "a/b", "", "claude-a b"] {
            assert_eq!(
                lock(&fresh).keep(&base, bad_id).unwrap_err(),
                ERR_INVALID_ID
            );
            assert_eq!(
                lock(&fresh).delete(&base, bad_id).unwrap_err(),
                ERR_INVALID_ID
            );
            assert_eq!(
                lock(&fresh).get(&base, bad_id, None).unwrap_err(),
                ERR_INVALID_ID
            );
        }

        // 删除已保留条目：目录随之删除，其他目录不受影响
        lock(&fresh).delete(&base, &id).unwrap();
        assert!(!dir.exists());
        assert!(base.join("claude-broken").exists());
        assert!(lock(&fresh).list(&base).sessions.is_empty());
        assert_eq!(lock(&fresh).delete(&base, &id).unwrap_err(), ERR_NOT_FOUND);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn bare_cr_and_empty_thinking_round_trip() {
        // 思考块一行里的单独 CR 不是换行；空的思考块后面同一条消息里紧跟文本
        let mut original = sample(Agent::Claude);
        original.messages = vec![
            message(Agent::Claude, "user", minute(0), false, vec![text("开始")]),
            message(
                Agent::Claude,
                "assistant",
                minute(1),
                false,
                vec![
                    block("thinking", Some("step 10%\rstep 20%"), None),
                    text("answer"),
                ],
            ),
            message(
                Agent::Claude,
                "assistant",
                minute(2),
                false,
                vec![block("thinking", Some(""), None), text("after empty")],
            ),
        ];
        for lang in [Lang::Zh, Lang::En] {
            let detailed = build_conversation_markdown(&original, true, lang);
            let parsed = parse_markdown(&detailed).unwrap();
            assert!(parsed.has_process);
            assert_same_messages(
                &parsed.detail.messages,
                &expected(&original, true),
                "bare cr / empty thinking",
            );
            let brief =
                parse_markdown(&build_conversation_markdown(&original, false, lang)).unwrap();
            assert_same_messages(
                &dialogue_only(&parsed.detail).messages,
                &brief.detail.messages,
                "dialogue_only",
            );
        }

        // 整个文件只有 CR 换行（老式 Mac）时仍按换行解析
        let plain = sample(Agent::Codex);
        let markdown = build_conversation_markdown(&plain, true, Lang::Zh);
        let lf = parse_markdown(&markdown).unwrap();
        let cr = parse_markdown(&markdown.replace('\n', "\r")).unwrap();
        assert_same_messages(&cr.detail.messages, &lf.detail.messages, "cr only");
    }

    #[test]
    fn tool_results_take_the_name_of_their_call_and_titles_skip_context_wrappers() {
        let mut original = sample(Agent::Claude);
        let result = |text: &str| block("tool_result", Some(text), None);
        original.messages = vec![
            message(
                Agent::Claude,
                "user",
                minute(0),
                false,
                vec![text(
                    "# Files mentioned by the user:\n\n## 报表.xlsx: C:/data/报表.xlsx\n\n## My request:\n**核对**报表里的金额",
                )],
            ),
            message(Agent::Claude, "assistant", minute(1), false, vec![tool_use("Task", json!({"prompt": "调研"}))]),
            message(Agent::Claude, "assistant", minute(1), false, vec![tool_use("Bash", json!({"command": "ls"}))]),
            message(Agent::Claude, "user", minute(2), false, vec![result("## 结论")]),
            message(Agent::Claude, "user", minute(2), false, vec![result("a.txt")]),
            // 调用之后先出现文本：这一轮已结束，后面的结果不再错配
            message(Agent::Claude, "assistant", minute(3), false, vec![tool_use("Read", json!({"file_path": "/a"}))]),
            message(Agent::Claude, "assistant", minute(3), false, vec![text("被打断了")]),
            message(Agent::Claude, "user", minute(4), false, vec![result("迟到的结果")]),
        ];
        let parsed =
            parse_markdown(&build_conversation_markdown(&original, true, Lang::Zh)).unwrap();
        let names: Vec<Option<&str>> = parsed
            .detail
            .messages
            .iter()
            .flat_map(|m| &m.blocks)
            .filter(|b| b.kind == "tool_result")
            .map(|b| b.tool_name.as_deref())
            .collect();
        assert_eq!(names, [Some("Task"), Some("Bash"), None]);
        assert_eq!(parsed.title, "**核对**报表里的金额");

        original.messages[0].blocks = vec![text(
            "# Context from my IDE setup:\n\n## Active file: src/a.rs\n\n## My request for Codex:\n修一下侧栏",
        )];
        let parsed =
            parse_markdown(&build_conversation_markdown(&original, false, Lang::En)).unwrap();
        assert_eq!(parsed.title, "修一下侧栏");
    }

    #[test]
    fn result_markers_carry_names_and_old_files_pair_per_chain() {
        let result = |text: &str, name: Option<&str>| block("tool_result", Some(text), name);
        let names = |markdown: &str| -> Vec<Option<String>> {
            parse_markdown(markdown)
                .unwrap()
                .detail
                .messages
                .iter()
                .flat_map(|m| &m.blocks)
                .filter(|b| b.kind == "tool_result")
                .map(|b| b.tool_name.clone())
                .collect()
        };
        let named = |list: &[&str]| -> Vec<Option<String>> {
            list.iter().map(|name| Some(name.to_string())).collect()
        };

        // 新版导出：结果标题带工具名，原样还原
        let mut codex = sample(Agent::Codex);
        let call = |time: i64, name: &str| {
            message(
                Agent::Codex,
                "assistant",
                minute(time),
                false,
                vec![tool_use(name, json!({}))],
            )
        };
        codex.messages = vec![
            message(Agent::Codex, "user", minute(0), false, vec![text("开始")]),
            call(1, "web_search"),
            call(1, "shell"),
            message(
                Agent::Codex,
                "assistant",
                minute(2),
                false,
                vec![result("a.txt", Some("shell"))],
            ),
            call(2, "spawn_agent"),
            message(
                Agent::Codex,
                "assistant",
                minute(3),
                false,
                vec![result("## 报告", Some("spawn_agent"))],
            ),
        ];
        let detailed = build_conversation_markdown(&codex, true, Lang::Zh);
        assert!(detailed.contains("**↩ 工具结果 · shell**"));
        assert_eq!(names(&detailed), named(&["shell", "spawn_agent"]));

        // 旧版导出没有工具名：web_search 从不产生结果，不入队，后面不会错位
        let old = detailed
            .replace("**↩ 工具结果 · shell**", "**↩ 工具结果**")
            .replace("**↩ 工具结果 · spawn_agent**", "**↩ 工具结果**");
        assert_eq!(names(&old), named(&["shell", "spawn_agent"]));

        // 旧版 Claude 会话把子代理消息穿插在主线里：两条线各自配对
        let mut claude = sample(Agent::Claude);
        claude.messages = vec![
            message(
                Agent::Claude,
                "assistant",
                minute(0),
                false,
                vec![tool_use("Task", json!({}))],
            ),
            message(Agent::Claude, "user", minute(1), true, vec![text("调研")]),
            message(
                Agent::Claude,
                "assistant",
                minute(1),
                true,
                vec![tool_use("Bash", json!({}))],
            ),
            message(
                Agent::Claude,
                "user",
                minute(1),
                true,
                vec![result("a.txt", None)],
            ),
            message(
                Agent::Claude,
                "assistant",
                minute(2),
                true,
                vec![text("找到了")],
            ),
            message(
                Agent::Claude,
                "user",
                minute(3),
                false,
                vec![result("## 报告", None)],
            ),
        ];
        let markdown = build_conversation_markdown(&claude, true, Lang::En);
        assert_eq!(names(&markdown), named(&["Bash", "Task"]));
    }

    #[test]
    fn keeping_over_a_broken_directory_replaces_it() {
        let root = temp_root("broken");
        let base = root.join("external_sessions");
        let id = format!("claude-{SID}");
        fs::create_dir_all(base.join(&id)).unwrap();
        fs::write(base.join(&id).join("full.md"), "# junk\n").unwrap();

        let store = Mutex::new(ExternalStore::new());
        let list = lock(&store).list(&base);
        assert!(list.sessions.is_empty());
        assert_eq!(list.load_errors.len(), 1);
        assert!(list.load_errors[0].starts_with(&format!("{id}：")));

        let brief_md = build_conversation_markdown(&sample(Agent::Claude), false, Lang::Zh);
        let brief = write_file(&root, "brief.md", brief_md.as_bytes());
        let result = import_paths(&store, &base, &[brief], 2_000);
        assert!(result.items[0].error.is_none());
        let summary = lock(&store).keep(&base, &id).unwrap();
        assert!(summary.kept && !summary.has_full && summary.has_brief);
        assert!(
            !base.join(&id).join("full.md").exists(),
            "遗留的坏文件被删除"
        );
        assert!(lock(&store).list(&base).load_errors.is_empty());

        let reloaded = Mutex::new(ExternalStore::new());
        let list = lock(&reloaded).list(&base);
        assert!(list.load_errors.is_empty());
        assert_eq!(list.sessions.len(), 1);
        assert!(list.sessions[0].kept && list.sessions[0].has_brief);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn store_views_fall_back_between_variants() {
        let root = temp_root("views");
        let base = root.join("external_sessions");
        let codex = sample(Agent::Codex);
        let full_only = write_file(
            &root,
            "full.md",
            build_conversation_markdown(&codex, true, Lang::En).as_bytes(),
        );
        let mut claude = sample(Agent::Claude);
        claude.session_id = "brief-only".into();
        let brief_only = write_file(
            &root,
            "brief.md",
            build_conversation_markdown(&claude, false, Lang::Zh).as_bytes(),
        );
        let store = Mutex::new(ExternalStore::new());
        let result = import_paths(&store, &base, &[full_only, brief_only], 5_000);
        assert!(result.items.iter().all(|item| item.error.is_none()));

        // 只有详细版：只看对话由详细版过滤得到
        let codex_id = format!("codex-{SID}");
        let parsed_full = lock(&store).get(&base, &codex_id, None).unwrap();
        let dialogue = lock(&store)
            .get(&base, &codex_id, Some(ExternalView::Dialogue))
            .unwrap();
        assert_eq!(dialogue.view, ExternalView::Dialogue);
        assert_same_messages(
            &dialogue.detail.messages,
            &dialogue_only(&parsed_full.detail).messages,
            "filtered full",
        );
        assert!(!dialogue.summary.has_brief);

        // 只有简洁版：完整视图退回简洁版并标为 dialogue
        let brief = lock(&store)
            .get(&base, "claude-brief-only", Some(ExternalView::Full))
            .unwrap();
        assert_eq!(brief.view, ExternalView::Dialogue);
        assert!(!brief.summary.has_full && brief.summary.has_brief);

        // 临时条目删除只动内存
        lock(&store).delete(&base, "claude-brief-only").unwrap();
        assert_eq!(lock(&store).list(&base).sessions.len(), 1);
        assert!(!base.exists());
        let _ = fs::remove_dir_all(&root);
    }

    /// 用真实导出文件核对解析结果，只打印计数，不输出任何内容。路径来自环境变量：
    /// `EXT_MD_DETAILED=… EXT_MD_BRIEF=… cargo test -- --ignored --nocapture real_export_pair`
    #[test]
    #[ignore = "reads real exports named by EXT_MD_DETAILED / EXT_MD_BRIEF; prints counts only"]
    fn real_export_pair_counts() {
        let (Ok(detailed_path), Ok(brief_path)) = (
            std::env::var("EXT_MD_DETAILED"),
            std::env::var("EXT_MD_BRIEF"),
        ) else {
            println!("EXT_MD_DETAILED / EXT_MD_BRIEF not set; skipped");
            return;
        };
        let load = |path: &str| parse_markdown(&read_markdown_file(Path::new(path)).unwrap());
        let detailed = load(&detailed_path).unwrap();
        let brief = load(&brief_path).unwrap();
        for (name, parsed) in [("detailed", &detailed), ("brief", &brief)] {
            let mut kinds = std::collections::BTreeMap::<&str, usize>::new();
            for b in parsed.detail.messages.iter().flat_map(|m| &m.blocks) {
                *kinds.entry(b.kind.as_str()).or_default() += 1;
            }
            println!(
                "{name}: agent={:?} messages={} meta_messages={:?} has_process={} session_id_len={} title_chars={} blocks={kinds:?}",
                parsed.detail.agent,
                parsed.detail.messages.len(),
                parsed.meta_messages,
                parsed.has_process,
                parsed.detail.session_id.len(),
                parsed.title.chars().count(),
            );
        }
        println!(
            "same session id: {}",
            detailed.detail.session_id == brief.detail.session_id
        );
        let filtered = dialogue_only(&detailed.detail);
        let mut mismatches = 0;
        let lengths = |m: &ChatMessage| -> Vec<(String, usize)> {
            m.blocks
                .iter()
                .map(|b| {
                    let chars = b.text.as_deref().map_or(0, |t| t.chars().count());
                    (b.kind.clone(), chars)
                })
                .collect()
        };
        for index in 0..filtered.messages.len().max(brief.detail.messages.len()) {
            let a = filtered.messages.get(index);
            let b = brief.detail.messages.get(index);
            let same = match (a, b) {
                (Some(a), Some(b)) => {
                    a.role == b.role
                        && a.is_sidechain == b.is_sidechain
                        && a.timestamp == b.timestamp
                        && a.blocks.len() == b.blocks.len()
                        && a.blocks.iter().zip(&b.blocks).all(|(x, y)| {
                            x.kind == y.kind && x.text == y.text && x.truncated == y.truncated
                        })
                }
                _ => false,
            };
            if !same {
                mismatches += 1;
                if mismatches <= 10 {
                    println!(
                        "mismatch #{index}: dialogue_only={:?} brief={:?}",
                        a.map(|m| (m.role.as_str(), lengths(m))),
                        b.map(|m| (m.role.as_str(), lengths(m))),
                    );
                }
            }
        }
        println!(
            "dialogue_only(detailed) messages={} brief messages={} mismatches={mismatches} equal={}",
            filtered.messages.len(),
            brief.detail.messages.len(),
            mismatches == 0
        );
    }
}
