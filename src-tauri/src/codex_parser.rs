//! Streaming parser adapter for OpenAI Codex history and rollout JSONL files.

use crate::models::{
    Agent, ChatMessage, ContentBlock, ConversationDetail, NormalizedUsage, SessionUsage,
};
use crate::parser::{
    clip, for_each_jsonl_line, stable_hash, ConvFileResult, RawPrompt, UsageEntry,
};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::Path;

const MAX_BLOCK_CHARS: usize = 24_000;
const PROMPT_TRANSITION_DEDUP_WINDOW_MS: u64 = 5 * 60 * 1_000;

#[derive(Clone, Debug, Default)]
struct SessionMeta {
    id: String,
    cwd: Option<String>,
    git_branch: Option<String>,
    cli_version: Option<String>,
    source: Option<String>,
    is_subagent: bool,
    timestamp: Option<i64>,
}

#[derive(Debug)]
struct PendingPrompt {
    line_no: usize,
    prompt: RawPrompt,
}

#[derive(Debug)]
struct PendingMessage {
    line_no: usize,
    message: ChatMessage,
}

struct RolloutAccumulator {
    collect_detail: bool,
    file_stem: String,
    meta: Vec<SessionMeta>,
    matching_meta: Option<SessionMeta>,
    fallback_project: Option<String>,
    current_cwd: Option<String>,
    current_model: Option<String>,
    current_turn_id: Option<String>,
    models: Vec<String>,
    earliest_timestamp: Option<i64>,
    latest_timestamp: Option<i64>,
    event_prompts: Vec<PendingPrompt>,
    legacy_prompts: Vec<PendingPrompt>,
    usage_entries: Vec<UsageEntry>,
    seen_usage: HashSet<String>,
    response_assistant_count: usize,
    response_assistant_texts: HashSet<u64>,
    event_assistant_texts: Vec<u64>,
    messages: Vec<PendingMessage>,
    event_user_messages: Vec<PendingMessage>,
    legacy_user_messages: Vec<PendingMessage>,
    event_assistant_messages: Vec<(u64, PendingMessage)>,
    tool_names: HashMap<String, String>,
}

impl RolloutAccumulator {
    fn new(path: &Path, collect_detail: bool) -> Option<Self> {
        let file_stem = path.file_stem()?.to_string_lossy().to_string();
        Some(Self {
            collect_detail,
            file_stem,
            meta: Vec::new(),
            matching_meta: None,
            fallback_project: None,
            current_cwd: None,
            current_model: None,
            current_turn_id: None,
            models: Vec::new(),
            earliest_timestamp: None,
            latest_timestamp: None,
            event_prompts: Vec::new(),
            legacy_prompts: Vec::new(),
            usage_entries: Vec::new(),
            seen_usage: HashSet::new(),
            response_assistant_count: 0,
            response_assistant_texts: HashSet::new(),
            event_assistant_texts: Vec::new(),
            messages: Vec::new(),
            event_user_messages: Vec::new(),
            legacy_user_messages: Vec::new(),
            event_assistant_messages: Vec::new(),
            tool_names: HashMap::new(),
        })
    }

    fn process_line(&mut self, line_no: usize, line: &str) {
        if line.is_empty() {
            return;
        }
        let record: Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(_) => return,
        };
        let record_type = record.get("type").and_then(Value::as_str).unwrap_or("");
        let raw_timestamp = record.get("timestamp");
        let timestamp = timestamp_ms(raw_timestamp);
        self.observe_timestamp(timestamp);
        let payload = match record.get("payload") {
            Some(Value::Object(_)) => &record["payload"],
            _ => return,
        };

