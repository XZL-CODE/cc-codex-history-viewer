//! 全文搜索会话内容：按需并行流式扫描会话文件，不落任何索引。
//!
//! 覆盖用户消息、助手回复、思考摘要与工具调用参数；工具结果正文（文件内容、命令输出）
//! 不参与匹配。每行先用大小写不敏感的字节子串做预筛，只有命中的行才解析 JSON。

use crate::codex_parser;
use crate::indexer::{self, AppIndex};
use crate::models::{Agent, AgentFilter, ConversationHit, ConversationSearchResponse};
use crate::parser;
use rayon::prelude::*;
use serde_json::Value;
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Instant;

pub const DEFAULT_LIMIT: usize = 300;
const PER_SESSION_LIMIT: usize = 20;
const SNIPPET_BEFORE: usize = 60;
const SNIPPET_AFTER: usize = 160;

/// One session file to scan, carrying the session metadata shown with each hit.
#[derive(Debug, Clone)]
pub struct SearchTarget {
    pub agent: Agent,
    pub session_id: String,
    pub project: String,
    pub title: String,
    pub started_at: i64,
    pub path: PathBuf,
}

struct Candidate {
    role: &'static str,
    kind: &'static str,
    uuid: String,
    timestamp: i64,
    tool_name: Option<String>,
    text: String,
}

struct Query {
    /// Unicode-folded tokens for exact matching after JSON parsing.
    folded: Vec<Vec<char>>,
    /// ASCII-lowercased raw tokens for the byte-level prefilter on raw lines.
    prefilter: Vec<Vec<u8>>,
}

impl Query {
    fn parse(query: &str) -> Option<Self> {
        let mut folded = Vec::new();
        let mut prefilter = Vec::new();
        for token in query.split_whitespace() {
            let chars: Vec<char> = token.chars().map(indexer::fold_char).collect();
            if chars.is_empty() {
                continue;
            }
            prefilter.push(token.to_ascii_lowercase().into_bytes());
            folded.push(chars);
        }
        (!folded.is_empty()).then_some(Self { folded, prefilter })
    }
}

/// Search every non-sub-agent session in the index that passes the agent and project filters.
pub fn search_index(
    index: &AppIndex,
    query: &str,
    filter: AgentFilter,
    project: Option<&str>,
    limit: Option<usize>,
) -> ConversationSearchResponse {
    let targets: Vec<SearchTarget> = index
        .sessions
        .iter()
        .filter(|session| filter.includes(session.agent))
        .filter(|session| project.map_or(true, |project| session.project == project))
        .filter_map(|session| {
            let path = index
                .session_files
                .get(&(session.agent, session.session_id.clone()))?;
            Some(SearchTarget {
                agent: session.agent,
                session_id: session.session_id.clone(),
                project: session.project.clone(),
                title: session.title.clone(),
                started_at: session.started_at,
                path: PathBuf::from(path),
            })
        })
        .collect();
    search_targets(&targets, query, limit.unwrap_or(DEFAULT_LIMIT))
}

pub fn search_targets(
    targets: &[SearchTarget],
    query: &str,
    limit: usize,
) -> ConversationSearchResponse {
    let started = Instant::now();
    let Some(query) = Query::parse(query) else {
        return ConversationSearchResponse {
            hits: Vec::new(),
            scanned_files: 0,
            matched_sessions: 0,
            truncated: false,
            elapsed_ms: 0,
        };
    };
    let per_file: Vec<(Vec<ConversationHit>, bool)> = targets
        .par_iter()
        .map(|target| scan_file(target, &query))
        .collect();

    let mut hits = Vec::new();
    let mut truncated = false;
    let mut matched_sessions = 0usize;
    for (file_hits, capped) in per_file {
        if !file_hits.is_empty() {
            matched_sessions += 1;
        }
        truncated |= capped;
        hits.extend(file_hits);
    }
    hits.sort_by(|left, right| {
        right
            .timestamp
            .cmp(&left.timestamp)
            .then_with(|| left.session_id.cmp(&right.session_id))
            .then_with(|| left.message_uuid.cmp(&right.message_uuid))
    });
    if hits.len() > limit {
        hits.truncate(limit);
        truncated = true;
    }
    ConversationSearchResponse {
        hits,
        scanned_files: targets.len(),
        matched_sessions,
        truncated,
        elapsed_ms: started.elapsed().as_millis() as u64,
    }
}

