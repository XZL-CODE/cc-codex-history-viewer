//! 旧版 Gemini CLI 会话：`tmp/<项目>/chats/session-*.jsonl`（追加式记录），更早版本的单文件
//! `session-*.json`，以及子代理的 `chats/<父会话 id>/<子会话 id>.jsonl`。
//!
//! 追加式记录按 Gemini CLI 自己的回放规则还原：首行是会话元数据；带 `id` 的行新增或替换一条
//! 消息；`$set` 更新元数据（带 `messages` 时整体替换消息）；`$patch` 修改、删除或重排消息；
//! `$rewindTo` 删除该消息及之后的全部消息。项目目录名旁的 `.project_root` 记着项目路径。

use crate::antigravity_parser::block;
use crate::codex_parser::nonempty_string;
use crate::models::{
    Agent, ChatMessage, ContentBlock, ConversationDetail, MessageUsage, NormalizedUsage,
    SessionUsage, UsageBreakdown,
};
use crate::parser::{self, BlockLimit, ConvFileResult, RawPrompt, UsageEntry, DISPLAY_BLOCK_LIMIT};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// `SessionSummary.source` of every Gemini CLI session.
pub const SOURCE: &str = "gemini-cli";
const SESSION_PREFIX: &str = "session-";

/// True for a session file below `chats/`: `session-*.jsonl` / `session-*.json` directly inside
/// it, or a sub-agent's `<parent id>/<child id>.jsonl` one level down.
pub fn is_session_file(path: &Path, chats_dir: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(chats_dir) else {
        return false;
    };
    let depth = relative.components().count();
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let extension = path.extension().and_then(|ext| ext.to_str());
    match depth {
        1 => name.starts_with(SESSION_PREFIX) && matches!(extension, Some("jsonl" | "json")),
        2 => extension == Some("jsonl"),
        _ => false,
    }
}

// ----------------------------- Replay -----------------------------

/// The conversation as Gemini CLI itself would load it.
#[derive(Default)]
struct Replay {
    metadata: Map<String, Value>,
    /// Messages in order; `None` marks a removed slot.
    slots: Vec<Option<Value>>,
    positions: HashMap<String, usize>,
}

impl Replay {
    fn messages(&self) -> impl Iterator<Item = &Value> {
        self.slots.iter().flatten()
    }

    fn upsert(&mut self, message: Value) {
        let Some(id) = nonempty_string(message.get("id")) else {
            return;
        };
        match self.positions.get(&id) {
            Some(&position) => self.slots[position] = Some(message),
            None => {
                self.positions.insert(id, self.slots.len());
                self.slots.push(Some(message));
            }
        }
    }

    fn remove(&mut self, id: &str) {
        if let Some(position) = self.positions.remove(id) {
            self.slots[position] = None;
        }
    }

    fn rebuild(&mut self, messages: Vec<Value>) {
        self.slots.clear();
        self.positions.clear();
        for message in messages {
            self.upsert(message);
        }
    }

    fn apply_record(&mut self, record: Value) {
        let Value::Object(mut object) = record else {
            return;
        };
        if let Some(target) = object.get("$rewindTo").and_then(Value::as_str) {
            // Drop the target and everything after it; an unknown target clears all.
            let ordered: Vec<Value> = self.messages().cloned().collect();
            let kept = match ordered
                .iter()
                .position(|message| message.get("id").and_then(Value::as_str) == Some(target))
            {
                Some(position) => ordered[..position].to_vec(),
                None => Vec::new(),
            };
            self.rebuild(kept);
            return;
        }
        if let Some(Value::Object(patch)) = object.remove("$patch") {
            self.apply_patch(patch);
            return;
        }
        if let Some(Value::Object(mut set)) = object.remove("$set") {
            if let Some(Value::Array(messages)) = set.remove("messages") {
                self.rebuild(messages);
            }
            self.metadata.extend(set);
            return;
        }
        if object.get("id").and_then(Value::as_str).is_some() {
            self.upsert(Value::Object(object));
            return;
        }
        if object.get("sessionId").and_then(Value::as_str).is_some() {
            // Session header, or a whole legacy `.json` conversation.
            if let Some(Value::Array(messages)) = object.remove("messages") {
                for message in messages {
                    self.upsert(message);
                }
            }
            self.metadata.extend(object);
        }
    }