        match record_type {
            "session_meta" => self.process_session_meta(payload, timestamp),
            "turn_context" => self.process_turn_context(payload),
            "event_msg" => self.process_event(line_no, payload, raw_timestamp, timestamp),
            "response_item" => {
                self.process_response_item(line_no, payload, raw_timestamp, timestamp)
            }
            _ => {}
        }
    }

    fn process_session_meta(&mut self, payload: &Value, top_timestamp: Option<i64>) {
        let id = nonempty_string(payload.get("session_id"))
            .or_else(|| nonempty_string(payload.get("id")))
            .unwrap_or_default();
        let cwd = nonempty_string(payload.get("cwd"));
        let source_value = payload.get("source");
        let is_subagent = matches!(
            source_value.and_then(Value::as_str),
            Some("subagent" | "sub_agent")
        ) || source_value
            .and_then(Value::as_object)
            .is_some_and(|source| {
                source.contains_key("subagent") || source.contains_key("sub_agent")
            })
            || payload.get("thread_source").and_then(Value::as_str) == Some("subagent");
        let source = match source_value {
            Some(Value::String(value)) if !value.is_empty() => Some(value.clone()),
            Some(Value::Object(_)) if is_subagent => Some("subagent".to_string()),
            Some(Value::Object(_)) => Some("other".to_string()),
            _ => None,
        };
        let git_branch = payload
            .get("git")
            .and_then(|git| nonempty_string(git.get("branch")));
        let meta = SessionMeta {
            id: id.clone(),
            cwd: cwd.clone(),
            git_branch,
            cli_version: nonempty_string(payload.get("cli_version")),
            source,
            is_subagent,
            timestamp: timestamp_ms(payload.get("timestamp")).or(top_timestamp),
        };

        let matches_file = !id.is_empty() && self.file_stem.ends_with(&id);
        if matches_file {
            self.matching_meta = Some(meta.clone());
            self.current_cwd = cwd.clone();
            self.fallback_project = cwd;
        } else if self.meta.is_empty() {
            self.current_cwd = cwd.clone();
            self.fallback_project = cwd;
        }
        self.meta.push(meta);
    }

    fn process_turn_context(&mut self, payload: &Value) {
        if let Some(model) = nonempty_string(payload.get("model")) {
            self.current_model = Some(model.clone());
            self.add_model(model);
        }
        if let Some(cwd) = nonempty_string(payload.get("cwd")) {
            if self.fallback_project.is_none() {
                self.fallback_project = Some(cwd.clone());
            }
            self.current_cwd = Some(cwd);
        }
        if let Some(turn_id) = nonempty_string(payload.get("turn_id")) {
            self.current_turn_id = Some(turn_id);
        }
    }

    fn process_event(
        &mut self,
        line_no: usize,
        payload: &Value,
        raw_timestamp: Option<&Value>,
        timestamp: Option<i64>,
    ) {
        match payload.get("type").and_then(Value::as_str).unwrap_or("") {
            "user_message" => self.process_event_user(line_no, payload, timestamp),
            "agent_message" => self.process_event_assistant(line_no, payload, timestamp),
            "task_started" => {
                if let Some(turn_id) = nonempty_string(payload.get("turn_id")) {
                    self.current_turn_id = Some(turn_id);
                }
            }
            "token_count" => self.process_token_count(payload, raw_timestamp, timestamp),
            _ => {}
        }
    }

    fn process_event_user(&mut self, line_no: usize, payload: &Value, timestamp: Option<i64>) {
        let mut text = nonempty_string(payload.get("message")).unwrap_or_default();
        let has_images = value_is_nonempty_array(payload.get("images"))
            || value_is_nonempty_array(payload.get("local_images"));
        if text.trim().is_empty() && has_images {
            text = "[Image]".to_string();
        }
        let text = text.trim().to_string();
        if text.is_empty() {
            return;
        }
        let ts = timestamp.unwrap_or(0);
        self.event_prompts.push(PendingPrompt {
            line_no,
            prompt: raw_prompt(text.clone(), self.current_cwd.clone(), ts),
        });
        if self.collect_detail {
            let mut blocks = vec![text_block(&text)];
            if has_images && text != "[Image]" {
                blocks.push(image_block());
            }
            self.event_user_messages.push(PendingMessage {
                line_no,
                message: chat_message(
                    stable_message_id("event-user", timestamp, payload),
                    "user",
                    ts,
                    blocks,
                ),
            });
        }
    }

    fn process_event_assistant(&mut self, line_no: usize, payload: &Value, timestamp: Option<i64>) {
        let text = match nonempty_string(payload.get("message")) {
            Some(text) => text,
            None => return,
        };
        let text_hash = text_fingerprint(&text);
        self.event_assistant_texts.push(text_hash);
        if self.collect_detail {
            self.event_assistant_messages.push((
                text_hash,
                PendingMessage {
                    line_no,
                    message: chat_message(
                        stable_message_id("event-assistant", timestamp, payload),
                        "assistant",
                        timestamp.unwrap_or(0),
                        vec![text_block(&text)],
                    ),
                },
            ));
        }
    }

    fn process_token_count(
        &mut self,
        payload: &Value,
        raw_timestamp: Option<&Value>,
        timestamp: Option<i64>,
    ) {
        // total_token_usage is cumulative and must never be added. Only the last call is a delta.
        let last = match payload
            .get("info")
            .and_then(|info| info.get("last_token_usage"))
            .and_then(Value::as_object)
        {
            Some(last) => last,
            None => return,
        };
        let input = value_u64(last.get("input_tokens"));
        let cached = value_u64(last.get("cached_input_tokens")).min(input);
        let output = value_u64(last.get("output_tokens"));
        let reasoning_output = value_u64(last.get("reasoning_output_tokens")).min(output);
        let usage = NormalizedUsage {
            uncached_input: input - cached,
            cache_read: cached,
            cache_creation: 0,
            output,
            reasoning_output,
        };
        if usage.total_tokens_including_cache() == 0 {
            return;
        }

        let model = self
            .current_model
            .clone()
            .unwrap_or_else(|| "unknown".to_string());
        self.add_model(model.clone());
        let timestamp_identity = scalar_identity(raw_timestamp);
        let turn_id = self.current_turn_id.as_deref().unwrap_or("");
        let uncached = usage.uncached_input.to_string();
        let cache_read = usage.cache_read.to_string();
        let output = usage.output.to_string();
        let reasoning = usage.reasoning_output.to_string();
        let dedup_key = format!(
            "codex:event:{:016x}",
            stable_hash(&[
                &timestamp_identity,
                turn_id,
                &model,
                &uncached,
                &cache_read,
                &output,
                &reasoning,
            ])
        );
        if !self.seen_usage.insert(dedup_key.clone()) {
            return;
        }
        self.usage_entries.push(UsageEntry {
            agent: Agent::Codex,
            dedup_key,
            model,
            timestamp: timestamp.unwrap_or(0),
            project: self
                .current_cwd
                .clone()
                .or_else(|| self.fallback_project.clone())
                .unwrap_or_default(),
            usage,
        });
    }

    fn process_response_item(
        &mut self,
        line_no: usize,
        payload: &Value,
        _raw_timestamp: Option<&Value>,
        timestamp: Option<i64>,
    ) {
        match payload.get("type").and_then(Value::as_str).unwrap_or("") {
            "message" => self.process_response_message(line_no, payload, timestamp),
            "agent_message" => self.process_response_agent_message(line_no, payload, timestamp),
            "reasoning" => self.process_reasoning(line_no, payload, timestamp),
            "function_call"
            | "custom_tool_call"
            | "web_search_call"
            | "tool_search_call"
            | "image_generation_call" => self.process_tool_call(line_no, payload, timestamp),
            "function_call_output" | "custom_tool_call_output" | "tool_search_output" => {
                self.process_tool_output(line_no, payload, timestamp)
            }
            _ => {}
        }
    }

    fn process_response_message(
        &mut self,
        line_no: usize,
        payload: &Value,
        timestamp: Option<i64>,
    ) {
        let role = payload.get("role").and_then(Value::as_str).unwrap_or("");
        match role {
            "user" => {
                let text = match legacy_user_content(payload.get("content")) {
                    Some(text) => text,
                    None => return,
                };
                let ts = timestamp.unwrap_or(0);
                self.legacy_prompts.push(PendingPrompt {
                    line_no,
                    prompt: raw_prompt(text.clone(), self.current_cwd.clone(), ts),
                });
                if self.collect_detail {
                    self.legacy_user_messages.push(PendingMessage {
                        line_no,
                        message: chat_message(
                            stable_message_id("legacy-user", timestamp, payload),
                            "user",
                            ts,
                            vec![text_block(&text)],
                        ),
                    });
                }
            }
            "assistant" => {
                let raw_assistant_text =
                    message_content_text(payload.get("content"), &["output_text", "text"]);
                if raw_assistant_text.is_empty() {
                    return;
                }
                self.response_assistant_count += 1;
                self.response_assistant_texts
                    .insert(text_fingerprint(&raw_assistant_text));
                if self.collect_detail {
                    let blocks = response_message_blocks(payload.get("content"));
                    self.messages.push(PendingMessage {
                        line_no,
                        message: chat_message(
                            stable_message_id("response-assistant", timestamp, payload),
                            "assistant",
                            timestamp.unwrap_or(0),
                            blocks,
                        ),
                    });
                }
            }
            // Developer and system records are injected context, not conversation messages.
            _ => {}
        }
    }

    fn process_response_agent_message(
        &mut self,
        line_no: usize,
        payload: &Value,
        timestamp: Option<i64>,
    ) {
        let text = match nonempty_string(payload.get("content")) {
            Some(text) => text,
            None => return,
        };
        let text_hash = text_fingerprint(&text);
        self.response_assistant_count += 1;
        self.response_assistant_texts.insert(text_hash);
        if self.collect_detail {
            self.messages.push(PendingMessage {
                line_no,
                message: chat_message(
                    stable_message_id("response-agent", timestamp, payload),
                    "assistant",
                    timestamp.unwrap_or(0),
                    vec![text_block(&text)],
                ),
            });
        }
    }

    fn process_reasoning(&mut self, line_no: usize, payload: &Value, timestamp: Option<i64>) {
        if !self.collect_detail {
            return;
        }
        let text = message_content_text(payload.get("summary"), &["summary_text", "text"]);
        if text.trim().is_empty() {
            return;
        }
        self.messages.push(PendingMessage {
            line_no,
            message: chat_message(
                stable_message_id("reasoning", timestamp, payload),
                "assistant",
                timestamp.unwrap_or(0),
                vec![clipped_block("thinking", &text, None)],
            ),
        });
    }

    fn process_tool_call(&mut self, line_no: usize, payload: &Value, timestamp: Option<i64>) {
        if !self.collect_detail {
            return;
        }
        let item_type = payload.get("type").and_then(Value::as_str).unwrap_or("");
        let name = nonempty_string(payload.get("name")).unwrap_or_else(|| match item_type {
            "web_search_call" => "web_search".to_string(),
            "tool_search_call" => "tool_search".to_string(),
            "image_generation_call" => "image_generation".to_string(),
            _ => "tool".to_string(),
        });
        let call_id = nonempty_string(payload.get("call_id"))
            .or_else(|| nonempty_string(payload.get("id")))
            .unwrap_or_else(|| stable_message_id("tool-call", timestamp, payload));
        self.tool_names.insert(call_id.clone(), name.clone());
        let raw_input = payload
            .get("arguments")
            .or_else(|| payload.get("input"))
            .or_else(|| payload.get("action"))
            .or_else(|| payload.get("query"));
        let input = raw_input.map(tool_input_value);
        self.messages.push(PendingMessage {
            line_no,
            message: chat_message(
                call_id.clone(),
                "assistant",
                timestamp.unwrap_or(0),
                vec![ContentBlock {
                    kind: "tool_use".to_string(),
                    text: None,
                    tool_name: Some(name.clone()),
                    tool_input: input,
                    truncated: false,
                }],
            ),
        });

        if item_type == "image_generation_call" {
            if let Some(output) = image_generation_output(payload) {
                self.messages.push(PendingMessage {
                    line_no,
                    message: chat_message(
                        format!("{call_id}:output"),
                        "assistant",
                        timestamp.unwrap_or(0),
                        vec![clipped_block("tool_result", &output, Some(name))],
                    ),
                });
            }
        }
    }

    fn process_tool_output(&mut self, line_no: usize, payload: &Value, timestamp: Option<i64>) {
        if !self.collect_detail {
            return;
        }
        let call_id = nonempty_string(payload.get("call_id"))
            .unwrap_or_else(|| stable_message_id("tool-output", timestamp, payload));
        let output = value_text(payload.get("output").or_else(|| payload.get("tools")));
        self.messages.push(PendingMessage {
            line_no,
            message: chat_message(
                format!("{call_id}:output"),
                "assistant",
                timestamp.unwrap_or(0),
                vec![clipped_block(
                    "tool_result",
                    &output,
                    self.tool_names.get(&call_id).cloned(),
                )],
            ),
        });
    }

    fn observe_timestamp(&mut self, timestamp: Option<i64>) {
        let Some(timestamp) = timestamp else {
            return;
        };
        self.earliest_timestamp = Some(
            self.earliest_timestamp
                .map_or(timestamp, |current| current.min(timestamp)),
        );
        self.latest_timestamp = Some(
            self.latest_timestamp
                .map_or(timestamp, |current| current.max(timestamp)),
        );
    }

    fn add_model(&mut self, model: String) {
        if !model.is_empty() && !self.models.contains(&model) {
            self.models.push(model);
        }
    }

    fn finish(mut self, path: &Path) -> (ConvFileResult, Vec<ChatMessage>) {
        let primary = self
            .matching_meta
            .clone()
            .or_else(|| self.meta.first().cloned())
            .unwrap_or_default();
        let session_id = if primary.id.is_empty() {
            session_id_from_stem(&self.file_stem)
        } else {
            primary.id.clone()
        };
        let project = primary.cwd.clone().or(self.fallback_project.clone());
        let started_at = primary.timestamp.or(self.earliest_timestamp).unwrap_or(0);
        let ended_at = self.latest_timestamp.unwrap_or(started_at).max(started_at);

        // Rollouts can switch formats mid-file. Once event prompts begin they are canonical,
        // while distinct legacy prompts from the earlier segment still belong to the session.
        let first_event_prompt_line = self.event_prompts.iter().map(|prompt| prompt.line_no).min();
        let mut selected_prompts = self.event_prompts;
        if let Some(first_event_prompt_line) = first_event_prompt_line {
            for legacy in self.legacy_prompts {
                if legacy.line_no > first_event_prompt_line {
                    continue;
                }
                let is_transition_duplicate = selected_prompts.iter().any(|event| {
                    event.line_no >= first_event_prompt_line
                        && legacy.prompt.text.trim() == event.prompt.text.trim()
                        && legacy.prompt.timestamp.abs_diff(event.prompt.timestamp)
                            <= PROMPT_TRANSITION_DEDUP_WINDOW_MS
                });
                if !is_transition_duplicate {
                    selected_prompts.push(legacy);
                }
            }
        } else {
            selected_prompts = self.legacy_prompts;
        }
        selected_prompts.sort_by_key(|pending| pending.line_no);
        for pending in &mut selected_prompts {
            if pending.prompt.project.is_empty() {
                pending.prompt.project = project.clone().unwrap_or_default();
            }
            if pending.prompt.timestamp == 0 {
                pending.prompt.timestamp = started_at;
            }
            pending.prompt.session_id = Some(session_id.clone());
        }
        let selected_prompt_lines: HashSet<usize> = selected_prompts
            .iter()
            .map(|pending| pending.line_no)
            .collect();
        let first_prompt = selected_prompts
            .first()
            .map(|pending| pending.prompt.text.clone())
            .unwrap_or_default();
        let user_prompts: Vec<RawPrompt> = selected_prompts
            .into_iter()
            .map(|pending| pending.prompt)
            .collect();

        let event_assistant_count = self
            .event_assistant_texts
            .iter()
            .filter(|hash| !self.response_assistant_texts.contains(hash))
            .count();
        let message_count = user_prompts
            .len()
            .saturating_add(self.response_assistant_count)
            .saturating_add(event_assistant_count);

        let mut detail_messages = Vec::new();
        if self.collect_detail {
            detail_messages.append(&mut self.messages);
            detail_messages.extend(
                self.event_user_messages
                    .into_iter()
                    .chain(self.legacy_user_messages)
                    .filter(|pending| selected_prompt_lines.contains(&pending.line_no)),
            );
            for (hash, message) in self.event_assistant_messages {
                if !self.response_assistant_texts.contains(&hash) {
                    detail_messages.push(message);
                }
            }
            detail_messages.sort_by_key(|pending| pending.line_no);
            if primary.is_subagent {
                for pending in &mut detail_messages {
                    pending.message.is_sidechain = true;
                }
            }
        }

        let result = ConvFileResult {
            agent: Agent::Codex,
            path: path.to_path_buf(),
            session_id,
            project,
            git_branch: primary.git_branch.clone(),
            version: primary.cli_version.clone(),
            source: primary.source.clone(),
            models: self.models,
            is_subagent: primary.is_subagent,
            started_at,
            ended_at,
            message_count,
            first_prompt,
            user_prompts,
            usage_entries: self.usage_entries,
        };
        (
            result,
            detail_messages
                .into_iter()
                .map(|pending| pending.message)
                .collect(),
        )
    }
}

