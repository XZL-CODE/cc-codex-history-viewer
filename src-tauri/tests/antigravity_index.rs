//! Antigravity source: Antigravity CLI transcripts and Gemini CLI sessions in a synthetic
//! `~/.gemini` tree shaped after real recordings (all content is made up).

use cc_history_viewer_lib::content_search;
use cc_history_viewer_lib::indexer;
use cc_history_viewer_lib::models::{Agent, AgentFilter, PromptOrigin};
use cc_history_viewer_lib::state::{ClaudeDataPaths, CodexDataPaths, DataPaths, GeminiDataPaths};
use cc_history_viewer_lib::{antigravity_parser, usage_detail};
use serde_json::json;
use std::path::PathBuf;

const MAIN: &str = "11111111-1111-4111-8111-111111111111";
const CHILD: &str = "22222222-2222-4222-8222-222222222222";
const PRINT: &str = "33333333-3333-4333-8333-333333333333";
const GEMINI: &str = "55555555-5555-4555-8555-555555555555";
const LEGACY: &str = "77777777-7777-4777-8777-777777777777";

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/antigravity")
}

fn gemini_only_paths() -> DataPaths {
    let missing = PathBuf::from("/synthetic/nonexistent");
    DataPaths {
        claude: ClaudeDataPaths {
            root: missing.clone(),
            history: missing.join("history.jsonl"),
            projects: missing.join("projects"),
            sessions: missing.join("sessions"),
        },
        codex: CodexDataPaths {
            root: missing.clone(),
            history: missing.join("codex-history.jsonl"),
            sessions: missing.join("codex-sessions"),
            archived_sessions: missing.join("codex-archived"),
        },
        gemini: GeminiDataPaths::under(root()),
    }
}

fn transcript(id: &str, name: &str) -> PathBuf {
    root()
        .join("antigravity-cli/brain")
        .join(id)
        .join(".system_generated/logs")
        .join(name)
}

fn block_summary(detail: &cc_history_viewer_lib::models::ConversationDetail) -> Vec<String> {
    detail
        .messages
        .iter()
        .map(|message| {
            let blocks: Vec<String> = message
                .blocks
                .iter()
                .map(|block| match block.kind.as_str() {
                    "tool_use" | "tool_result" => format!(
                        "{}:{}",
                        block.kind,
                        block.tool_name.as_deref().unwrap_or("?")
                    ),
                    kind => kind.to_string(),
                })
                .collect();
            format!("{} {}", message.role, blocks.join(","))
        })
        .collect()
}

#[test]
fn antigravity_transcript_becomes_a_readable_conversation() {
    let path = transcript(MAIN, "transcript_full.jsonl");
    let detail = antigravity_parser::parse_detail_with_limit(&path, Some(24_000)).unwrap();
    assert_eq!(detail.agent, Agent::Antigravity);
    assert_eq!(detail.session_id, MAIN);
    assert_eq!(detail.source.as_deref(), Some("antigravity-cli"));
    assert_eq!(
        detail.models,
        [
            "claude-sonnet-4-6",
            "gemini-3.8-flash-high",
            "gemini-3.8-flash-low"
        ]
    );
    assert_eq!(
        block_summary(&detail),
        [
            "user text",
            "assistant tool_use:read_file",
            "assistant tool_result:read_file",
            "assistant tool_use:write_file,tool_use:run_command",
            "assistant tool_result:write_file",
            "assistant tool_result:run_command",
            "assistant text",
            "user text",
            "assistant tool_use:invoke_subagent",
            "assistant tool_result:invoke_subagent",
            "assistant tool_result:subagent",
            "assistant thinking,text",
        ]
    );
    let text = |index: usize| detail.messages[index].blocks[0].text.clone().unwrap();
    // Only what the user typed: the local time and model-change notes are dropped.
    assert_eq!(text(0), "Read hello.py and run it");
    // The Created At / Completed At header is not part of the output.
    assert_eq!(text(2), "Showing lines 1-1 of 1\n1: print(\"hello\")");
    assert_eq!(text(5), "Command exited with code 0.\n\nCommand output:\n1");
    assert_eq!(text(10), "There is 1 file: hello.py");
    // Typed arguments from the full transcript.
    assert_eq!(
        detail.messages[3].blocks[1].tool_input,
        Some(json!({"CommandLine": "python3 hello.py", "WaitMsBeforeAsync": 5000}))
    );
    // input_tokens excludes cache reads; output_tokens is taken whole.
    let usage = detail.messages[3].usage.as_ref().unwrap();
    assert_eq!(usage.call_key, format!("{MAIN}:3"));
    assert_eq!(usage.model, "gemini-3.8-flash-low");
    assert_eq!(usage.usage.uncached_input, 2160);
    assert_eq!(usage.usage.cache_read, 7713);
    assert_eq!(usage.usage.output, 101);
    assert_eq!(usage.usage.reasoning_output, 0);
    assert_eq!(usage.effort, None);
}

