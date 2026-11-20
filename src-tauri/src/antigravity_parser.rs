//! Antigravity CLI（agy）会话：`antigravity-cli/brain/<会话 id>/.system_generated/logs/` 下的
//! 转录 JSONL，以及记录每次输入所在工作区的 `antigravity-cli/history.jsonl`。
//!
//! 转录每行是一步（step）：`USER_INPUT` 是用户输入，`PLANNER_RESPONSE` 是一次模型调用（回复、
//! 思考、工具调用与 Token），`GENERIC` 是紧随其后的工具结果，`SYSTEM_MESSAGE` 是系统提示或
//! 子代理发来的消息。转录不记工作目录，工作区由索引按 history.jsonl 补上。

use crate::codex_parser::nonempty_string;
use crate::models::{
    Agent, ChatMessage, ContentBlock, ConversationDetail, MessageUsage, NormalizedUsage,
    SessionUsage, UsageBreakdown,
};
use crate::parser::{
    self, clip_to, BlockLimit, ConvFileResult, RawPrompt, UsageEntry, DISPLAY_BLOCK_LIMIT,
};
use serde_json::Value;
use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};

/// `SessionSummary.source` of every Antigravity CLI session.
pub const SOURCE: &str = "antigravity-cli";
/// Keeps tool arguments typed; preferred when present.
const FULL_TRANSCRIPT: &str = "transcript_full.jsonl";
/// Same steps, but every tool argument value is JSON-encoded into a string.
const PLAIN_TRANSCRIPT: &str = "transcript.jsonl";
/// Tool name shown on a message a sub-agent sent back to its parent.
const SUBAGENT_MESSAGE_TOOL: &str = "subagent";

/// The transcript to read for one conversation directory under `brain/`.
pub fn transcript_for(conversation_dir: &Path) -> Option<PathBuf> {
    let logs = conversation_dir.join(".system_generated").join("logs");
    [FULL_TRANSCRIPT, PLAIN_TRANSCRIPT]
        .iter()
        .map(|name| logs.join(name))
        .find(|path| path.is_file())
}

/// True for `<id>/.system_generated/logs/transcript[_full].jsonl`.
pub fn is_transcript(path: &Path) -> bool {
    let name = path.file_name().and_then(|name| name.to_str());
    matches!(name, Some(FULL_TRANSCRIPT | PLAIN_TRANSCRIPT))
        && path
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|name| name == "logs")
}

/// `brain/<id>/.system_generated/logs/<file>` → `<id>`.
fn conversation_id(path: &Path) -> Option<String> {
    path.parent()?
        .parent()?
        .parent()?
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .filter(|name| !name.is_empty())
}

// ----------------------------- history.jsonl -----------------------------

/// Parse `antigravity-cli/history.jsonl`: one line per input with its workspace. The first input
/// of a conversation is written before the conversation has an ID, so it may lack one.
pub fn parse_history(path: &Path) -> Vec<RawPrompt> {
    let mut prompts = Vec::new();
    let _ = parser::for_each_jsonl_line(path, |_, line| {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            return;
        };
        let Some(text) = nonempty_string(value.get("display")) else {
            return;
        };
        let Some(timestamp) = value.get("timestamp").and_then(Value::as_i64) else {
            return;
        };
        prompts.push(RawPrompt {
            agent: Agent::Antigravity,
            text: text.trim().to_string(),
            project: nonempty_string(value.get("workspace"))
                .map(|workspace| workspace_path(&workspace))
                .unwrap_or_default(),
            timestamp,
            session_id: nonempty_string(value.get("conversationId")),
            git_branch: None,
            pasted_count: 0,
            from_history: true,
        });
    });
    prompts
}