fn scan_file(target: &SearchTarget, query: &Query) -> (Vec<ConversationHit>, bool) {
    let mut hits = Vec::new();
    let mut capped = false;
    // Codex writes some assistant messages twice (event + response item); keep the first copy.
    let mut seen: HashSet<u64> = HashSet::new();
    let _ = parser::for_each_jsonl_line_while(&target.path, |_, line| {
        if line.is_empty()
            || !query
                .prefilter
                .iter()
                .all(|token| contains_ci(line.as_bytes(), token))
        {
            return true;
        }
        let value: Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(_) => return true,
        };
        let candidates = match target.agent {
            Agent::Claude => claude_candidates(&value),
            Agent::Codex => codex_candidates(&value),
        };
        for candidate in candidates {
            if !seen.insert(parser::stable_hash(&[candidate.kind, &candidate.text])) {
                continue;
            }
            if let Some(hit) = match_candidate(target, candidate, query) {
                hits.push(hit);
                if hits.len() >= PER_SESSION_LIMIT {
                    capped = true;
                    return false;
                }
            }
        }
        true
    });
    (hits, capped)
}

/// ASCII-case-insensitive byte substring test; `needle` must already be ASCII-lowercased.
fn contains_ci(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    if needle.len() > haystack.len() {
        return false;
    }
    let first = needle[0];
    let first_upper = first.to_ascii_uppercase();
    let last_start = haystack.len() - needle.len();
    let mut index = 0usize;
    while index <= last_start {
        let byte = haystack[index];
        if (byte == first || byte == first_upper)
            && haystack[index..index + needle.len()]
                .iter()
                .zip(needle)
                .all(|(candidate, wanted)| candidate.to_ascii_lowercase() == *wanted)
        {
            return true;
        }
        index += 1;
    }
    false
}

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn match_candidate(
    target: &SearchTarget,
    candidate: Candidate,
    query: &Query,
) -> Option<ConversationHit> {
    let folded: Vec<char> = candidate.text.chars().map(indexer::fold_char).collect();
    let mut anchor = usize::MAX;
    for token in &query.folded {
        let first = *indexer::find_all(&folded, token).first()?;
        anchor = anchor.min(first);
    }
    let chars: Vec<char> = candidate.text.chars().collect();
    let start = anchor.saturating_sub(SNIPPET_BEFORE);
    let end = (anchor + SNIPPET_AFTER).min(chars.len());
    let mut snippet = collapse_whitespace(&chars[start..end].iter().collect::<String>());
    if start > 0 {
        snippet.insert(0, '…');
    }
    if end < chars.len() {
        snippet.push('…');
    }
    let folded_snippet: Vec<char> = snippet.chars().map(indexer::fold_char).collect();
    let mut ranges = Vec::new();
    for token in &query.folded {
        for position in indexer::find_all(&folded_snippet, token) {
            ranges.push([position, position + token.len()]);
        }
    }
    Some(ConversationHit {
        agent: target.agent,
        session_id: target.session_id.clone(),
        project: target.project.clone(),
        session_title: target.title.clone(),
        session_started_at: target.started_at,
        message_uuid: candidate.uuid,
        timestamp: candidate.timestamp,
        role: candidate.role.to_string(),
        kind: candidate.kind.to_string(),
        tool_name: candidate.tool_name,
        snippet,
        match_ranges: indexer::merge_ranges(ranges),
    })
}