#[test]
fn plain_transcript_arguments_are_decoded() {
    let path = transcript(PRINT, "transcript.jsonl");
    assert_eq!(
        antigravity_parser::transcript_for(
            path.parent().unwrap().parent().unwrap().parent().unwrap()
        ),
        Some(path.clone())
    );
    let detail = antigravity_parser::parse_detail_with_limit(&path, None).unwrap();
    assert_eq!(
        detail.messages[1].blocks[0].tool_input,
        Some(json!({"CommandLine": "echo hi", "WaitMsBeforeAsync": 5000}))
    );
}

#[test]
fn gemini_session_replays_rewinds_patches_and_hides_injected_context() {
    let path = root().join(format!(
        "tmp/sample/chats/session-2026-06-06T07-16-{}.jsonl",
        &GEMINI[..8]
    ));
    let detail = antigravity_parser::parse_detail_with_limit(&path, Some(24_000)).unwrap();
    assert_eq!(detail.session_id, GEMINI);
    assert_eq!(detail.source.as_deref(), Some("gemini-cli"));
    assert_eq!(detail.project, "/synthetic/gemini-sample");
    assert_eq!(
        block_summary(&detail),
        [
            "user text",
            "assistant thinking,text",
            "user text",
            "assistant tool_use:list_directory,tool_use:invoke_agent",
            "assistant tool_result:list_directory",
        ]
    );
    assert_eq!(
        detail.messages[1].blocks[0].text.as_deref(),
        Some("**Reading flags** Checking the CLI help.")
    );
    assert_eq!(
        detail.messages[2].blocks[0].text.as_deref(),
        Some("List the files")
    );
    assert_eq!(
        detail.messages[4].blocks[0].text.as_deref(),
        Some("a.txt\nb.txt")
    );
    // input includes the cached part; thoughts and tool-use prompt tokens are counted beside it.
    let usage = detail.messages[1].usage.as_ref().unwrap().usage;
    assert_eq!(usage.uncached_input, 18979 - 4000 + 10);
    assert_eq!(usage.cache_read, 4000);
    assert_eq!(usage.output, 522 + 556);
    assert_eq!(usage.reasoning_output, 556);
}

