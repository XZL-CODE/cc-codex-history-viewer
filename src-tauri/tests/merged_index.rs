//! Deterministic cross-agent index, identity, and incremental-cache coverage.

use cc_history_viewer_lib::codex_parser;
use cc_history_viewer_lib::indexer;
use cc_history_viewer_lib::models::{Agent, AgentFilter, UsageStats};
use cc_history_viewer_lib::parser;
use cc_history_viewer_lib::state::{ClaudeDataPaths, CodexDataPaths, DataPaths, GeminiDataPaths};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

struct SyntheticTree {
    root: PathBuf,
    paths: DataPaths,
    claude_rollout: PathBuf,
    cache: PathBuf,
}

impl SyntheticTree {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let serial = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let root = std::env::temp_dir().join(format!(
            "coding-agent-history-merged-{}-{nanos}-{serial}",
            std::process::id()
        ));
        let claude = root.join("claude");
        let codex = root.join("codex");
        let claude_rollout = claude.join("projects/shared/shared-session.jsonl");
        let codex_rollout =
            codex.join("sessions/2026/07/17/rollout-2026-07-17T08-00-00-shared-session.jsonl");

        write_file(
            &claude.join("history.jsonl"),
            concat!(
                "{\"display\":\"Shared synthetic request\",\"timestamp\":1784275200000,",
                "\"project\":\"/synthetic/shared\",\"sessionId\":\"shared-session\"}\n"
            ),
        );
        write_file(
            &claude_rollout,
            concat!(
                "{\"type\":\"user\",\"uuid\":\"claude-user\",\"timestamp\":\"2026-07-17T08:00:00.000Z\",",
                "\"cwd\":\"/synthetic/shared\",\"gitBranch\":\"main\",\"version\":\"1.0.0-synthetic\",",
                "\"message\":{\"role\":\"user\",\"content\":\"Shared synthetic request\"}}\n",
                "{\"type\":\"assistant\",\"uuid\":\"claude-assistant\",\"timestamp\":\"2026-07-17T08:00:02.000Z\",",
                "\"cwd\":\"/synthetic/shared\",\"message\":{\"role\":\"assistant\",\"id\":\"claude-shared-message\",",
                "\"model\":\"claude-sonnet-4-5-20250929\",\"usage\":{\"input_tokens\":100,\"cache_read_input_tokens\":40,",
                "\"cache_creation_input_tokens\":20,\"output_tokens\":30},",
                "\"content\":[{\"type\":\"text\",\"text\":\"Synthetic Claude answer\"}]}}\n"
            ),
        );

        write_file(
            &codex.join("history.jsonl"),
            "{\"session_id\":\"shared-session\",\"text\":\"Shared synthetic request\",\"ts\":1784275200}\n",
        );
        write_file(
            &codex_rollout,
            concat!(
                "{\"timestamp\":\"2026-07-17T08:00:00.000Z\",\"type\":\"session_meta\",",
                "\"payload\":{\"id\":\"shared-session\",\"cwd\":\"/synthetic/shared\",",
                "\"cli_version\":\"1.0.0-synthetic\",\"source\":\"cli\"}}\n",
                "{\"timestamp\":\"2026-07-17T08:00:01.000Z\",\"type\":\"turn_context\",",
                "\"payload\":{\"turn_id\":\"codex-shared-turn\",\"cwd\":\"/synthetic/shared\",\"model\":\"gpt-5.4\"}}\n",
                "{\"timestamp\":\"2026-07-17T08:00:00.000Z\",\"type\":\"event_msg\",",
                "\"payload\":{\"type\":\"user_message\",\"message\":\"Shared synthetic request\"}}\n",
                "{\"timestamp\":\"2026-07-17T08:00:02.000Z\",\"type\":\"response_item\",",
                "\"payload\":{\"type\":\"message\",\"role\":\"assistant\",",
                "\"content\":[{\"type\":\"output_text\",\"text\":\"Synthetic Codex answer\"}]}}\n",
                "{\"timestamp\":\"2026-07-17T08:00:03.000Z\",\"type\":\"event_msg\",",
                "\"payload\":{\"type\":\"token_count\",\"info\":{",
                "\"total_token_usage\":{\"input_tokens\":9000,\"cached_input_tokens\":4000,\"output_tokens\":1800},",
                "\"last_token_usage\":{\"input_tokens\":1000,\"cached_input_tokens\":400,",
                "\"output_tokens\":200,\"reasoning_output_tokens\":50}}}}\n"
            ),
        );

        let paths = DataPaths {
            claude: ClaudeDataPaths {
                root: claude.clone(),
                history: claude.join("history.jsonl"),
                projects: claude.join("projects"),
                sessions: claude.join("sessions"),
            },
            codex: CodexDataPaths {
                root: codex.clone(),
                history: codex.join("history.jsonl"),
                sessions: codex.join("sessions"),
                archived_sessions: codex.join("archived_sessions"),
            },
            gemini: GeminiDataPaths::under(root.join("gemini")),
        };
        let cache = root.join("app-data/index_cache_v5.json");
        Self {
            root,
            paths,
            claude_rollout,
            cache,
        }
    }
}