/// Parse Codex `history.jsonl`. Cwd is intentionally blank until the indexer joins session ids
/// against primary rollout metadata.
pub fn parse_history(path: &Path) -> Vec<RawPrompt> {
    let mut prompts = Vec::new();
    let _ = for_each_jsonl_line(path, |_, line| {
        if line.is_empty() {
            return;
        }
        let value: Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(_) => return,
        };
        let text = match nonempty_string(value.get("text")) {
            Some(text) => text.trim().to_string(),
            None => return,
        };
        let timestamp = match timestamp_ms(value.get("ts")) {
            Some(timestamp) => timestamp,
            None => return,
        };
        prompts.push(RawPrompt {
            agent: Agent::Codex,
            text,
            project: String::new(),
            timestamp,
            session_id: nonempty_string(value.get("session_id")),
            git_branch: None,
            pasted_count: 0,
            from_history: true,
        });
    });
    prompts
}

/// Parse one Codex rollout into the file-level cache/index representation.
pub fn parse_rollout_file(path: &Path) -> Option<ConvFileResult> {
    let mut accumulator = RolloutAccumulator::new(path, false)?;
    for_each_jsonl_line(path, |line_no, line| {
        accumulator.process_line(line_no, line)
    })
    .ok()?;
    Some(accumulator.finish(path).0)
}