    fn apply_patch(&mut self, mut patch: Map<String, Value>) {
        let mut updates = Vec::new();
        if patch.get("id").and_then(Value::as_str).is_some() {
            updates.push(Value::Object(patch.clone()));
        }
        if let Some(Value::Array(more)) = patch.remove("updates") {
            updates.extend(more);
        }
        for update in updates {
            self.apply_single_patch(&update);
        }
        if let Some(Value::Array(ids)) = patch.remove("removeIds") {
            for id in ids.iter().filter_map(Value::as_str) {
                self.remove(id);
            }
        }
        if let Some(Value::Array(ids)) = patch.remove("orderIds") {
            // Listed messages move to the end in the given order; the rest keep theirs.
            let listed: Vec<&str> = ids.iter().filter_map(Value::as_str).collect();
            let mut prefix = Vec::new();
            let mut ordered: Vec<Option<Value>> = vec![None; listed.len()];
            for message in self.messages() {
                let id = message
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                match listed.iter().position(|wanted| *wanted == id) {
                    Some(position) => ordered[position] = Some(message.clone()),
                    None => prefix.push(message.clone()),
                }
            }
            prefix.extend(ordered.into_iter().flatten());
            self.rebuild(prefix);
        }
    }

    fn apply_single_patch(&mut self, patch: &Value) {
        let Some(id) = patch.get("id").and_then(Value::as_str) else {
            return;
        };
        let Some(&position) = self.positions.get(id) else {
            return;
        };
        let Some(Value::Object(existing)) = self.slots[position].as_mut() else {
            return;
        };
        if let Some(content) = patch.get("content").filter(|content| !content.is_null()) {
            existing.insert("content".to_string(), content.clone());
        }
        let Some(tool_patches) = patch.get("toolCalls").and_then(Value::as_array) else {
            return;
        };
        let Some(Value::Array(calls)) = existing.get_mut("toolCalls") else {
            return;
        };
        for tool_patch in tool_patches {
            let Some(call_id) = tool_patch.get("id").and_then(Value::as_str) else {
                continue;
            };
            let Some(result) = tool_patch.get("result") else {
                continue;
            };
            if let Some(Value::Object(call)) = calls
                .iter_mut()
                .find(|call| call.get("id").and_then(Value::as_str) == Some(call_id))
            {
                call.insert("result".to_string(), result.clone());
            }
        }
    }
}

fn replay(path: &Path) -> Option<Replay> {
    let mut replay = Replay::default();
    if path.extension().is_some_and(|ext| ext == "json") {
        let text = fs::read_to_string(path).ok()?;
        replay.apply_record(serde_json::from_str(&text).ok()?);
    } else {
        parser::for_each_jsonl_line(path, |_, line| {
            if let Ok(record) = serde_json::from_str::<Value>(line) {
                replay.apply_record(record);
            }
        })
        .ok()?;
    }
    Some(replay)
}

// ----------------------------- Message content -----------------------------

/// Text parts of a `PartListUnion` (a string, one part, or a list of parts), joined.
pub(crate) fn parts_text(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(|part| match part {
                Value::String(text) => Some(text.as_str()),
                _ => part.get("text").and_then(Value::as_str),
            })
            .collect::<Vec<_>>()
            .join(""),
        Some(Value::Object(part)) => part
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        _ => String::new(),
    }
}