/// Workspaces are plain paths in history.jsonl but `file://` URIs elsewhere.
pub(crate) fn workspace_path(raw: &str) -> String {
    let Some(rest) = raw.strip_prefix("file://") else {
        return raw.trim_end_matches('/').to_string();
    };
    let decoded = percent_decode(rest);
    // file:///C:/work → C:/work
    let bytes = decoded.as_bytes();
    let path = if bytes.len() > 2 && bytes[0] == b'/' && bytes[2] == b':' {
        &decoded[1..]
    } else {
        decoded.as_str()
    };
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        "/".to_string()
    } else {
        trimmed.to_string()
    }
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = &text[index + 1..index + 3];
            if let Ok(byte) = u8::from_str_radix(hex, 16) {
                out.push(byte);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

// ----------------------------- Transcript steps -----------------------------

struct Step {
    index: i64,
    kind: String,
    timestamp: Option<i64>,
    content: Option<String>,
    thinking: Option<String>,
    tool_calls: Vec<(String, Value)>,
    model: Option<String>,
    usage: Option<NormalizedUsage>,
}

impl Step {
    fn parse(line_no: usize, line: &str, decode_args: bool) -> Option<Self> {
        let value: Value = serde_json::from_str(line).ok()?;
        let index = value
            .get("step_index")
            .and_then(Value::as_i64)
            .unwrap_or(line_no as i64);
        let kind = nonempty_string(value.get("type"))?;
        let tool_calls = value
            .get("tool_calls")
            .and_then(Value::as_array)
            .map(|calls| {
                calls
                    .iter()
                    .map(|call| {
                        let name =
                            nonempty_string(call.get("name")).unwrap_or_else(|| "tool".to_string());
                        let args = call.get("args").cloned().unwrap_or(Value::Null);
                        let args = if decode_args {
                            decode_arg_values(args)
                        } else {
                            args
                        };
                        (name, args)
                    })
                    .collect()
            })
            .unwrap_or_default();
        let has_tokens = ["input_tokens", "cache_read_tokens", "output_tokens"]
            .iter()
            .any(|key| value.get(*key).is_some());
        let usage = has_tokens.then(|| {
            // input_tokens excludes the cached part (cache reads exceed it on later turns);
            // output_tokens already includes thinking, which the transcript does not split out.
            NormalizedUsage {
                uncached_input: count(value.get("input_tokens")),
                cache_read: count(value.get("cache_read_tokens")),
                cache_creation: 0,
                cache_creation_1h: 0,
                output: count(value.get("output_tokens")),
                reasoning_output: 0,
            }
        });
        Some(Self {
            index,
            kind,
            timestamp: value
                .get("created_at")
                .and_then(Value::as_str)
                .and_then(parser::iso_to_ms),
            content: nonempty_string(value.get("content")),
            thinking: nonempty_string(value.get("thinking")),
            tool_calls,
            model: nonempty_string(value.get("model")),
            usage,
        })
    }
}

fn count(value: Option<&Value>) -> u64 {
    value
        .and_then(|value| value.as_u64().or_else(|| value.as_f64().map(|v| v as u64)))
        .unwrap_or(0)
}

/// `transcript.jsonl` JSON-encodes every argument value (`"5000"`, `"\"ls\""`); undo that so both
/// transcripts show the same arguments.
fn decode_arg_values(args: Value) -> Value {
    match args {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, value)| {
                    let decoded = match &value {
                        Value::String(text) => serde_json::from_str(text).unwrap_or(value),
                        _ => value,
                    };
                    (key, decoded)
                })
                .collect(),
        ),
        other => other,
    }
}

/// Read every step, keyed by `step_index` so a rewritten line replaces the earlier copy.
fn read_steps(path: &Path) -> Option<Vec<Step>> {
    let decode_args = path
        .file_name()
        .is_some_and(|name| name == PLAIN_TRANSCRIPT);
    let mut steps = BTreeMap::new();
    parser::for_each_jsonl_line(path, |line_no, line| {
        if let Some(step) = Step::parse(line_no, line, decode_args) {
            steps.insert(step.index, step);
        }
    })
    .ok()?;
    Some(steps.into_values().collect())
}

/// The text the user typed: the `<USER_REQUEST>` block, without the injected local time and
/// model-change notes that follow it.
pub(crate) fn user_request_text(content: &str) -> Option<String> {
    let text = match between(content, "<USER_REQUEST>", "</USER_REQUEST>") {
        Some(inner) => inner.to_string(),
        None => strip_block(
            &strip_block(content, "ADDITIONAL_METADATA"),
            "USER_SETTINGS_CHANGE",
        ),
    };
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

fn between<'a>(text: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let start = text.find(open)? + open.len();
    let end = text[start..].find(close)? + start;
    Some(&text[start..end])
}