/// Parse one Codex rollout into a normalized conversation detail.
pub fn parse_rollout_detail(path: &Path) -> Option<ConversationDetail> {
    let mut accumulator = RolloutAccumulator::new(path, true)?;
    for_each_jsonl_line(path, |line_no, line| {
        accumulator.process_line(line_no, line)
    })
    .ok()?;
    let (result, messages) = accumulator.finish(path);
    Some(ConversationDetail {
        agent: Agent::Codex,
        session_id: result.session_id,
        project: result.project.unwrap_or_default(),
        git_branch: result.git_branch,
        started_at: result.started_at,
        ended_at: result.ended_at,
        cli_version: result.version,
        source: result.source,
        models: result.models,
        messages,
        usage: SessionUsage::default(),
    })
}

fn raw_prompt(text: String, project: Option<String>, timestamp: i64) -> RawPrompt {
    RawPrompt {
        agent: Agent::Codex,
        text,
        project: project.unwrap_or_default(),
        timestamp,
        session_id: None,
        git_branch: None,
        pasted_count: 0,
        from_history: false,
    }
}

fn chat_message(
    uuid: String,
    role: &str,
    timestamp: i64,
    blocks: Vec<ContentBlock>,
) -> ChatMessage {
    ChatMessage {
        agent: Agent::Codex,
        uuid,
        role: role.to_string(),
        timestamp,
        is_sidechain: false,
        blocks,
    }
}