/// Function responses carried by a `PartListUnion`: tool name and output text.
fn function_responses(content: Option<&Value>) -> Vec<(Option<String>, String)> {
    let parts: Vec<&Value> = match content {
        Some(Value::Array(parts)) => parts.iter().collect(),
        Some(part @ Value::Object(_)) => vec![part],
        _ => Vec::new(),
    };
    parts
        .into_iter()
        .filter_map(|part| part.get("functionResponse"))
        .map(|response| {
            let name = nonempty_string(response.get("name"));
            let body = response.get("response");
            let text = body
                .and_then(|body| body.get("output").or_else(|| body.get("error")))
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| body.map(|body| serde_json::to_string_pretty(body).unwrap_or_default()))
                .unwrap_or_default();
            (name, text)
        })
        .collect()
}

/// What the user typed; injected context and empty input are not prompts.
pub(crate) fn user_text(message: &Value) -> Option<String> {
    let shown = message
        .get("displayContent")
        .filter(|content| !content.is_null());
    let text = parts_text(shown.or_else(|| message.get("content")));
    let trimmed = text.trim();
    let injected =
        trimmed.starts_with("<session_context>") || trimmed.starts_with("<hook_context>");
    (!trimmed.is_empty() && !injected).then(|| trimmed.to_string())
}

/// Thought summaries as one thinking block: `**subject** description` per thought.
pub(crate) fn thoughts_text(message: &Value) -> String {
    message
        .get("thoughts")
        .and_then(Value::as_array)
        .map(|thoughts| {
            thoughts
                .iter()
                .filter_map(|thought| {
                    let subject = nonempty_string(thought.get("subject"));
                    let description = nonempty_string(thought.get("description"));
                    match (subject, description) {
                        (Some(subject), Some(description)) => {
                            Some(format!("**{subject}** {description}"))
                        }
                        (Some(text), None) | (None, Some(text)) => Some(text),
                        (None, None) => None,
                    }
                })
                .collect::<Vec<_>>()
                .join("\n\n")
        })
        .unwrap_or_default()
}

/// A tool call's output: its function response, else the text shown in the terminal.
fn tool_call_result(call: &Value) -> Option<String> {
    let from_result = function_responses(call.get("result"))
        .into_iter()
        .map(|(_, text)| text)
        .collect::<Vec<_>>()
        .join("\n");
    if !from_result.trim().is_empty() {
        return Some(from_result);
    }
    let plain = parts_text(call.get("result"));
    if !plain.trim().is_empty() {
        return Some(plain);
    }
    match call.get("resultDisplay") {
        Some(Value::String(text)) if !text.trim().is_empty() => Some(text.clone()),
        Some(display) => display
            .get("result")
            .or_else(|| display.get("summary"))
            .and_then(Value::as_str)
            .map(str::to_string),
        None => None,
    }
}

/// Gemini CLI token counts: `input` includes the cached part; thoughts and tool-use prompt
/// tokens are counted beside it (`total = input + output + thoughts + tool`).
fn normalize_tokens(tokens: &Value) -> NormalizedUsage {
    let number = |key: &str| {
        tokens
            .get(key)
            .and_then(|value| value.as_u64().or_else(|| value.as_f64().map(|v| v as u64)))
            .unwrap_or(0)
    };
    let input = number("input");
    let cached = number("cached").min(input);
    let thoughts = number("thoughts");
    NormalizedUsage {
        uncached_input: input - cached + number("tool"),
        cache_read: cached,
        cache_creation: 0,
        cache_creation_1h: 0,
        output: number("output") + thoughts,
        reasoning_output: thoughts,
    }
}

// ----------------------------- Project path -----------------------------

/// `tmp/<dir>/chats/...` → `tmp/<dir>`.
fn project_dir(path: &Path) -> Option<&Path> {
    path.ancestors()
        .find(|ancestor| ancestor.file_name().is_some_and(|name| name == "chats"))?
        .parent()
}

fn read_marker(path: &Path) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    let line = text.lines().next()?.trim();
    (!line.is_empty()).then(|| line.to_string())
}