impl Drop for SyntheticTree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn write_file(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create synthetic source directory");
    }
    fs::write(path, contents).expect("write synthetic source");
}

fn assert_usage_sum(all: &UsageStats, claude: &UsageStats, codex: &UsageStats) {
    assert_eq!(
        all.uncached_input,
        claude.uncached_input + codex.uncached_input
    );
    assert_eq!(all.cache_read, claude.cache_read + codex.cache_read);
    assert_eq!(
        all.cache_creation,
        claude.cache_creation + codex.cache_creation
    );
    assert_eq!(all.output, claude.output + codex.output);
    assert_eq!(
        all.reasoning_output,
        claude.reasoning_output + codex.reasoning_output
    );
    assert_eq!(
        all.total_tokens_including_cache,
        claude.total_tokens_including_cache + codex.total_tokens_including_cache
    );
    assert!((all.est_cost_usd - claude.est_cost_usd - codex.est_cost_usd).abs() < 1e-10);
}

#[test]
fn merged_index_keeps_agent_identity_and_exact_additive_invariants() {
    let tree = SyntheticTree::new();
    let index = indexer::load_or_build(&tree.paths, None);
    let all = index.stats_for(AgentFilter::All);
    let claude = index.stats_for(AgentFilter::Claude);
    let codex = index.stats_for(AgentFilter::Codex);

    assert_eq!(claude.total_prompts, 1);
    assert_eq!(codex.total_prompts, 1);
    assert_eq!(all.total_prompts, 2);
    assert_eq!(
        all.total_prompts,
        claude.total_prompts + codex.total_prompts
    );
    assert_eq!(all.total_sessions, 2);
    assert_eq!(
        all.total_sessions,
        claude.total_sessions + codex.total_sessions
    );
    assert_eq!(index.prompts[0].text, index.prompts[1].text);
    assert_ne!(index.prompts[0].agent, index.prompts[1].agent);
    assert_ne!(index.prompts[0].id, index.prompts[1].id);

    assert_eq!(index.projects_for(AgentFilter::Claude).len(), 1);
    assert_eq!(index.projects_for(AgentFilter::Codex).len(), 1);
    let projects = index.projects_for(AgentFilter::All);
    assert_eq!(projects.len(), 1, "all projects are the exact cwd union");
    assert_eq!(projects[0].agents, vec![Agent::Claude, Agent::Codex]);
    assert_usage_sum(&all.usage, &claude.usage, &codex.usage);
    assert_eq!(claude.usage.total_tokens_including_cache, 190);
    assert_eq!(codex.usage.total_tokens_including_cache, 1_200);
    assert_eq!(codex.usage.reasoning_output, 50);
    assert_eq!(all.usage.total_tokens_including_cache, 1_390);

    let claude_path = index
        .session_files
        .get(&(Agent::Claude, "shared-session".to_string()))
        .expect("Claude session identity");
    let codex_path = index
        .session_files
        .get(&(Agent::Codex, "shared-session".to_string()))
        .expect("Codex session identity");
    assert_ne!(claude_path, codex_path);
    let claude_detail = parser::parse_conversation_detail(Path::new(claude_path)).unwrap();
    let codex_detail = codex_parser::parse_rollout_detail(Path::new(codex_path)).unwrap();
    assert_eq!(claude_detail.agent, Agent::Claude);
    assert_eq!(codex_detail.agent, Agent::Codex);
    assert!(claude_detail
        .messages
        .iter()
        .any(|message| message.role == "assistant"));
    assert!(codex_detail
        .messages
        .iter()
        .any(|message| message.role == "assistant"));
}

#[test]
fn one_or_both_missing_sources_produce_valid_indexes() {
    let tree = SyntheticTree::new();
    let missing = tree.root.join("missing");
    let claude_only = DataPaths {
        claude: tree.paths.claude.clone(),
        codex: CodexDataPaths {
            root: missing.clone(),
            history: missing.join("history.jsonl"),
            sessions: missing.join("sessions"),
            archived_sessions: missing.join("archived_sessions"),
        },
        gemini: GeminiDataPaths::under(missing.join("gemini")),
    };
    let claude_index = indexer::load_or_build(&claude_only, None);
    assert_eq!(claude_index.stats_for(AgentFilter::Claude).total_prompts, 1);
    assert_eq!(claude_index.stats_for(AgentFilter::Codex).total_prompts, 0);

    let no_sources = DataPaths {
        claude: ClaudeDataPaths {
            root: missing.clone(),
            history: missing.join("claude-history.jsonl"),
            projects: missing.join("claude-projects"),
            sessions: missing.join("claude-sessions"),
        },
        codex: CodexDataPaths {
            root: missing.clone(),
            history: missing.join("codex-history.jsonl"),
            sessions: missing.join("codex-sessions"),
            archived_sessions: missing.join("codex-archived"),
        },
        gemini: GeminiDataPaths::under(missing.join("gemini")),
    };
    let empty = indexer::load_or_build(&no_sources, None);
    assert!(empty.prompts.is_empty());
    assert!(empty.sessions.is_empty());
    assert_eq!(empty.stats_for(AgentFilter::All).total_projects, 0);
}