fn text_block(text: &str) -> ContentBlock {
    clipped_block("text", text, None)
}

fn clipped_block(kind: &str, text: &str, tool_name: Option<String>) -> ContentBlock {
    let (text, truncated) = clip(text, MAX_BLOCK_CHARS);
    ContentBlock {
        kind: kind.to_string(),
        text: Some(text),
        tool_name,
        tool_input: None,
        truncated,
    }
}

fn image_block() -> ContentBlock {
    ContentBlock {
        kind: "image".to_string(),
        text: None,
        tool_name: None,
        tool_input: None,
        truncated: false,
    }
}

fn response_message_blocks(content: Option<&Value>) -> Vec<ContentBlock> {
    match content {
        Some(Value::String(text)) if !text.trim().is_empty() => {
            vec![text_block(text.trim())]
        }
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|item| {
                let item_type = item.get("type").and_then(Value::as_str).unwrap_or("");
                if !matches!(item_type, "output_text" | "text") {
                    return None;
                }
                nonempty_string(item.get("text")).map(|text| text_block(&text))
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn message_content_text(content: Option<&Value>, accepted_types: &[&str]) -> String {
    match content {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(items)) => items
            .iter()
            .filter(|item| {
                item.get("type")
                    .and_then(Value::as_str)
                    .is_some_and(|kind| accepted_types.contains(&kind))
            })
            .filter_map(|item| item.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn legacy_user_content(content: Option<&Value>) -> Option<String> {
    let parts: Vec<String> = match content {
        Some(Value::String(text)) => sanitize_legacy_user_text(text).into_iter().collect(),
        Some(Value::Array(items)) => items
            .iter()
            .filter(|item| {
                matches!(
                    item.get("type").and_then(Value::as_str),
                    Some("input_text" | "text")
                )
            })
            .filter_map(|item| item.get("text").and_then(Value::as_str))
            .filter_map(sanitize_legacy_user_text)
            .collect(),
        _ => Vec::new(),
    };
    let text = parts.join("\n");
    (!text.trim().is_empty()).then_some(text)
}

fn sanitize_legacy_user_text(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.contains("# agents.md instructions")
        || lower.starts_with("<system")
        || lower.starts_with("<developer")
        || lower.starts_with("<instructions>")
    {
        return None;
    }

    let mut cleaned = trimmed.to_string();
    for tag in [
        "environment_context",
        "codex_internal_context",
        "permissions",
        "skills_instructions",
        "apps_instructions",
        "plugins_instructions",
        "instructions",
        "system-reminder",
    ] {
        cleaned = strip_tag_blocks(&cleaned, tag);
    }
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned.to_string())
    }
}

fn strip_tag_blocks(input: &str, tag: &str) -> String {
    let open_prefix = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut output = String::new();
    let mut rest = input;
    loop {
        match rest.find(&open_prefix) {
            Some(start) => match rest[start..].find(&close) {
                Some(close_relative) => {
                    output.push_str(&rest[..start]);
                    rest = &rest[start + close_relative + close.len()..];
                }
                None => {
                    // An unterminated known wrapper is still injected context.
                    output.push_str(&rest[..start]);
                    break;
                }
            },
            None => {
                output.push_str(rest);
                break;
            }
        }
    }
    output
}

fn stable_message_id(kind: &str, timestamp: Option<i64>, payload: &Value) -> String {
    if let Some(id) =
        nonempty_string(payload.get("id")).or_else(|| nonempty_string(payload.get("call_id")))
    {
        return id;
    }
    let timestamp = timestamp.unwrap_or(0).to_string();
    let payload = serde_json::to_string(payload).unwrap_or_default();
    format!(
        "codex:{kind}:{:016x}",
        stable_hash(&[kind, &timestamp, &payload])
    )
}

fn text_fingerprint(text: &str) -> u64 {
    stable_hash(&[text.trim()])
}

fn timestamp_ms(value: Option<&Value>) -> Option<i64> {
    match value? {
        Value::String(value) => chrono::DateTime::parse_from_rfc3339(value)
            .ok()
            .map(|timestamp| timestamp.timestamp_millis())
            .or_else(|| value.parse::<i64>().ok().and_then(normalize_epoch)),
        Value::Number(value) => value
            .as_i64()
            .and_then(normalize_epoch)
            .or_else(|| {
                value
                    .as_u64()
                    .and_then(|value| i64::try_from(value).ok())
                    .and_then(normalize_epoch)
            })
            .or_else(|| {
                value
                    .as_f64()
                    .and_then(|value| normalize_epoch(value as i64))
            }),
        _ => None,
    }
}

fn normalize_epoch(value: i64) -> Option<i64> {
    if value.unsigned_abs() < 100_000_000_000 {
        value.checked_mul(1_000)
    } else {
        Some(value)
    }
}

fn scalar_identity(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(value)) => value.clone(),
        Some(value) => value.to_string(),
        None => String::new(),
    }
}

fn value_u64(value: Option<&Value>) -> u64 {
    value
        .and_then(|value| {
            value
                .as_u64()
                .or_else(|| value.as_i64().and_then(|value| u64::try_from(value).ok()))
        })
        .unwrap_or(0)
}

fn value_text(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(value)) => value.clone(),
        Some(value) => serde_json::to_string_pretty(value).unwrap_or_default(),
        None => String::new(),
    }
}