fn is_sha256_hex(text: &str) -> bool {
    text.len() == 64 && text.chars().all(|character| character.is_ascii_hexdigit())
}

pub(crate) fn project_hash(path: &str) -> String {
    let digest = Sha256::digest(path.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// The project a session belongs to. Current Gemini CLI names each directory after the project
/// and writes the path into `.project_root`; `projects.json` maps paths to those names. Older
/// versions named the directory after `projectHash`, the SHA-256 of the path.
fn resolve_project(path: &Path, hash: Option<&str>) -> Option<String> {
    let dir = project_dir(path)?;
    let name = dir.file_name()?.to_string_lossy().to_string();
    if let Some(root) = read_marker(&dir.join(".project_root")) {
        return Some(root);
    }
    let gemini_root = dir.parent()?.parent()?;
    if let Some(root) = read_marker(
        &gemini_root
            .join("history")
            .join(&name)
            .join(".project_root"),
    ) {
        return Some(root);
    }
    let projects: Value = fs::read_to_string(gemini_root.join("projects.json"))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())?;
    let projects = projects.get("projects")?.as_object()?;
    let wanted_hash = hash.or_else(|| is_sha256_hex(&name).then_some(name.as_str()));
    projects.iter().find_map(|(project, alias)| {
        let by_alias = alias.as_str() == Some(name.as_str());
        let by_hash = wanted_hash.is_some_and(|wanted| project_hash(project) == wanted);
        (by_alias || by_hash).then(|| project.clone())
    })
}

// ----------------------------- Parse -----------------------------

/// Parse one Gemini CLI session for the index.
pub fn parse_session_file(path: &Path) -> Option<ConvFileResult> {
    parse(path, None).map(|(result, _)| result)
}

/// Parse one Gemini CLI session into the conversation view's messages.
pub fn parse_session_detail_with_limit(
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

fn parse(path: &Path, detail: Option<BlockLimit>) -> Option<(ConvFileResult, Vec<ChatMessage>)> {
    let replay = replay(path)?;
    let metadata = &replay.metadata;
    let session_id = nonempty_string(metadata.get("sessionId")).or_else(|| {
        path.file_stem()
            .map(|stem| stem.to_string_lossy().to_string())
    })?;
    let limit = detail.unwrap_or(DISPLAY_BLOCK_LIMIT);
    let is_subagent = metadata.get("kind").and_then(Value::as_str) == Some("subagent");
    let parent_session_id = is_subagent
        .then(|| {
            path.parent()?
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
        })
        .flatten();
    let hash = nonempty_string(metadata.get("projectHash"));
    let project = resolve_project(path, hash.as_deref()).or_else(|| {
        metadata
            .get("directories")
            .and_then(Value::as_array)
            .and_then(|directories| directories.first())
            .and_then(Value::as_str)
            .map(str::to_string)
    });

    let mut messages = Vec::new();
    let mut user_prompts = Vec::new();
    let mut usage_entries = Vec::new();
    let mut models: Vec<String> = Vec::new();
    let mut first_prompt = String::new();
    let mut message_count = 0usize;
    let mut first_time: Option<i64> = None;
    let mut last_time: Option<i64> = None;

    for record in replay.messages() {
        let Some(id) = nonempty_string(record.get("id")) else {
            continue;
        };
        let timestamp = record
            .get("timestamp")
            .and_then(Value::as_str)
            .and_then(parser::iso_to_ms);
        if let Some(time) = timestamp {
            first_time = Some(first_time.map_or(time, |first| first.min(time)));
            last_time = Some(last_time.map_or(time, |last| last.max(time)));
        }
        let timestamp = timestamp.unwrap_or(0);
        match record
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
        {
            "user" => {
                // Tool responses recorded as user turns are results, not input.
                let responses = function_responses(record.get("content"));
                if let Some(text) = user_text(record) {
                    message_count += 1;
                    if first_prompt.is_empty() {
                        first_prompt = text.clone();
                    }
                    user_prompts.push(RawPrompt {
                        agent: Agent::Antigravity,
                        text: text.clone(),
                        project: project.clone().unwrap_or_default(),
                        timestamp,
                        session_id: Some(session_id.clone()),
                        git_branch: None,
                        pasted_count: 0,
                        from_history: false,
                    });
                    if detail.is_some() {
                        messages.push(message(
                            id.clone(),
                            "user",
                            timestamp,
                            vec![block("text", &text, None, limit)],
                        ));
                    }
                }
                if detail.is_some() && !responses.is_empty() {
                    let blocks = responses
                        .iter()
                        .map(|(name, text)| block("tool_result", text, name.clone(), limit))
                        .collect();
                    messages.push(message(
                        format!("{id}#results"),
                        "assistant",
                        timestamp,
                        blocks,
                    ));
                }
            }
            "gemini" => {
                message_count += 1;
                let model = nonempty_string(record.get("model"));
                if let Some(model) = &model {
                    if !models.contains(model) {
                        models.push(model.clone());
                    }
                }
                let call = record
                    .get("tokens")
                    .filter(|tokens| tokens.is_object())
                    .map(|tokens| {
                        let usage = normalize_tokens(tokens);
                        let model = model.clone().unwrap_or_default();
                        let dedup_key = format!("{session_id}:{id}");
                        usage_entries.push(UsageEntry {
                            agent: Agent::Antigravity,
                            dedup_key: dedup_key.clone(),
                            model: model.clone(),
                            timestamp,
                            project: project.clone().unwrap_or_default(),
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
                if detail.is_none() {
                    continue;
                }
                let calls: &[Value] = record
                    .get("toolCalls")
                    .and_then(Value::as_array)
                    .map(Vec::as_slice)
                    .unwrap_or_default();
                let mut blocks = Vec::new();
                let thoughts = thoughts_text(record);
                if !thoughts.is_empty() {
                    blocks.push(block("thinking", &thoughts, None, limit));
                }
                let text = parts_text(record.get("content"));
                if !text.trim().is_empty() {
                    blocks.push(block("text", &text, None, limit));
                }
                for tool in calls {
                    blocks.push(ContentBlock {
                        kind: "tool_use".to_string(),
                        text: None,
                        tool_name: nonempty_string(tool.get("name")),
                        tool_input: tool.get("args").cloned(),
                        truncated: false,
                        persisted_output: None,
                    });
                }
                if !blocks.is_empty() || call.is_some() {
                    let mut chat = message(id.clone(), "assistant", timestamp, blocks);
                    chat.usage = call;
                    messages.push(chat);
                }
                let results: Vec<ContentBlock> = calls
                    .iter()
                    .filter_map(|tool| {
                        tool_call_result(tool).map(|text| {
                            block(
                                "tool_result",
                                &text,
                                nonempty_string(tool.get("name")),
                                limit,
                            )
                        })
                    })
                    .collect();
                if !results.is_empty() {
                    messages.push(message(
                        format!("{id}#results"),
                        "assistant",
                        timestamp,
                        results,
                    ));
                }
            }
            _ => {}
        }
    }

    let header_time = |key: &str| {
        metadata
            .get(key)
            .and_then(Value::as_str)
            .and_then(parser::iso_to_ms)
    };
    let started_at = header_time("startTime").or(first_time).unwrap_or(0);
    let ended_at = last_time
        .into_iter()
        .chain(header_time("lastUpdated"))
        .max()
        .unwrap_or(started_at);
    models.sort();
    let result = ConvFileResult {
        agent: Agent::Antigravity,
        path: path.to_path_buf(),
        session_id,
        project_hash: if project.is_none() { hash } else { None },
        project,
        git_branch: None,
        version: None,
        source: Some(SOURCE.to_string()),
        models,
        is_subagent,
        parent_session_id,
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