fn strip_block(text: &str, tag: &str) -> String {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(&open) {
        out.push_str(&rest[..start]);
        match rest[start..].find(&close) {
            Some(end) => rest = &rest[start + end + close.len()..],
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Tool results open with `Created At:` / `Completed At:` lines (often twice); the output follows.
pub(crate) fn tool_result_text(content: &str) -> String {
    let mut rest = content;
    loop {
        let line_end = rest.find('\n').map_or(rest.len(), |end| end + 1);
        let line = &rest[..line_end];
        if line.starts_with("Created At:") || line.starts_with("Completed At:") {
            rest = &rest[line_end..];
            if rest.is_empty() {
                break;
            }
        } else {
            break;
        }
    }
    rest.trim_end().to_string()
}

/// A `SYSTEM_MESSAGE` from another conversation: `[Message] timestamp=… sender=<id> priority=…
/// content=<text>` inside the `<SYSTEM_MESSAGE>` block. Returns the sender and the text.
pub(crate) fn peer_message(content: &str) -> Option<(String, String)> {
    // The preamble itself mentions `<SYSTEM_MESSAGE>`, so look for the message header instead.
    let start = content.find("[Message]")? + "[Message]".len();
    let mut header = &content[start..];
    if let Some(end) = header.rfind("</SYSTEM_MESSAGE>") {
        header = &header[..end];
    }
    let sender_start = header.find("sender=")? + "sender=".len();
    let sender: String = header[sender_start..]
        .chars()
        .take_while(|character| !character.is_whitespace())
        .collect();
    if !looks_like_uuid(&sender) {
        return None;
    }
    let text = header
        .find("content=")
        .map(|start| header[start + "content=".len()..].trim().to_string())
        .unwrap_or_default();
    Some((sender, text))
}

fn looks_like_uuid(text: &str) -> bool {
    text.len() == 36
        && text.chars().enumerate().all(|(index, character)| {
            if [8, 13, 18, 23].contains(&index) {
                character == '-'
            } else {
                character.is_ascii_hexdigit()
            }
        })
}

// ----------------------------- Parse -----------------------------

/// Parse one Antigravity CLI transcript for the index.
pub fn parse_transcript_file(path: &Path) -> Option<ConvFileResult> {
    parse(path, None).map(|(result, _)| result)
}

/// Parse one Antigravity CLI transcript into the conversation view's messages.
pub fn parse_transcript_detail_with_limit(
    path: &Path,
    block_limit: BlockLimit,
) -> Option<ConversationDetail> {
    let (result, messages) = parse(path, Some(block_limit))?;
    Some(ConversationDetail {
        agent: Agent::Antigravity,
        session_id: result.session_id,
        project: result.project.unwrap_or_default(),
        git_branch: None,
        started_at: result.started_at,
        ended_at: result.ended_at,
        cli_version: None,
        source: result.source,
        models: result.models,
        messages,
        usage: SessionUsage::default(),
        usage_breakdown: UsageBreakdown::default(),
    })
}

/// `detail` is the block limit when the caller wants messages, `None` for the index only.
fn parse(path: &Path, detail: Option<BlockLimit>) -> Option<(ConvFileResult, Vec<ChatMessage>)> {
    let session_id = conversation_id(path)?;
    let steps = read_steps(path)?;
    if steps.is_empty() {
        return None;
    }
    let limit = detail.unwrap_or(DISPLAY_BLOCK_LIMIT);

    // A sub-agent's conversation opens with its parent's task, sent as a peer message.
    let has_user_input = steps.iter().any(|step| step.kind == "USER_INPUT");
    let parent_session_id = if has_user_input {
        None
    } else {
        steps
            .iter()
            .find(|step| step.kind == "SYSTEM_MESSAGE")
            .and_then(|step| step.content.as_deref())
            .and_then(peer_message)
            .map(|(sender, _)| sender)
    };

    let mut messages = Vec::new();
    let mut user_prompts = Vec::new();
    let mut usage_entries = Vec::new();
    let mut models: Vec<String> = Vec::new();
    let mut first_prompt = String::new();
    let mut message_count = 0usize;
    let mut started_at = i64::MAX;
    let mut ended_at = 0i64;
    let mut pending_tools: VecDeque<String> = VecDeque::new();

    for step in &steps {
        let timestamp = step.timestamp.unwrap_or(0);
        if let Some(time) = step.timestamp {
            started_at = started_at.min(time);
            ended_at = ended_at.max(time);
        }
        let uuid = format!("step-{}", step.index);
        match step.kind.as_str() {
            "USER_INPUT" => {
                let Some(text) = step.content.as_deref().and_then(user_request_text) else {
                    continue;
                };
                message_count += 1;
                if first_prompt.is_empty() {
                    first_prompt = text.clone();
                }
                user_prompts.push(RawPrompt {
                    agent: Agent::Antigravity,
                    text: text.clone(),
                    project: String::new(),
                    timestamp,
                    session_id: Some(session_id.clone()),
                    git_branch: None,
                    pasted_count: 0,
                    from_history: false,
                });
                if detail.is_some() {
                    messages.push(message(
                        uuid,
                        "user",
                        timestamp,
                        vec![block("text", &text, None, limit)],
                    ));
                }
            }
            "PLANNER_RESPONSE" => {
                message_count += 1;
                let call = step.usage.map(|usage| {
                    let model = step.model.clone().unwrap_or_default();
                    let dedup_key = format!("{session_id}:{}", step.index);
                    usage_entries.push(UsageEntry {
                        agent: Agent::Antigravity,
                        dedup_key: dedup_key.clone(),
                        model: model.clone(),
                        timestamp,
                        project: String::new(),
                        usage,
                        effort: None,
                    });
                    MessageUsage {
                        call_key: dedup_key,
                        model,
                        usage,
                        effort: None,
                        est_cost_usd: None,
                        attributed: true,
                    }
                });
                if let Some(model) = &step.model {
                    if !models.contains(model) {
                        models.push(model.clone());
                    }
                }
                for (name, _) in &step.tool_calls {
                    pending_tools.push_back(name.clone());
                }
                if detail.is_some() {
                    let mut blocks = Vec::new();
                    if let Some(thinking) = &step.thinking {
                        blocks.push(block("thinking", thinking, None, limit));
                    }
                    if let Some(text) = &step.content {
                        blocks.push(block("text", text, None, limit));
                    }
                    for (name, args) in &step.tool_calls {
                        blocks.push(ContentBlock {
                            kind: "tool_use".to_string(),
                            text: None,
                            tool_name: Some(name.clone()),
                            tool_input: Some(args.clone()),
                            truncated: false,
                            persisted_output: None,
                        });
                    }
                    if !blocks.is_empty() || call.is_some() {
                        let mut chat = message(uuid, "assistant", timestamp, blocks);
                        chat.usage = call;
                        messages.push(chat);
                    }
                }
            }
            "GENERIC" => {
                // Results follow their calls in order; the transcript carries no call ID.
                let tool_name = pending_tools.pop_front();
                if detail.is_some() {
                    let text = step
                        .content
                        .as_deref()
                        .map(tool_result_text)
                        .unwrap_or_default();
                    messages.push(message(
                        uuid,
                        "assistant",
                        timestamp,
                        vec![block("tool_result", &text, tool_name, limit)],
                    ));
                }
            }
            "SYSTEM_MESSAGE" if detail.is_some() && parent_session_id.is_none() => {
                // Reminders and notices are not conversation content; a sub-agent's report is.
                let Some((_, text)) = step.content.as_deref().and_then(peer_message) else {
                    continue;
                };
                messages.push(message(
                    uuid,
                    "assistant",
                    timestamp,
                    vec![block(
                        "tool_result",
                        &text,
                        Some(SUBAGENT_MESSAGE_TOOL.to_string()),
                        limit,
                    )],
                ));
            }
            _ => {}
        }
    }

    if started_at == i64::MAX {
        started_at = 0;
    }
    models.sort();
    let result = ConvFileResult {
        agent: Agent::Antigravity,
        path: path.to_path_buf(),
        session_id,
        project: None,
        git_branch: None,
        version: None,
        source: Some(SOURCE.to_string()),
        models,
        is_subagent: parent_session_id.is_some(),
        parent_session_id,
        project_hash: None,
        started_at,
        ended_at,
        message_count,
        first_prompt,
        user_prompts,
        usage_entries,
    };
    Some((result, messages))
}

fn message(uuid: String, role: &str, timestamp: i64, blocks: Vec<ContentBlock>) -> ChatMessage {
    ChatMessage {
        agent: Agent::Antigravity,
        uuid,
        role: role.to_string(),
        timestamp,
        is_sidechain: false,
        blocks,
        usage: None,
    }
}

pub(crate) fn block(
    kind: &str,
    text: &str,
    tool_name: Option<String>,
    limit: BlockLimit,
) -> ContentBlock {
    let (text, truncated) = clip_to(text, limit);
    ContentBlock {
        kind: kind.to_string(),
        text: Some(text),
        tool_name,
        tool_input: None,
        truncated,
        persisted_output: None,
    }
}

// ----------------------------- Either file kind -----------------------------

/// Parse any file of the Antigravity source for the index: an Antigravity CLI transcript or a
/// Gemini CLI session.
pub fn parse_file(path: &Path) -> Option<ConvFileResult> {
    if is_transcript(path) {
        parse_transcript_file(path)
    } else {
        crate::gemini_parser::parse_session_file(path)
    }
}

/// Conversation detail for any file of the Antigravity source.
pub fn parse_detail_with_limit(path: &Path, block_limit: BlockLimit) -> Option<ConversationDetail> {
    if is_transcript(path) {
        parse_transcript_detail_with_limit(path, block_limit)
    } else {
        crate::gemini_parser::parse_session_detail_with_limit(path, block_limit)
    }
}