fn image_generation_output(payload: &Value) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(prompt) = nonempty_string(payload.get("revised_prompt")) {
        parts.push(format!("Revised prompt: {prompt}"));
    }
    if let Some(result) = payload.get("result").filter(|result| !result.is_null()) {
        let result = value_text(Some(result));
        if !result.trim().is_empty() {
            parts.push(format!("Result: {result}"));
        }
    }
    (!parts.is_empty()).then(|| parts.join("\n"))
}

fn tool_input_value(value: &Value) -> Value {
    match value {
        Value::String(value) => {
            serde_json::from_str(value).unwrap_or_else(|_| Value::String(value.clone()))
        }
        value => value.clone(),
    }
}

fn nonempty_string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn value_is_nonempty_array(value: Option<&Value>) -> bool {
    value
        .and_then(Value::as_array)
        .is_some_and(|items| !items.is_empty())
}

fn session_id_from_stem(stem: &str) -> String {
    let suffix = stem.rsplit('-').collect::<Vec<_>>();
    if suffix.len() >= 5 {
        let candidate = suffix[..5]
            .iter()
            .rev()
            .copied()
            .collect::<Vec<_>>()
            .join("-");
        if candidate.len() == 36 {
            return candidate;
        }
    }
    stem.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

    struct TestFile(PathBuf);

    impl TestFile {
        fn new(stem: &str, contents: &str) -> Self {
            let serial = NEXT_FILE.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!(
                "cc-history-codex-parser-{}-{serial}",
                std::process::id()
            ));
            fs::create_dir_all(&dir).unwrap();
            let path = dir.join(format!("{stem}.jsonl"));
            fs::write(&path, contents).unwrap();
            Self(path)
        }
    }

    impl Drop for TestFile {
        fn drop(&mut self) {
            if let Some(parent) = self.0.parent() {
                let _ = fs::remove_dir_all(parent);
            }
        }
    }

    fn repository_fixture(relative: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/codex")
            .join(relative)
    }

    #[test]
    fn history_streams_skips_bad_lines_and_normalizes_seconds_and_millis() {
        let file = TestFile::new(
            "history",
            concat!(
                "{\"session_id\":\"s1\",\"text\":\"first\",\"ts\":1700000000}\n",
                "not json\n",
                "{\"session_id\":\"s2\",\"text\":\"second\",\"ts\":1700000000123}\n",
                "{\"session_id\":\"s3\",\"text\":\"  \" ,\"ts\":1700000001}\n"
            ),
        );
        let prompts = parse_history(&file.0);
        assert_eq!(prompts.len(), 2);
        assert!(prompts.iter().all(|prompt| prompt.agent == Agent::Codex));
        assert!(prompts.iter().all(|prompt| prompt.project.is_empty()));
        assert_eq!(prompts[0].timestamp, 1_700_000_000_000);
        assert_eq!(prompts[1].timestamp, 1_700_000_000_123);
    }

    #[test]
    fn current_rollout_uses_event_prompt_last_usage_and_model_switches() {
        const ID: &str = "11111111-2222-3333-4444-555555555555";
        let file = TestFile::new(
            &format!("rollout-2026-01-02T00-00-00-{ID}"),
            &format!(
                concat!(
                    "{{\"timestamp\":\"2026-01-02T00:00:00.000Z\",\"type\":\"session_meta\",\"payload\":{{\"id\":\"{id}\",\"cwd\":\"/synthetic/main\",\"cli_version\":\"1.2.3\",\"source\":\"cli\",\"timestamp\":\"2026-01-02T00:00:00.000Z\"}}}}\n",
                    "{{\"timestamp\":\"2026-01-02T00:00:01.000Z\",\"type\":\"turn_context\",\"payload\":{{\"turn_id\":\"turn-1\",\"cwd\":\"/synthetic/main\",\"model\":\"gpt-codex-a\"}}}}\n",
                    "{{\"timestamp\":\"2026-01-02T00:00:02.000Z\",\"type\":\"event_msg\",\"payload\":{{\"type\":\"user_message\",\"message\":\"real prompt\"}}}}\n",
                    "{{\"timestamp\":\"2026-01-02T00:00:02.100Z\",\"type\":\"response_item\",\"payload\":{{\"type\":\"message\",\"role\":\"user\",\"content\":[{{\"type\":\"input_text\",\"text\":\"duplicate fallback\"}}]}}}}\n",
                    "{{\"timestamp\":\"2026-01-02T00:00:03.000Z\",\"type\":\"event_msg\",\"payload\":{{\"type\":\"token_count\",\"info\":{{\"last_token_usage\":{{\"input_tokens\":100,\"cached_input_tokens\":40,\"output_tokens\":20,\"reasoning_output_tokens\":5,\"total_tokens\":120}},\"total_token_usage\":{{\"input_tokens\":999999,\"output_tokens\":999999}}}}}}}}\n",
                    "bad json\n",
                    "{{\"timestamp\":\"2026-01-02T00:00:04.000Z\",\"type\":\"turn_context\",\"payload\":{{\"turn_id\":\"turn-2\",\"cwd\":\"/synthetic/main\",\"model\":\"gpt-codex-b\"}}}}\n",
                    "{{\"timestamp\":\"2026-01-02T00:00:05.000Z\",\"type\":\"event_msg\",\"payload\":{{\"type\":\"token_count\",\"info\":{{\"last_token_usage\":{{\"input_tokens\":10,\"cached_input_tokens\":2,\"output_tokens\":4,\"reasoning_output_tokens\":1}}}}}}}}\n",
                    "{{\"timestamp\":\"2026-01-02T00:00:06.000Z\",\"type\":\"unknown\",\"payload\":{{\"private\":\"ignored\"}}}}\n"
                ),
                id = ID
            ),
        );
        let result = parse_rollout_file(&file.0).unwrap();
        assert_eq!(result.agent, Agent::Codex);
        assert_eq!(result.session_id, ID);
        assert_eq!(result.project.as_deref(), Some("/synthetic/main"));
        assert_eq!(result.version.as_deref(), Some("1.2.3"));
        assert_eq!(result.models, vec!["gpt-codex-a", "gpt-codex-b"]);
        assert_eq!(result.user_prompts.len(), 1);
        assert_eq!(result.user_prompts[0].text, "real prompt");
        assert_eq!(result.usage_entries.len(), 2);
        assert_eq!(
            result.usage_entries[0].usage,
            NormalizedUsage {
                uncached_input: 60,
                cache_read: 40,
                cache_creation: 0,
                output: 20,
                reasoning_output: 5,
            }
        );
        assert_eq!(result.usage_entries[1].model, "gpt-codex-b");
    }

    #[test]
    fn copied_usage_fingerprint_ignores_file_and_new_session_id() {
        let shared = concat!(
            "{\"timestamp\":\"2026-02-01T00:00:01.000123Z\",\"type\":\"turn_context\",\"payload\":{\"turn_id\":\"original-turn\",\"cwd\":\"/synthetic/original\",\"model\":\"gpt-codex\"}}\n",
            "{\"timestamp\":\"2026-02-01T00:00:02.000456Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\",\"info\":{\"last_token_usage\":{\"input_tokens\":50,\"cached_input_tokens\":10,\"output_tokens\":8,\"reasoning_output_tokens\":2}}}}\n"
        );
        let first_id = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
        let second_id = "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb";
        let first = TestFile::new(
            &format!("rollout-2026-02-01T00-00-00-{first_id}"),
            &format!("{{\"timestamp\":\"2026-02-01T00:00:00Z\",\"type\":\"session_meta\",\"payload\":{{\"id\":\"{first_id}\",\"cwd\":\"/synthetic/one\",\"source\":\"cli\"}}}}\n{shared}"),
        );
        let second = TestFile::new(
            &format!("rollout-2026-02-01T01-00-00-{second_id}"),
            &format!("{{\"timestamp\":\"2026-02-01T01:00:00Z\",\"type\":\"session_meta\",\"payload\":{{\"id\":\"{second_id}\",\"cwd\":\"/synthetic/two\",\"source\":\"cli\"}}}}\n{shared}"),
        );
        let first_key = &parse_rollout_file(&first.0).unwrap().usage_entries[0].dedup_key;
        let second_key = &parse_rollout_file(&second.0).unwrap().usage_entries[0].dedup_key;
        assert_eq!(first_key, second_key);
    }

    #[test]
    fn matching_primary_meta_beats_copied_parent_and_marks_subagent() {
        const CHILD: &str = "cccccccc-cccc-cccc-cccc-cccccccccccc";
        let file = TestFile::new(
            &format!("rollout-2026-03-01T00-00-00-{CHILD}"),
            &format!(
                concat!(
                    "{{\"timestamp\":\"2026-03-01T00:00:00Z\",\"type\":\"session_meta\",\"payload\":{{\"id\":\"{child}\",\"cwd\":\"/synthetic/child\",\"cli_version\":\"2.0.0\",\"source\":{{\"subagent\":{{\"thread_spawn\":{{\"depth\":1}}}}}}}}}}\n",
                    "{{\"timestamp\":\"2026-01-01T00:00:00Z\",\"type\":\"session_meta\",\"payload\":{{\"id\":\"dddddddd-dddd-dddd-dddd-dddddddddddd\",\"cwd\":\"/synthetic/parent\",\"cli_version\":\"old\",\"source\":\"cli\"}}}}\n"
                ),
                child = CHILD
            ),
        );
        let result = parse_rollout_file(&file.0).unwrap();
        assert_eq!(result.session_id, CHILD);
        assert_eq!(result.project.as_deref(), Some("/synthetic/child"));
        assert_eq!(result.version.as_deref(), Some("2.0.0"));
        assert_eq!(result.source.as_deref(), Some("subagent"));
        assert!(result.is_subagent);
        assert_eq!(result.started_at, 1_772_323_200_000);
    }

    #[test]
    fn string_subagent_source_is_classified() {
        const ID: &str = "abababab-abab-abab-abab-abababababab";
        let file = TestFile::new(
            &format!("rollout-2026-03-02T00-00-00-{ID}"),
            &format!(
                "{{\"timestamp\":\"2026-03-02T00:00:00Z\",\"type\":\"session_meta\",\"payload\":{{\"id\":\"{ID}\",\"cwd\":\"/synthetic/string-subagent\",\"source\":\"subagent\"}}}}\n"
            ),
        );
        let result = parse_rollout_file(&file.0).unwrap();
        assert!(result.is_subagent);
        assert_eq!(result.source.as_deref(), Some("subagent"));
    }

    #[test]
    fn legacy_fallback_rejects_injections_and_detail_keeps_tools() {
        const ID: &str = "eeeeeeee-eeee-eeee-eeee-eeeeeeeeeeee";
        let file = TestFile::new(
            &format!("rollout-2025-01-01T00-00-00-{ID}"),
            &format!(
                concat!(
                    "{{\"timestamp\":\"2025-01-01T00:00:00Z\",\"type\":\"session_meta\",\"payload\":{{\"id\":\"{id}\",\"cwd\":\"/synthetic/legacy\",\"source\":\"exec\"}}}}\n",
                    "{{\"timestamp\":\"2025-01-01T00:00:01Z\",\"type\":\"response_item\",\"payload\":{{\"type\":\"message\",\"role\":\"developer\",\"content\":[{{\"type\":\"input_text\",\"text\":\"developer injection\"}}]}}}}\n",
                    "{{\"timestamp\":\"2025-01-01T00:00:02Z\",\"type\":\"response_item\",\"payload\":{{\"type\":\"message\",\"role\":\"user\",\"content\":[{{\"type\":\"input_text\",\"text\":\"# AGENTS.md instructions\\nnoise\"}}]}}}}\n",
                    "{{\"timestamp\":\"2025-01-01T00:00:03Z\",\"type\":\"response_item\",\"payload\":{{\"type\":\"message\",\"role\":\"user\",\"content\":[{{\"type\":\"input_text\",\"text\":\"<environment_context>noise</environment_context>\"}}]}}}}\n",
                    "{{\"timestamp\":\"2025-01-01T00:00:04Z\",\"type\":\"response_item\",\"payload\":{{\"type\":\"message\",\"role\":\"user\",\"content\":[{{\"type\":\"input_text\",\"text\":\"legacy real prompt\"}}]}}}}\n",
                    "{{\"timestamp\":\"2025-01-01T00:00:05Z\",\"type\":\"response_item\",\"payload\":{{\"type\":\"function_call\",\"call_id\":\"call-1\",\"name\":\"shell\",\"arguments\":\"{{\\\"cmd\\\":\\\"pwd\\\"}}\"}}}}\n",
                    "{{\"timestamp\":\"2025-01-01T00:00:06Z\",\"type\":\"response_item\",\"payload\":{{\"type\":\"function_call_output\",\"call_id\":\"call-1\",\"output\":\"ok\"}}}}\n",
                    "{{\"timestamp\":\"2025-01-01T00:00:07Z\",\"type\":\"response_item\",\"payload\":{{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{{\"type\":\"output_text\",\"text\":\"done\"}}]}}}}\n"
                ),
                id = ID
            ),
        );
        let result = parse_rollout_file(&file.0).unwrap();
        assert_eq!(result.user_prompts.len(), 1);
        assert_eq!(result.user_prompts[0].text, "legacy real prompt");

        let detail = parse_rollout_detail(&file.0).unwrap();
        assert_eq!(detail.agent, Agent::Codex);
        assert!(detail.messages.iter().any(|message| {
            message.blocks.iter().any(|block| {
                block.kind == "tool_use" && block.tool_name.as_deref() == Some("shell")
            })
        }));
        assert!(detail.messages.iter().any(|message| {
            message
                .blocks
                .iter()
                .any(|block| block.kind == "tool_result" && block.text.as_deref() == Some("ok"))
        }));
        assert!(!detail.messages.iter().any(|message| {
            message.blocks.iter().any(|block| {
                block
                    .text
                    .as_deref()
                    .is_some_and(|text| text.contains("injection") || text.contains("AGENTS"))
            })
        }));
    }

    #[test]
    fn repository_current_fixture_is_a_parser_golden() {
        let path = repository_fixture("sessions/2026/07/17/rollout-current.jsonl");
        let result = parse_rollout_file(&path).unwrap();
        assert_eq!(result.session_id, "codex-current-0001");
        assert_eq!(result.project.as_deref(), Some("/synthetic/project-alpha"));
        assert_eq!(result.models, vec!["gpt-5.4", "gpt-5.4-mini"]);
        assert_eq!(
            result
                .user_prompts
                .iter()
                .map(|prompt| prompt.text.as_str())
                .collect::<Vec<_>>(),
            vec!["Synthetic current request", "Synthetic follow-up request"]
        );
        assert_eq!(result.usage_entries.len(), 2);
        assert_eq!(
            result.usage_entries[0].usage,
            NormalizedUsage {
                uncached_input: 600,
                cache_read: 400,
                cache_creation: 0,
                output: 200,
                reasoning_output: 50,
            }
        );
        assert_eq!(
            result.usage_entries[0].usage.total_tokens_including_cache(),
            1_200
        );

        let detail = parse_rollout_detail(&path).unwrap();
        let tool_names: HashSet<&str> = detail
            .messages
            .iter()
            .flat_map(|message| &message.blocks)
            .filter(|block| block.kind == "tool_use")
            .filter_map(|block| block.tool_name.as_deref())
            .collect();
        assert!(tool_names.contains("exec_command"));
        assert!(tool_names.contains("apply_patch"));
        assert!(detail.messages.iter().any(|message| {
            message.blocks.iter().any(|block| {
                block.text.as_deref() == Some("Synthetic answer after the damaged line.")
            })
        }));
    }

    #[test]
    fn repository_legacy_subagent_archive_and_fork_fixtures_are_covered() {
        let legacy = parse_rollout_file(&repository_fixture(
            "sessions/2026/07/16/rollout-legacy.jsonl",
        ))
        .unwrap();
        assert_eq!(legacy.user_prompts.len(), 1);
        assert_eq!(legacy.user_prompts[0].text, "Legacy synthetic request.");

        let subagent = parse_rollout_file(&repository_fixture(
            "sessions/2026/07/14/rollout-subagent.jsonl",
        ))
        .unwrap();
        assert!(subagent.is_subagent);
        assert_eq!(subagent.usage_entries.len(), 1);
        assert_eq!(
            subagent.usage_entries[0].project,
            "/synthetic/project-subagent"
        );

        let archived = parse_rollout_file(&repository_fixture(
            "archived_sessions/synthetic-archived.jsonl",
        ))
        .unwrap();
        assert_eq!(archived.session_id, "codex-archived-0001");

        let original = parse_rollout_file(&repository_fixture(
            "sessions/2026/07/15/rollout-fork-original.jsonl",
        ))
        .unwrap();
        let copy = parse_rollout_file(&repository_fixture(
            "sessions/2026/07/15/rollout-fork-copy.jsonl",
        ))
        .unwrap();
        let distinct = parse_rollout_file(&repository_fixture(
            "sessions/2026/07/15/rollout-distinct-same-usage.jsonl",
        ))
        .unwrap();
        assert_eq!(
            original.usage_entries[0].dedup_key,
            copy.usage_entries[0].dedup_key
        );
        assert_ne!(
            original.usage_entries[0].dedup_key,
            distinct.usage_entries[0].dedup_key
        );
    }
}