#[test]
fn claude_subagent_files_hide_prompts_and_sessions_but_keep_usage() {
    let tree = SyntheticTree::new();
    let subagent_rollout = tree
        .paths
        .claude
        .projects
        .join("shared/subagents/agent-synthetic.jsonl");
    write_file(
        &subagent_rollout,
        concat!(
            "{\"type\":\"user\",\"uuid\":\"claude-subagent-user\",\"timestamp\":\"2026-07-17T08:05:00.000Z\",",
            "\"cwd\":\"/synthetic/subagent\",\"message\":{\"role\":\"user\",",
            "\"content\":\"Internal synthetic subagent request\"}}\n",
            "{\"type\":\"assistant\",\"uuid\":\"claude-subagent-assistant\",",
            "\"timestamp\":\"2026-07-17T08:05:01.000Z\",\"cwd\":\"/synthetic/subagent\",",
            "\"message\":{\"role\":\"assistant\",\"id\":\"claude-subagent-message\",",
            "\"model\":\"claude-sonnet-4-5-20250929\",\"usage\":{\"input_tokens\":7,",
            "\"cache_read_input_tokens\":5,\"cache_creation_input_tokens\":3,\"output_tokens\":2},",
            "\"content\":[{\"type\":\"text\",\"text\":\"Synthetic subagent answer\"}]}}\n"
        ),
    );

    let parsed = parser::parse_conversation_file(&subagent_rollout).unwrap();
    assert!(parsed.is_subagent);
    assert_eq!(parsed.user_prompts.len(), 1);
    assert_eq!(parsed.usage_entries.len(), 1);

    let detail = parser::parse_conversation_detail(&subagent_rollout).unwrap();
    assert!(!detail.messages.is_empty());
    assert!(detail.messages.iter().all(|message| message.is_sidechain));

    let index = indexer::load_or_build(&tree.paths, None);
    let claude = index.stats_for(AgentFilter::Claude);
    assert_eq!(claude.total_prompts, 1);
    assert_eq!(claude.total_sessions, 1);
    assert_eq!(claude.usage.total_tokens_including_cache, 207);
    assert!(!index.prompts.iter().any(|prompt| {
        prompt.agent == Agent::Claude && prompt.text == "Internal synthetic subagent request"
    }));
}

#[test]
fn cache_handles_hits_single_file_changes_deletion_and_old_versions() {
    let tree = SyntheticTree::new();
    let first = indexer::load_or_build(&tree.paths, Some(&tree.cache));
    assert!(!first.from_cache);
    assert_eq!(first.reparsed_files, 2);

    let hit = indexer::load_or_build(&tree.paths, Some(&tree.cache));
    assert!(hit.from_cache);
    assert_eq!(hit.reparsed_files, 0);

    let mut file = OpenOptions::new()
        .append(true)
        .open(&tree.claude_rollout)
        .unwrap();
    file.write_all(
        concat!(
            "{\"type\":\"user\",\"uuid\":\"claude-user-2\",\"timestamp\":\"2026-07-17T08:10:00.000Z\",",
            "\"cwd\":\"/synthetic/shared\",\"message\":{\"role\":\"user\",",
            "\"content\":\"A second synthetic Claude request\"}}\n"
        )
        .as_bytes(),
    )
    .unwrap();
    file.flush().unwrap();
    let changed = indexer::load_or_build(&tree.paths, Some(&tree.cache));
    assert!(!changed.from_cache);
    assert_eq!(changed.reparsed_files, 1);
    assert_eq!(changed.stats_for(AgentFilter::Claude).total_prompts, 2);

    fs::remove_file(&tree.claude_rollout).unwrap();
    let deleted = indexer::load_or_build(&tree.paths, Some(&tree.cache));
    assert!(!deleted.from_cache);
    assert_eq!(deleted.reparsed_files, 0);
    assert_eq!(deleted.stats_for(AgentFilter::Claude).total_sessions, 0);
    assert_eq!(deleted.source_files, 3);

    write_file(&tree.cache, "{\"version\":4,\"histories\":{},\"files\":{}}");
    let invalidated = indexer::load_or_build(&tree.paths, Some(&tree.cache));
    assert!(!invalidated.from_cache);
    assert_eq!(invalidated.reparsed_files, 1);
}