fn text_of(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

// ----------------------------- Claude -----------------------------

fn claude_candidates(value: &Value) -> Vec<Candidate> {
    let line_type = value.get("type").and_then(Value::as_str).unwrap_or("");
    if line_type != "user" && line_type != "assistant" {
        return Vec::new();
    }
    let Some(message) = value.get("message") else {
        return Vec::new();
    };
    let uuid = value
        .get("uuid")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let timestamp = value
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(parser::iso_to_ms)
        .unwrap_or(0);
    let role = message
        .get("role")
        .and_then(Value::as_str)
        .unwrap_or(line_type);
    let Some(content) = message.get("content") else {
        return Vec::new();
    };
    let make = |kind: &'static str, tool_name: Option<String>, text: String| Candidate {
        role: if role == "user" { "user" } else { "assistant" },
        kind,
        uuid: uuid.clone(),
        timestamp,
        tool_name,
        text,
    };

    if role == "user" {
        // Same cleaning as prompt extraction: strips injected wrappers, drops tool-result-only rows.
        return parser::extract_prompt_text(content)
            .map(|text| vec![make("text", None, text)])
            .unwrap_or_default();
    }

    let mut out = Vec::new();
    match content {
        Value::String(text) => {
            if let Some(text) = text_of(Some(&Value::String(text.clone()))) {
                out.push(make("text", None, text));
            }
        }
        Value::Array(blocks) => {
            for block in blocks {
                match block.get("type").and_then(Value::as_str).unwrap_or("") {
                    "text" => {
                        if let Some(text) = text_of(block.get("text")) {
                            out.push(make("text", None, text));
                        }
                    }
                    "thinking" => {
                        if let Some(text) = text_of(block.get("thinking")) {
                            out.push(make("thinking", None, text));
                        }
                    }
                    "tool_use" => {
                        let name = text_of(block.get("name")).unwrap_or_else(|| "tool".to_string());
                        let input = block
                            .get("input")
                            .map(|input| match input {
                                Value::String(text) => text.clone(),
                                other => serde_json::to_string(other).unwrap_or_default(),
                            })
                            .unwrap_or_default();
                        out.push(make(
                            "tool_use",
                            Some(name.clone()),
                            format!("{name} {input}"),
                        ));
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }
    out
}

// ----------------------------- Codex -----------------------------

fn codex_candidates(value: &Value) -> Vec<Candidate> {
    let record_type = value.get("type").and_then(Value::as_str).unwrap_or("");
    let raw_timestamp = codex_parser::timestamp_ms(value.get("timestamp"));
    let timestamp = raw_timestamp.unwrap_or(0);
    let Some(payload) = value.get("payload").filter(|payload| payload.is_object()) else {
        return Vec::new();
    };
    let payload_type = payload.get("type").and_then(Value::as_str).unwrap_or("");
    let id = |kind: &str| codex_parser::stable_message_id(kind, raw_timestamp, payload);
    let mut out = Vec::new();

    match (record_type, payload_type) {
        ("event_msg", "user_message") => {
            if let Some(text) = codex_parser::nonempty_string(payload.get("message")) {
                out.push(Candidate {
                    role: "user",
                    kind: "text",
                    uuid: id("event-user"),
                    timestamp,
                    tool_name: None,
                    text,
                });
            }
        }
        ("event_msg", "agent_message") => {
            if let Some(text) = codex_parser::nonempty_string(payload.get("message")) {
                out.push(Candidate {
                    role: "assistant",
                    kind: "text",
                    uuid: id("event-assistant"),
                    timestamp,
                    tool_name: None,
                    text,
                });
            }
        }
        ("response_item", "message") => match payload.get("role").and_then(Value::as_str) {
            Some("user") => {
                if let Some(text) = codex_parser::legacy_user_content(payload.get("content")) {
                    out.push(Candidate {
                        role: "user",
                        kind: "text",
                        uuid: id("legacy-user"),
                        timestamp,
                        tool_name: None,
                        text,
                    });
                }
            }
            Some("assistant") => {
                let text = codex_parser::message_content_text(
                    payload.get("content"),
                    &["output_text", "text"],
                );
                if !text.trim().is_empty() {
                    out.push(Candidate {
                        role: "assistant",
                        kind: "text",
                        uuid: id("response-assistant"),
                        timestamp,
                        tool_name: None,
                        text,
                    });
                }
            }
            // developer / system rows are injected context, never conversation content
            _ => {}
        },
        ("response_item", "agent_message") => {
            if let Some(text) = codex_parser::nonempty_string(payload.get("content")) {
                out.push(Candidate {
                    role: "assistant",
                    kind: "text",
                    uuid: id("response-agent"),
                    timestamp,
                    tool_name: None,
                    text,
                });
            }
        }
        ("response_item", "reasoning") => {
            let text = codex_parser::message_content_text(
                payload.get("summary"),
                &["summary_text", "text"],
            );
            if !text.trim().is_empty() {
                out.push(Candidate {
                    role: "assistant",
                    kind: "thinking",
                    uuid: id("reasoning"),
                    timestamp,
                    tool_name: None,
                    text,
                });
            }
        }
        (
            "response_item",
            "function_call"
            | "custom_tool_call"
            | "web_search_call"
            | "tool_search_call"
            | "image_generation_call",
        ) => {
            let name = codex_parser::nonempty_string(payload.get("name")).unwrap_or_else(|| {
                match payload_type {
                    "web_search_call" => "web_search",
                    "tool_search_call" => "tool_search",
                    "image_generation_call" => "image_generation",
                    _ => "tool",
                }
                .to_string()
            });
            let uuid = codex_parser::nonempty_string(payload.get("call_id"))
                .or_else(|| codex_parser::nonempty_string(payload.get("id")))
                .unwrap_or_else(|| id("tool-call"));
            let input = codex_parser::value_text(
                payload
                    .get("arguments")
                    .or_else(|| payload.get("input"))
                    .or_else(|| payload.get("action"))
                    .or_else(|| payload.get("query")),
            );
            out.push(Candidate {
                role: "assistant",
                kind: "tool_use",
                uuid,
                timestamp,
                tool_name: Some(name.clone()),
                text: format!("{name} {input}"),
            });
        }
        _ => {}
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn fixture(relative: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(relative)
    }

    fn target(agent: Agent, relative: &str) -> SearchTarget {
        SearchTarget {
            agent,
            session_id: "synthetic-session".to_string(),
            project: "/synthetic/project".to_string(),
            title: "synthetic title".to_string(),
            started_at: 0,
            path: fixture(relative),
        }
    }

    const CLAUDE_FIXTURE: &str = "f1e2d3c4-aaaa-bbbb-cccc-000000000001.jsonl";
    const CODEX_FIXTURE: &str = "codex/sessions/2026/07/17/rollout-current.jsonl";

    #[test]
    fn claude_search_covers_user_assistant_and_tool_calls_but_not_tool_results() {
        let targets = vec![target(Agent::Claude, CLAUDE_FIXTURE)];
        let response = search_targets(&targets, "重构", 100);
        assert_eq!(response.scanned_files, 1);
        assert_eq!(response.matched_sessions, 1);
        assert!(!response.truncated);
        let snippets: Vec<&str> = response
            .hits
            .iter()
            .map(|hit| hit.snippet.as_str())
            .collect();
        assert!(snippets.iter().any(|s| s.contains("帮我重构 parser 模块")));
        assert!(snippets.iter().any(|s| s.contains("重构完成")));
        assert!(response.hits.iter().all(|hit| hit.kind != "tool_result"));
        for hit in &response.hits {
            let chars: Vec<char> = hit.snippet.chars().collect();
            assert!(!hit.match_ranges.is_empty());
            for [start, end] in &hit.match_ranges {
                assert!(*end <= chars.len());
                assert_eq!(chars[*start..*end].iter().collect::<String>(), "重构");
            }
        }

        let tool = search_targets(&targets, "BASH", 100);
        assert!(tool
            .hits
            .iter()
            .any(|hit| hit.kind == "tool_use" && hit.tool_name.as_deref() == Some("Bash")));

        let multi = search_targets(&targets, "parser 模块", 100);
        assert_eq!(multi.hits.len(), 1);
        assert_eq!(multi.hits[0].role, "user");
        assert_eq!(multi.hits[0].match_ranges.len(), 2);

        assert!(search_targets(&targets, "这个词不存在", 100)
            .hits
            .is_empty());
        let blank = search_targets(&targets, "   ", 100);
        assert!(blank.hits.is_empty());
        assert_eq!(blank.scanned_files, 0);
    }

    #[test]
    fn codex_search_skips_injected_context_and_finds_tool_calls() {
        let targets = vec![target(Agent::Codex, CODEX_FIXTURE)];
        let response = search_targets(&targets, "synthetic", 100);
        let snippets: Vec<&str> = response
            .hits
            .iter()
            .map(|hit| hit.snippet.as_str())
            .collect();
        assert!(snippets
            .iter()
            .any(|s| s.contains("Synthetic current request")));
        assert!(snippets
            .iter()
            .any(|s| s.contains("Synthetic answer after the damaged line")));
        assert!(!snippets
            .iter()
            .any(|s| s.contains("developer instruction") || s.contains("AGENTS")));
        assert!(response.hits.iter().all(|hit| !hit.message_uuid.is_empty()));

        let patch = search_targets(&targets, "apply_patch", 100);
        assert!(patch
            .hits
            .iter()
            .any(|hit| hit.kind == "tool_use" && hit.tool_name.as_deref() == Some("apply_patch")));
    }

    #[test]
    fn limits_prefilter_and_snippets_behave() {
        let targets = vec![target(Agent::Claude, CLAUDE_FIXTURE)];
        let limited = search_targets(&targets, "重构", 1);
        assert_eq!(limited.hits.len(), 1);
        assert!(limited.truncated);

        assert!(contains_ci(b"Hello World", b"o w"));
        assert!(contains_ci(b"ABC", b"abc"));
        assert!(!contains_ci(b"ab", b"abc"));
        assert!(contains_ci("重构 parser".as_bytes(), "重构".as_bytes()));
        assert!(!contains_ci(b"xyz", b"a"));

        let long_text = format!("{}needle{}", "a".repeat(200), "b".repeat(400));
        let candidate = Candidate {
            role: "assistant",
            kind: "text",
            uuid: "u".to_string(),
            timestamp: 1,
            tool_name: None,
            text: long_text,
        };
        let query = Query::parse("NEEDLE").unwrap();
        let hit = match_candidate(&targets[0], candidate, &query).unwrap();
        assert!(hit.snippet.starts_with('…') && hit.snippet.ends_with('…'));
        assert!(hit.snippet.chars().count() < 240);
        let chars: Vec<char> = hit.snippet.chars().collect();
        let [start, end] = hit.match_ranges[0];
        assert_eq!(chars[start..end].iter().collect::<String>(), "needle");
    }
}