#[test]
fn index_resolves_projects_rolls_up_subagents_and_merges_prompts() {
    let index = indexer::load_or_build(&gemini_only_paths(), None);
    let mut sessions: Vec<(&str, &str, &str)> = index
        .sessions
        .iter()
        .filter(|session| session.agent == Agent::Antigravity)
        .map(|session| {
            (
                session.session_id.as_str(),
                session.project.as_str(),
                session.source.as_deref().unwrap_or(""),
            )
        })
        .collect();
    sessions.sort();
    assert_eq!(
        sessions,
        [
            (MAIN, "/synthetic/agy-sample", "antigravity-cli"),
            (PRINT, "/synthetic/print-only", "antigravity-cli"),
            (GEMINI, "/synthetic/gemini-sample", "gemini-cli"),
            (LEGACY, "/synthetic/agy-sample", "gemini-cli"),
        ]
    );

    // Sub-agent calls count in their parent session and are flagged as sub-agent calls.
    let calls = index.session_calls(Agent::Antigravity, MAIN);
    assert_eq!(calls.len(), 7);
    assert_eq!(calls.iter().filter(|(_, subagent)| *subagent).count(), 2);
    assert!(calls
        .iter()
        .any(|(entry, _)| entry.dedup_key == format!("{CHILD}:3")));
    let gemini_calls = index.session_calls(Agent::Antigravity, GEMINI);
    assert_eq!(gemini_calls.len(), 3);
    let usage = &index.session_usage[&(Agent::Antigravity, MAIN.to_string())];
    assert_eq!(usage.assistant_messages, 7);
    // Gemini calls are priced at Gemini 3.8 Flash's 2026-11 rates (effort suffix stripped):
    // 30,857 uncached at 0.75 + 17,739 cached at 0.075 + 970 output at 3.75 per million.
    // The Claude call made through Antigravity stays unpriced.
    assert!((usage.est_cost_usd - 0.028110675).abs() < 1e-12);
    assert_eq!(usage.unknown_model_tokens, 14_271 + 191);

    // History lines without a conversation ID merge with the transcript's prompts.
    let read = index
        .prompts
        .iter()
        .find(|prompt| prompt.text == "Read hello.py and run it")
        .unwrap();
    assert_eq!(read.origin, PromptOrigin::Both);
    assert_eq!(read.session_id.as_deref(), Some(MAIN));
    assert!(read.has_conversation);
    assert!(!index
        .prompts
        .iter()
        .any(|prompt| prompt.text == "Research the files"));

    let stats = index.stats_for(AgentFilter::Antigravity);
    assert_eq!(stats.total_sessions, 4);
    assert_eq!(stats.total_projects, 3);
    assert_eq!(stats.total_prompts, 7);
    assert_eq!(stats.command_count, 1);
    assert_eq!(index.stats_for(AgentFilter::Claude).total_sessions, 0);
    assert_eq!(index.stats_for(AgentFilter::All).total_sessions, 4);
    let projects: Vec<&str> = index
        .projects_for(AgentFilter::Antigravity)
        .iter()
        .map(|project| project.path.as_str())
        .collect();
    assert_eq!(projects.len(), 3);
    assert!(projects.contains(&"/synthetic/gemini-sample"));

    // The detail view lists the same calls the session total sums.
    let path = PathBuf::from(&index.session_files[&(Agent::Antigravity, MAIN.to_string())]);
    let mut detail = antigravity_parser::parse_detail_with_limit(&path, Some(24_000)).unwrap();
    usage_detail::attach_usage_breakdown(&mut detail, &calls);
    assert_eq!(detail.usage_breakdown.calls.len(), 7);
    assert!(detail
        .messages
        .iter()
        .filter_map(|message| message.usage.as_ref())
        .all(|usage| usage.attributed));
}

#[test]
fn full_text_search_reaches_both_clis_with_detail_ids() {
    let index = indexer::load_or_build(&gemini_only_paths(), None);
    let response =
        content_search::search_index(&index, "approval", AgentFilter::Antigravity, None, None);
    let hits: Vec<(&str, &str, &str)> = response
        .hits
        .iter()
        .map(|hit| {
            (
                hit.session_id.as_str(),
                hit.message_uuid.as_str(),
                hit.role.as_str(),
            )
        })
        .collect();
    assert!(hits.contains(&(GEMINI, "u1", "user")));
    assert!(hits.contains(&(GEMINI, "g1", "assistant")));

    let response =
        content_search::search_index(&index, "hello.py", AgentFilter::Antigravity, None, None);
    assert!(response
        .hits
        .iter()
        .any(|hit| hit.session_id == MAIN && hit.message_uuid == "step-0" && hit.role == "user"));
    assert!(response
        .hits
        .iter()
        .any(|hit| hit.session_id == MAIN && hit.kind == "tool_use"));

    let legacy = content_search::search_index(&index, "single-file", AgentFilter::All, None, None);
    assert_eq!(legacy.hits.len(), 1);
    assert_eq!(legacy.hits[0].session_id, LEGACY);
    assert_eq!(legacy.hits[0].message_uuid, "m1");
}
