//! Integration coverage for the fully synthetic Codex fixture tree.

use cc_history_viewer_lib::codex_parser;
use cc_history_viewer_lib::indexer;
use cc_history_viewer_lib::models::{Agent, AgentFilter, PromptOrigin};
use cc_history_viewer_lib::state::{ClaudeDataPaths, CodexDataPaths, DataPaths, GeminiDataPaths};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

fn fixture(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/codex")
        .join(relative)
}

fn codex_only_paths() -> DataPaths {
    let codex_root = fixture("");
    let missing_claude = PathBuf::from("/synthetic/nonexistent-claude");
    DataPaths {
        claude: ClaudeDataPaths {
            root: missing_claude.clone(),
            history: missing_claude.join("history.jsonl"),
            projects: missing_claude.join("projects"),
            sessions: missing_claude.join("sessions"),
        },
        codex: CodexDataPaths {
            history: codex_root.join("history.jsonl"),
            sessions: codex_root.join("sessions"),
            archived_sessions: codex_root.join("archived_sessions"),
            root: codex_root,
        },
        gemini: GeminiDataPaths::under(PathBuf::from("/synthetic/nonexistent-gemini")),
    }
}

struct TemporaryCache {
    directory: PathBuf,
    path: PathBuf,
}

impl TemporaryCache {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let sequence = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let directory = std::env::temp_dir().join(format!(
            "coding-agent-history-codex-golden-{}-{nanos}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(&directory).expect("create isolated cache directory");
        let path = directory.join("index-cache-v5.json");
        Self { directory, path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TemporaryCache {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

#[test]
fn current_history_rollout_and_tool_detail_golden() {
    let history = codex_parser::parse_history(&fixture("history.jsonl"));
    assert_eq!(
        history.len(),
        2,
        "damaged and incomplete history lines are skipped"
    );
    assert_eq!(history[0].agent, Agent::Codex);
    assert_eq!(history[0].session_id.as_deref(), Some("codex-current-0001"));
    assert_eq!(history[0].timestamp, 1_784_275_200_000);
    assert_eq!(history[1].timestamp, 1_784_275_500_000);
    assert!(history.iter().all(|prompt| prompt.project.is_empty()));

    let path = fixture("sessions/2026/07/17/rollout-current.jsonl");
    let rollout = codex_parser::parse_rollout_file(&path).expect("current rollout parses");
    assert_eq!(rollout.agent, Agent::Codex);
    assert_eq!(rollout.session_id, "codex-current-0001");
    assert_eq!(rollout.project.as_deref(), Some("/synthetic/project-alpha"));
    assert_eq!(rollout.version.as_deref(), Some("9.9.0-synthetic"));
    assert_eq!(rollout.source.as_deref(), Some("cli"));
    assert!(!rollout.is_subagent);
    assert_eq!(rollout.models, vec!["gpt-5.4", "gpt-5.4-mini"]);
    assert_eq!(
        rollout
            .user_prompts
            .iter()
            .map(|prompt| prompt.text.as_str())
            .collect::<Vec<_>>(),
        vec!["Synthetic current request", "Synthetic follow-up request"]
    );
    assert_eq!(rollout.usage_entries.len(), 2);

    let first = &rollout.usage_entries[0];
    assert_eq!(first.model, "gpt-5.4");
    assert_eq!(first.project, "/synthetic/project-alpha");
    assert_eq!(first.usage.uncached_input, 600);
    assert_eq!(first.usage.cache_read, 400);
    assert_eq!(first.usage.cache_creation, 0);
    assert_eq!(first.usage.output, 200);
    assert_eq!(first.usage.reasoning_output, 50);
    assert_eq!(first.usage.total_tokens_including_cache(), 1_200);

    let second = &rollout.usage_entries[1];
    assert_eq!(second.model, "gpt-5.4-mini");
    assert_eq!(second.usage.uncached_input, 400);
    assert_eq!(second.usage.cache_read, 100);
    assert_eq!(second.usage.output, 80);
    assert_eq!(second.usage.reasoning_output, 20);
    assert_eq!(second.usage.total_tokens_including_cache(), 580);

    let detail = codex_parser::parse_rollout_detail(&path).expect("current detail parses");
    assert_eq!(detail.agent, Agent::Codex);
    assert_eq!(detail.models, vec!["gpt-5.4", "gpt-5.4-mini"]);
    let blocks = detail
        .messages
        .iter()
        .flat_map(|message| message.blocks.iter())
        .collect::<Vec<_>>();
    assert!(blocks.iter().any(|block| {
        block.kind == "text"
            && block.text.as_deref() == Some("Synthetic answer after the damaged line.")
    }));
    assert!(blocks.iter().any(|block| {
        block.kind == "tool_use" && block.tool_name.as_deref() == Some("exec_command")
    }));
    assert!(blocks.iter().any(|block| {
        block.kind == "tool_use" && block.tool_name.as_deref() == Some("apply_patch")
    }));
    assert!(blocks.iter().any(|block| {
        block.kind == "tool_result"
            && block
                .text
                .as_deref()
                .is_some_and(|text| text.contains("synthetic"))
    }));
}

#[test]
fn legacy_and_archived_rollouts_are_normalized() {
    let legacy_path = fixture("sessions/2026/07/16/rollout-legacy.jsonl");
    let legacy = codex_parser::parse_rollout_file(&legacy_path).expect("legacy rollout parses");
    assert_eq!(legacy.session_id, "codex-legacy-0001");
    assert_eq!(legacy.project.as_deref(), Some("/synthetic/project-legacy"));
    assert!(legacy.models.is_empty());
    assert!(legacy.usage_entries.is_empty());
    assert_eq!(legacy.user_prompts.len(), 1);
    assert_eq!(legacy.user_prompts[0].text, "Legacy synthetic request.");

    let detail = codex_parser::parse_rollout_detail(&legacy_path).expect("legacy detail parses");
    let user_text = detail
        .messages
        .iter()
        .filter(|message| message.role == "user")
        .flat_map(|message| message.blocks.iter())
        .filter_map(|block| block.text.as_deref())
        .collect::<Vec<_>>();
    assert_eq!(user_text, vec!["Legacy synthetic request."]);
    assert!(detail
        .messages
        .iter()
        .flat_map(|message| &message.blocks)
        .any(
            |block| block.kind == "tool_use" && block.tool_name.as_deref() == Some("shell_command")
        ));
    assert!(detail
        .messages
        .iter()
        .flat_map(|message| &message.blocks)
        .any(|block| block.kind == "tool_result"
            && block.text.as_deref() == Some("synthetic legacy tool output")));

    let archived =
        codex_parser::parse_rollout_file(&fixture("archived_sessions/synthetic-archived.jsonl"))
            .expect("archived rollout parses");
    assert_eq!(archived.session_id, "codex-archived-0001");
    assert_eq!(
        archived.project.as_deref(),
        Some("/synthetic/project-archive")
    );
    assert_eq!(archived.user_prompts[0].text, "Synthetic archived request");
    assert_eq!(archived.models, vec!["gpt-5.3-codex"]);
    assert_eq!(archived.usage_entries.len(), 1);
    assert_eq!(archived.usage_entries[0].usage.uncached_input, 200);
    assert_eq!(archived.usage_entries[0].usage.cache_read, 50);
    assert_eq!(archived.usage_entries[0].usage.output, 40);
    assert_eq!(
        archived.usage_entries[0]
            .usage
            .total_tokens_including_cache(),
        290
    );
}

#[test]
fn mixed_rollout_merges_transition_prompts_and_preserves_tool_details() {
    let path = fixture("cases/rollout-mixed.jsonl");
    let rollout = codex_parser::parse_rollout_file(&path).expect("mixed rollout parses");
    assert_eq!(rollout.session_id, "codex-mixed-0001");
    assert_eq!(rollout.project.as_deref(), Some("/synthetic/project-mixed"));
    assert_eq!(
        rollout
            .user_prompts
            .iter()
            .map(|prompt| prompt.text.as_str())
            .collect::<Vec<_>>(),
        vec![
            "Legacy turn before the format upgrade.",
            "Current turn at the format transition.",
            "Second current event turn.",
        ],
        "legacy prompts before the transition merge with canonical event prompts"
    );
    assert_eq!(
        rollout.first_prompt,
        "Legacy turn before the format upgrade."
    );
    assert_eq!(rollout.message_count, 5);

    let detail = codex_parser::parse_rollout_detail(&path).expect("mixed detail parses");
    let user_text = detail
        .messages
        .iter()
        .filter(|message| message.role == "user")
        .flat_map(|message| message.blocks.iter())
        .filter_map(|block| block.text.as_deref())
        .collect::<Vec<_>>();
    assert_eq!(
        user_text,
        vec![
            "Legacy turn before the format upgrade.",
            "Current turn at the format transition.",
            "Second current event turn.",
        ],
        "detail uses exactly the prompt records selected for the index"
    );

    let blocks = detail
        .messages
        .iter()
        .flat_map(|message| message.blocks.iter())
        .collect::<Vec<_>>();
    assert!(blocks.iter().any(|block| {
        block.kind == "tool_result"
            && block.tool_name.as_deref() == Some("tool_search")
            && block
                .text
                .as_deref()
                .is_some_and(|text| text.contains("synthetic.search"))
    }));
    assert!(blocks.iter().any(|block| {
        block.kind == "tool_use" && block.tool_name.as_deref() == Some("image_generation")
    }));
    assert!(blocks.iter().any(|block| {
        block.kind == "tool_result"
            && block.tool_name.as_deref() == Some("image_generation")
            && block.text.as_deref().is_some_and(|text| {
                text.contains("Synthetic revised image prompt.")
                    && text.contains("synthetic-image-result")
            })
    }));
    assert!(blocks
        .iter()
        .filter(|block| block.kind == "tool_result")
        .all(|block| block
            .text
            .as_deref()
            .is_some_and(|text| !text.trim().is_empty())));
}

#[test]
fn codex_only_index_filters_subagents_deduplicates_usage_and_reuses_cache() {
    let paths = codex_only_paths();
    let cache = TemporaryCache::new();
    let first = indexer::load_or_build(&paths, Some(cache.path()));

    assert!(!first.from_cache);
    assert_eq!(first.source_files, 8, "history plus seven rollout files");
    assert_eq!(first.reparsed_files, 7);
    assert!(cache.path().is_file());
    assert_eq!(first.prompts.len(), 4);
    assert!(first
        .prompts
        .iter()
        .all(|prompt| prompt.agent == Agent::Codex));
    assert!(first
        .prompts
        .iter()
        .all(|prompt| prompt.id.starts_with("codex-")));
    assert_eq!(
        first
            .prompts
            .iter()
            .filter(|prompt| prompt.origin == PromptOrigin::Both)
            .count(),
        2,
        "history and event prompts merge within Codex"
    );
    assert!(first
        .prompts
        .iter()
        .all(|prompt| { prompt.text != "Synthetic automatically generated subagent task." }));

    assert_eq!(first.sessions.len(), 6);
    assert!(first
        .sessions
        .iter()
        .all(|session| session.agent == Agent::Codex));
    assert!(first
        .sessions
        .iter()
        .all(|session| session.session_id != "codex-subagent-0001"));
    assert!(first
        .session_files
        .contains_key(&(Agent::Codex, "codex-current-0001".to_string())));
    assert!(first
        .session_files
        .contains_key(&(Agent::Codex, "codex-archived-0001".to_string())));

    let all_projects = first.projects_for(AgentFilter::All);
    let codex_projects = first.projects_for(AgentFilter::Codex);
    assert_eq!(all_projects.len(), 4);
    assert_eq!(all_projects.len(), codex_projects.len());
    assert!(first.projects_for(AgentFilter::Claude).is_empty());
    assert!(codex_projects
        .iter()
        .all(|project| project.agents == vec![Agent::Codex]));

    let all = first.stats_for(AgentFilter::All);
    let codex = first.stats_for(AgentFilter::Codex);
    let claude = first.stats_for(AgentFilter::Claude);
    assert_eq!(all.total_prompts, 4);
    assert_eq!(all.total_sessions, 6);
    assert_eq!(all.total_projects, 4);
    assert_eq!(all.total_prompts, codex.total_prompts);
    assert_eq!(all.total_sessions, codex.total_sessions);
    assert_eq!(all.total_projects, codex.total_projects);
    assert_eq!(claude.total_prompts, 0);
    assert_eq!(claude.total_sessions, 0);
    assert_eq!(claude.total_projects, 0);

    let usage = &all.usage;
    assert_eq!(usage.assistant_messages, 6);
    assert_eq!(usage.uncached_input, 1_960);
    assert_eq!(usage.cache_read, 1_090);
    assert_eq!(usage.cache_creation, 0);
    assert_eq!(usage.output, 530);
    assert_eq!(usage.reasoning_output, 140);
    assert_eq!(usage.total_tokens_including_cache, 3_580);
    assert_eq!(usage.unknown_model_tokens, 0);
    assert_eq!(
        usage.total_tokens_including_cache,
        usage.uncached_input + usage.cache_read + usage.output
    );
    assert_eq!(
        usage.total_tokens_including_cache,
        codex.usage.total_tokens_including_cache
    );
    assert_eq!(claude.usage.total_tokens_including_cache, 0);

    let gpt54 = usage
        .by_model
        .iter()
        .find(|model| model.agent == Agent::Codex && model.model == "gpt-5.4")
        .expect("gpt-5.4 model aggregate");
    assert_eq!(
        gpt54.messages, 3,
        "fork copy is removed, distinct event remains"
    );
    assert_eq!(gpt54.uncached_input, 960);
    assert_eq!(gpt54.cache_read, 640);
    assert_eq!(gpt54.output, 320);
    assert_eq!(gpt54.reasoning_output, 80);
    assert_eq!(gpt54.total_tokens_including_cache, 1_920);

    let subagent_project = usage
        .by_project
        .iter()
        .find(|project| project.path == "/synthetic/project-subagent")
        .expect("subagent usage remains attributed to its cwd");
    assert_eq!(subagent_project.agents, vec![Agent::Codex]);
    assert_eq!(subagent_project.uncached_input, 400);
    assert_eq!(subagent_project.cache_read, 300);
    assert_eq!(subagent_project.output, 90);
    assert_eq!(subagent_project.reasoning_output, 30);
    assert_eq!(subagent_project.total_tokens_including_cache, 790);
    assert!(all_projects
        .iter()
        .all(|project| project.path != "/synthetic/project-subagent"));

    let search_all = indexer::search(&first.prompts, "synthetic", None, true, AgentFilter::Codex);
    assert_eq!(search_all.len(), 4);
    assert!(
        indexer::search(&first.prompts, "synthetic", None, true, AgentFilter::Claude,).is_empty()
    );

    let second = indexer::load_or_build(&paths, Some(cache.path()));
    assert!(second.from_cache);
    assert_eq!(second.reparsed_files, 0);
    assert_eq!(second.source_files, first.source_files);
    assert_eq!(
        second
            .stats_for(AgentFilter::Codex)
            .usage
            .total_tokens_including_cache,
        3_580
    );
}
