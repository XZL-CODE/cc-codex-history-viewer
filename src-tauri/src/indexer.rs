//! Agent-aware incremental index construction and aggregation.

use crate::codex_parser;
use crate::models::*;
use crate::parser::{self, ConvFileResult, RawPrompt, UsageEntry};
use crate::pricing;
use crate::state::DataPaths;
use chrono::{Datelike, Local, TimeZone, Timelike};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const DEDUP_WINDOW_MS: i64 = 5 * 60 * 1000;
const CACHE_VERSION: u32 = 5;

pub struct AppIndex {
    pub prompts: Vec<PromptEntry>,
    pub sessions: Vec<SessionSummary>,
    pub session_files: HashMap<(Agent, String), String>,
    /// Usage attributed per (agent, session id); see [`attribute_session_usage`].
    pub session_usage: HashMap<(Agent, String), SessionUsage>,
    /// Globally unique usage events (fork/resume copies removed), for on-demand range statistics.
    pub usage_events: Vec<UsageEntry>,
    /// CLI versions seen in Claude `sessions/*.json`, merged into every statistics view.
    pub claude_cli_versions: Vec<String>,
    projects_all: Vec<ProjectInfo>,
    projects_claude: Vec<ProjectInfo>,
    projects_codex: Vec<ProjectInfo>,
    stats_all: AppStats,
    stats_claude: AppStats,
    stats_codex: AppStats,
    pub source_files: usize,
    pub built_at: i64,
    pub from_cache: bool,
    pub reparsed_files: usize,
}

impl AppIndex {
    pub fn projects_for(&self, filter: AgentFilter) -> &[ProjectInfo] {
        match filter {
            AgentFilter::Claude => &self.projects_claude,
            AgentFilter::Codex => &self.projects_codex,
            AgentFilter::All => &self.projects_all,
        }
    }

    pub fn stats_for(&self, filter: AgentFilter) -> &AppStats {
        match filter {
            AgentFilter::Claude => &self.stats_claude,
            AgentFilter::Codex => &self.stats_codex,
            AgentFilter::All => &self.stats_all,
        }
    }
}

// ----------------------------- File cache -----------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct FileFingerprint {
    mtime_ns: u64,
    len: u64,
}

impl FileFingerprint {
    fn read(path: &Path) -> Self {
        fs::metadata(path)
            .ok()
            .map(|metadata| Self {
                mtime_ns: metadata
                    .modified()
                    .ok()
                    .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                    .map(|duration| duration.as_nanos().min(u128::from(u64::MAX)) as u64)
                    .unwrap_or(0),
                len: metadata.len(),
            })
            .unwrap_or(Self {
                mtime_ns: 0,
                len: 0,
            })
    }
}

#[derive(Serialize, Deserialize)]
struct HistoryCache {
    agent: Agent,
    fingerprint: FileFingerprint,
    prompts: Vec<RawPrompt>,
}

#[derive(Serialize, Deserialize)]
struct FileCache {
    agent: Agent,
    fingerprint: FileFingerprint,
    conv: ConvFileResult,
}

#[derive(Serialize, Deserialize)]
struct CacheV5 {
    version: u32,
    /// Key is `agent:absolute-path`, so changing roots cannot cross-contaminate records.
    histories: HashMap<String, HistoryCache>,
    files: HashMap<String, FileCache>,
}

impl CacheV5 {
    fn empty() -> Self {
        Self {
            version: CACHE_VERSION,
            histories: HashMap::new(),
            files: HashMap::new(),
        }
    }
}

fn cache_key(agent: Agent, path: &Path) -> String {
    format!("{}:{}", agent.as_str(), path.to_string_lossy())
}

fn read_cache(path: &Path) -> Option<CacheV5> {
    let file = File::open(path).ok()?;
    let cache: CacheV5 = serde_json::from_reader(BufReader::new(file)).ok()?;
    (cache.version == CACHE_VERSION).then_some(cache)
}

fn write_cache(path: &Path, cache: &CacheV5) {
    let Some(parent) = path.parent() else {
        return;
    };
    if fs::create_dir_all(parent).is_err() {
        return;
    }
    let temporary = path.with_extension("json.tmp");
    let written = File::create(&temporary).ok().and_then(|file| {
        let mut writer = BufWriter::new(file);
        serde_json::to_writer(&mut writer, cache).ok()?;
        writer.flush().ok()
    });
    if written.is_some() {
        if fs::rename(&temporary, path).is_err() {
            let _ = fs::remove_file(&temporary);
        }
    } else {
        let _ = fs::remove_file(&temporary);
    }
}

#[derive(Clone)]
struct FileSpec {
    agent: Agent,
    path: PathBuf,
}

/// Progress callback invoked from parser worker threads while an index builds.
pub type ProgressFn<'a> = &'a (dyn Fn(IndexProgress) + Sync);

fn no_progress(_: IndexProgress) {}

pub fn load_or_build(paths: &DataPaths, cache_path: Option<&Path>) -> AppIndex {
    build(paths, cache_path, false, &no_progress)
}

pub fn build_and_cache(paths: &DataPaths, cache_path: Option<&Path>) -> AppIndex {
    build(paths, cache_path, true, &no_progress)
}

/// Build the index, reusing the cache unless `force` is set, and report progress as files parse.
pub fn build_with_progress(
    paths: &DataPaths,
    cache_path: Option<&Path>,
    force: bool,
    progress: ProgressFn,
) -> AppIndex {
    build(paths, cache_path, force, progress)
}

fn build(
    paths: &DataPaths,
    cache_path: Option<&Path>,
    force: bool,
    progress: ProgressFn,
) -> AppIndex {
    progress(IndexProgress {
        phase: IndexPhase::Scanning,
        done: 0,
        total: 0,
    });
    let specs = collect_source_files(paths);
    let mut old = if force {
        CacheV5::empty()
    } else {
        cache_path
            .and_then(read_cache)
            .unwrap_or_else(CacheV5::empty)
    };

    let history_specs = [
        (Agent::Claude, paths.claude.history.as_path()),
        (Agent::Codex, paths.codex.history.as_path()),
    ];
    let mut histories = HashMap::new();
    let mut history_changed = false;
    for (agent, path) in history_specs {
        let key = cache_key(agent, path);
        let fingerprint = FileFingerprint::read(path);
        let cached = old.histories.remove(&key);
        let prompts = match cached {
            Some(record)
                if !force && record.agent == agent && record.fingerprint == fingerprint =>
            {
                record.prompts
            }
            _ => {
                history_changed = true;
                match agent {
                    Agent::Claude => parser::parse_history(path),
                    Agent::Codex => codex_parser::parse_history(path),
                }
            }
        };
        histories.insert(
            key,
            HistoryCache {
                agent,
                fingerprint,
                prompts,
            },
        );
    }

    let mut files = HashMap::with_capacity(specs.len());
    let mut to_parse = Vec::new();
    for spec in specs {
        let key = cache_key(spec.agent, &spec.path);
        let fingerprint = FileFingerprint::read(&spec.path);
        match old.files.remove(&key) {
            Some(record)
                if !force && record.agent == spec.agent && record.fingerprint == fingerprint =>
            {
                files.insert(key, record);
            }
            _ => to_parse.push((key, fingerprint, spec)),
        }
    }

    let reparsed_files = to_parse.len();
    progress(IndexProgress {
        phase: IndexPhase::Parsing,
        done: 0,
        total: reparsed_files,
    });
    for (key, fingerprint, conv) in parse_files_parallel(to_parse, progress) {
        files.insert(
            key,
            FileCache {
                agent: conv.agent,
                fingerprint,
                conv,
            },
        );
    }

    let removed_any = !old.files.is_empty() || !old.histories.is_empty();
    let from_cache = !force && !history_changed && reparsed_files == 0 && !removed_any;
    let cache = CacheV5 {
        version: CACHE_VERSION,
        histories,
        files,
    };
    if force || history_changed || reparsed_files > 0 || removed_any {
        if let Some(path) = cache_path {
            write_cache(path, &cache);
        }
    }
    progress(IndexProgress {
        phase: IndexPhase::Assembling,
        done: reparsed_files,
        total: reparsed_files,
    });
    let index = assemble_index(paths, cache, from_cache, reparsed_files);
    progress(IndexProgress {
        phase: IndexPhase::Done,
        done: reparsed_files,
        total: reparsed_files,
    });
    index
}

fn parse_files_parallel(
    items: Vec<(String, FileFingerprint, FileSpec)>,
    progress: ProgressFn,
) -> Vec<(String, FileFingerprint, ConvFileResult)> {
    let total = items.len();
    let finished = AtomicUsize::new(0);
    items
        .into_par_iter()
        .filter_map(|(key, fingerprint, spec)| {
            let result = match spec.agent {
                Agent::Claude => parser::parse_conversation_file(&spec.path),
                Agent::Codex => codex_parser::parse_rollout_file(&spec.path),
            };
            let done = finished.fetch_add(1, Ordering::Relaxed) + 1;
            progress(IndexProgress {
                phase: IndexPhase::Parsing,
                done,
                total,
            });
            result.map(|result| (key, fingerprint, result))
        })
        .collect()
}

fn collect_source_files(paths: &DataPaths) -> Vec<FileSpec> {
    let mut files = Vec::new();
    collect_jsonl_recursive(&paths.claude.projects, false, Agent::Claude, &mut files);
    collect_jsonl_recursive(&paths.codex.sessions, true, Agent::Codex, &mut files);
    collect_jsonl_recursive(
        &paths.codex.archived_sessions,
        false,
        Agent::Codex,
        &mut files,
    );
    files.sort_by(|left, right| {
        left.agent
            .cmp(&right.agent)
            .then_with(|| left.path.cmp(&right.path))
    });
    files
}

fn collect_jsonl_recursive(
    directory: &Path,
    require_rollout_prefix: bool,
    agent: Agent,
    files: &mut Vec<FileSpec>,
) {
    if !directory.is_dir() {
        return;
    }
    for entry in walkdir::WalkDir::new(directory)
        .into_iter()
        .filter_map(Result::ok)
    {
        let path = entry.path();
        let is_jsonl = path.is_file() && path.extension().is_some_and(|ext| ext == "jsonl");
        let allowed_name = !require_rollout_prefix
            || path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("rollout-"));
        if is_jsonl && allowed_name {
            files.push(FileSpec {
                agent,
                path: path.to_path_buf(),
            });
        }
    }
}

// ----------------------------- Assembly -----------------------------

fn assemble_index(
    paths: &DataPaths,
    cache: CacheV5,
    from_cache: bool,
    reparsed_files: usize,
) -> AppIndex {
    let source_files = cache.files.len()
        + usize::from(paths.claude.history.is_file())
        + usize::from(paths.codex.history.is_file());
    let mut histories: Vec<RawPrompt> = cache
        .histories
        .into_values()
        .flat_map(|history| history.prompts)
        .collect();
    let mut conv: Vec<ConvFileResult> = cache
        .files
        .into_values()
        .map(|record| record.conv)
        .collect();
    conv.sort_by(|left, right| {
        left.agent
            .cmp(&right.agent)
            .then_with(|| left.path.cmp(&right.path))
    });

    resolve_missing_claude_projects(&histories, &mut conv);
    associate_codex_history(&mut histories, &conv);

    let mut prompts = merge_prompts(histories, &conv);
    let winners = session_winners(paths, &conv);
    let session_usage = attribute_session_usage(&conv);
    let sessions = build_sessions(&winners, &session_usage);
    let session_files = winners
        .iter()
        .map(|result| {
            (
                (result.agent, result.session_id.clone()),
                result.path.to_string_lossy().to_string(),
            )
        })
        .collect();
    let present_sessions: HashSet<(Agent, &str)> = winners
        .iter()
        .map(|result| (result.agent, result.session_id.as_str()))
        .collect();
    for prompt in &mut prompts {
        prompt.has_conversation = prompt
            .session_id
            .as_deref()
            .is_some_and(|session_id| present_sessions.contains(&(prompt.agent, session_id)));
    }

    let projects_all = aggregate_projects(&prompts, &sessions, AgentFilter::All);
    let projects_claude = aggregate_projects(&prompts, &sessions, AgentFilter::Claude);
    let projects_codex = aggregate_projects(&prompts, &sessions, AgentFilter::Codex);
    let extra_claude_versions = collect_claude_session_versions(&paths.claude.sessions);
    let usage_events = collect_unique_events(&conv);
    let stats_all = compute_stats(
        &prompts,
        &sessions,
        &usage_events,
        AgentFilter::All,
        &extra_claude_versions,
        None,
    );
    let stats_claude = compute_stats(
        &prompts,
        &sessions,
        &usage_events,
        AgentFilter::Claude,
        &extra_claude_versions,
        None,
    );
    let stats_codex = compute_stats(
        &prompts,
        &sessions,
        &usage_events,
        AgentFilter::Codex,
        &extra_claude_versions,
        None,
    );

    AppIndex {
        prompts,
        sessions,
        session_files,
        session_usage,
        usage_events,
        claude_cli_versions: extra_claude_versions,
        projects_all,
        projects_claude,
        projects_codex,
        stats_all,
        stats_claude,
        stats_codex,
        source_files,
        built_at: now_ms(),
        from_cache,
        reparsed_files,
    }
}

fn resolve_missing_claude_projects(history: &[RawPrompt], conv: &mut [ConvFileResult]) {
    let mut paths = HashSet::new();
    for prompt in history
        .iter()
        .filter(|prompt| prompt.agent == Agent::Claude)
    {
        if !prompt.project.is_empty() {
            paths.insert(prompt.project.clone());
        }
    }
    for result in conv.iter().filter(|result| result.agent == Agent::Claude) {
        if let Some(path) = result.project.as_ref().filter(|path| !path.is_empty()) {
            paths.insert(path.clone());
        }
    }
    let decoded: HashMap<String, String> = paths
        .into_iter()
        .map(|path| (encode_claude_project_dir(&path), path))
        .collect();

    for result in conv
        .iter_mut()
        .filter(|result| result.agent == Agent::Claude && result.project.is_none())
    {
        let directory = result
            .path
            .parent()
            .and_then(Path::file_name)
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        let project = decoded.get(&directory).cloned().unwrap_or(directory);
        result.project = Some(project.clone());
        for prompt in &mut result.user_prompts {
            if prompt.project.is_empty() {
                prompt.project = project.clone();
            }
        }
        for usage in &mut result.usage_entries {
            if usage.project.is_empty() {
                usage.project = project.clone();
            }
        }
    }
}

/// Claude Code names each project directory by replacing every non-alphanumeric character of
/// the cwd with `-`: `/Users/me/我的项目` becomes `-Users-me-----` and `C:\work` becomes `C--work`.
fn encode_claude_project_dir(path: &str) -> String {
    path.chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect()
}

fn associate_codex_history(history: &mut Vec<RawPrompt>, conv: &[ConvFileResult]) {
    let sessions: HashMap<&str, (&str, bool)> = conv
        .iter()
        .filter(|result| result.agent == Agent::Codex)
        .map(|result| {
            (
                result.session_id.as_str(),
                (result.project.as_deref().unwrap_or(""), result.is_subagent),
            )
        })
        .collect();
    history.retain_mut(|prompt| {
        if prompt.agent != Agent::Codex {
            return true;
        }
        let Some(session_id) = prompt.session_id.as_deref() else {
            return true;
        };
        match sessions.get(session_id) {
            Some((_, true)) => false,
            Some((project, false)) => {
                prompt.project = (*project).to_string();
                true
            }
            None => true,
        }
    });
}

// ----------------------------- Prompts and sessions -----------------------------

fn merge_prompts(history: Vec<RawPrompt>, conv: &[ConvFileResult]) -> Vec<PromptEntry> {
    let mut all = history;
    all.extend(
        conv.iter()
            .filter(|result| !result.is_subagent)
            .flat_map(|result| result.user_prompts.iter().cloned()),
    );
    let mut groups: HashMap<(Agent, String, String), Vec<RawPrompt>> = HashMap::new();
    for prompt in all.into_iter().filter(|prompt| !prompt.text.is_empty()) {
        groups
            .entry((prompt.agent, prompt.project.clone(), prompt.text.clone()))
            .or_default()
            .push(prompt);
    }

    let mut entries = Vec::new();
    for ((agent, project, text), mut items) in groups {
        items.sort_by_key(|item| item.timestamp);
        let mut start = 0usize;
        while start < items.len() {
            let base_ts = items[start].timestamp;
            let mut end = start;
            let mut history_seen = false;
            let mut conversation_seen = false;
            let mut session_id = None;
            let mut git_branch = None;
            let mut pasted_count = 0usize;
            while end < items.len()
                && items[end].timestamp.saturating_sub(base_ts) <= DEDUP_WINDOW_MS
            {
                let item = &items[end];
                history_seen |= item.from_history;
                conversation_seen |= !item.from_history;
                if item.session_id.is_some() && (!item.from_history || session_id.is_none()) {
                    session_id = item.session_id.clone();
                }
                if git_branch.is_none() {
                    git_branch = item.git_branch.clone();
                }
                pasted_count = pasted_count.max(item.pasted_count);
                end += 1;
            }
            let origin = match (history_seen, conversation_seen) {
                (true, true) => PromptOrigin::Both,
                (true, false) => PromptOrigin::History,
                _ => PromptOrigin::Conversation,
            };
            entries.push(PromptEntry {
                id: make_prompt_id(agent, &project, base_ts, &text),
                agent,
                text: text.clone(),
                project: project.clone(),
                timestamp: base_ts,
                origin,
                session_id,
                has_conversation: false,
                git_branch,
                is_command: text.starts_with('/'),
                pasted_count,
                char_count: text.chars().count(),
            });
            start = end;
        }
    }
    entries.sort_by(|left, right| {
        right
            .timestamp
            .cmp(&left.timestamp)
            .then_with(|| left.agent.cmp(&right.agent))
            .then_with(|| left.id.cmp(&right.id))
    });
    entries
}

fn make_prompt_id(agent: Agent, project: &str, timestamp: i64, text: &str) -> String {
    format!(
        "{}-{:016x}",
        agent.as_str(),
        parser::stable_hash(&[agent.as_str(), project, &timestamp.to_string(), text])
    )
}

fn session_winners<'a>(paths: &DataPaths, conv: &'a [ConvFileResult]) -> Vec<&'a ConvFileResult> {
    let mut winners: HashMap<(Agent, &str), &ConvFileResult> = HashMap::new();
    for candidate in conv {
        let key = (candidate.agent, candidate.session_id.as_str());
        match winners.get(&key).copied() {
            None => {
                winners.insert(key, candidate);
            }
            Some(current) => {
                let is_active = |result: &ConvFileResult| {
                    result.agent != Agent::Codex || result.path.starts_with(&paths.codex.sessions)
                };
                let candidate_score = (
                    is_active(candidate),
                    candidate.ended_at,
                    candidate.message_count,
                );
                let current_score = (is_active(current), current.ended_at, current.message_count);
                if candidate_score > current_score {
                    winners.insert(key, candidate);
                }
            }
        }
    }
    let mut values: Vec<&ConvFileResult> = winners.into_values().collect();
    values.sort_by(|left, right| {
        left.agent
            .cmp(&right.agent)
            .then_with(|| left.session_id.cmp(&right.session_id))
    });
    values
}

fn build_sessions(
    conv: &[&ConvFileResult],
    usage: &HashMap<(Agent, String), SessionUsage>,
) -> Vec<SessionSummary> {
    let mut sessions: Vec<SessionSummary> = conv
        .iter()
        .filter(|result| !result.is_subagent)
        .map(|result| SessionSummary {
            agent: result.agent,
            session_id: result.session_id.clone(),
            project: result.project.clone().unwrap_or_default(),
            title: result.first_prompt.clone(),
            started_at: result.started_at,
            ended_at: result.ended_at,
            message_count: result.message_count,
            git_branch: result.git_branch.clone(),
            cli_version: result.version.clone(),
            source: result.source.clone(),
            models: result.models.clone(),
            usage: usage
                .get(&(result.agent, result.session_id.clone()))
                .cloned()
                .unwrap_or_default(),
        })
        .collect();
    sessions.sort_by(|left, right| {
        right
            .started_at
            .cmp(&left.started_at)
            .then_with(|| left.agent.cmp(&right.agent))
            .then_with(|| left.session_id.cmp(&right.session_id))
    });
    sessions
}

/// Last path component, accepting both `/` and `\` so Windows working directories get a name.
pub(crate) fn project_name(path: &str) -> String {
    let trimmed = path.trim_end_matches(['/', '\\']);
    trimmed
        .rsplit(['/', '\\'])
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(path)
        .to_string()
}

fn aggregate_projects(
    prompts: &[PromptEntry],
    sessions: &[SessionSummary],
    filter: AgentFilter,
) -> Vec<ProjectInfo> {
    let mut projects: HashMap<String, ProjectInfo> = HashMap::new();
    for prompt in prompts
        .iter()
        .filter(|prompt| filter.includes(prompt.agent))
    {
        if prompt.project.is_empty() {
            continue;
        }
        let info = projects
            .entry(prompt.project.clone())
            .or_insert_with(|| ProjectInfo {
                path: prompt.project.clone(),
                name: project_name(&prompt.project),
                agents: Vec::new(),
                prompt_count: 0,
                command_count: 0,
                session_count: 0,
                first_active: prompt.timestamp,
                last_active: prompt.timestamp,
                has_conversations: false,
            });
        if !info.agents.contains(&prompt.agent) {
            info.agents.push(prompt.agent);
        }
        info.prompt_count += 1;
        info.command_count += usize::from(prompt.is_command);
        info.first_active = info.first_active.min(prompt.timestamp);
        info.last_active = info.last_active.max(prompt.timestamp);
    }
    for session in sessions
        .iter()
        .filter(|session| filter.includes(session.agent) && !session.project.is_empty())
    {
        let info = projects
            .entry(session.project.clone())
            .or_insert_with(|| ProjectInfo {
                path: session.project.clone(),
                name: project_name(&session.project),
                agents: Vec::new(),
                prompt_count: 0,
                command_count: 0,
                session_count: 0,
                first_active: session.started_at,
                last_active: session.ended_at,
                has_conversations: true,
            });
        if !info.agents.contains(&session.agent) {
            info.agents.push(session.agent);
        }
        info.session_count += 1;
        info.has_conversations = true;
        if session.started_at > 0 {
            if info.first_active == 0 {
                info.first_active = session.started_at;
            } else {
                info.first_active = info.first_active.min(session.started_at);
            }
        }
        info.last_active = info.last_active.max(session.ended_at);
    }
    let mut values: Vec<ProjectInfo> = projects.into_values().collect();
    for project in &mut values {
        project.agents.sort();
    }
    values.sort_by(|left, right| {
        right
            .last_active
            .cmp(&left.last_active)
            .then_with(|| left.path.cmp(&right.path))
    });
    values
}

// ----------------------------- Statistics -----------------------------

fn in_range(timestamp: i64, range: Option<(i64, i64)>) -> bool {
    range.map_or(true, |(start, end)| timestamp >= start && timestamp <= end)
}

/// Statistics for an arbitrary inclusive millisecond range, computed on demand from the index.
pub fn compute_stats_in_range(
    index: &AppIndex,
    filter: AgentFilter,
    start_ms: i64,
    end_ms: i64,
) -> AppStats {
    compute_stats(
        &index.prompts,
        &index.sessions,
        &index.usage_events,
        filter,
        &index.claude_cli_versions,
        Some((start_ms, end_ms)),
    )
}

fn compute_stats(
    prompts: &[PromptEntry],
    sessions: &[SessionSummary],
    usage_events: &[UsageEntry],
    filter: AgentFilter,
    extra_claude_versions: &[String],
    range: Option<(i64, i64)>,
) -> AppStats {
    let selected_prompts: Vec<&PromptEntry> = prompts
        .iter()
        .filter(|prompt| filter.includes(prompt.agent))
        .filter(|prompt| in_range(prompt.timestamp, range))
        .collect();
    let selected_sessions: Vec<&SessionSummary> = sessions
        .iter()
        .filter(|session| filter.includes(session.agent))
        .filter(|session| in_range(session.started_at, range))
        .collect();
    // Same definition as the project list: the union of non-empty cwd paths.
    let total_projects = selected_prompts
        .iter()
        .map(|prompt| prompt.project.as_str())
        .chain(
            selected_sessions
                .iter()
                .map(|session| session.project.as_str()),
        )
        .filter(|project| !project.is_empty())
        .collect::<HashSet<_>>()
        .len();
    let mut history_prompts = 0usize;
    let mut conversation_prompts = 0usize;
    let mut command_count = 0usize;
    let mut first_use = i64::MAX;
    let mut last_use = i64::MIN;
    let mut by_day = HashMap::new();
    let mut by_hour = [0usize; 24];
    let mut by_weekday = [0usize; 7];
    let mut by_project = HashMap::new();

    for prompt in &selected_prompts {
        match prompt.origin {
            PromptOrigin::History => history_prompts += 1,
            PromptOrigin::Conversation => conversation_prompts += 1,
            PromptOrigin::Both => {
                history_prompts += 1;
                conversation_prompts += 1;
            }
        }
        command_count += usize::from(prompt.is_command);
        first_use = first_use.min(prompt.timestamp);
        last_use = last_use.max(prompt.timestamp);
        if let Some(time) = Local.timestamp_millis_opt(prompt.timestamp).single() {
            *by_day
                .entry(time.format("%Y-%m-%d").to_string())
                .or_insert(0usize) += 1;
            by_hour[time.hour() as usize] += 1;
            by_weekday[time.weekday().num_days_from_monday() as usize] += 1;
        }
        if !prompt.project.is_empty() {
            *by_project.entry(prompt.project.clone()).or_insert(0usize) += 1;
        }
    }
    if first_use == i64::MAX {
        first_use = 0;
    }
    if last_use == i64::MIN {
        last_use = 0;
    }

    let mut by_day: Vec<DayCount> = by_day
        .into_iter()
        .map(|(day, count)| DayCount { day, count })
        .collect();
    by_day.sort_by(|left, right| left.day.cmp(&right.day));
    let by_hour = (0..24)
        .map(|hour| HourCount {
            hour: hour as u8,
            count: by_hour[hour],
        })
        .collect();
    let by_weekday = (0..7)
        .map(|weekday| WeekdayCount {
            weekday: weekday as u8,
            count: by_weekday[weekday],
        })
        .collect();
    let mut top_projects: Vec<ProjectCount> = by_project
        .into_iter()
        .map(|(path, count)| ProjectCount {
            name: project_name(&path),
            path,
            count,
        })
        .collect();
    top_projects.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.path.cmp(&right.path))
    });
    top_projects.truncate(8);

    let mut versions: HashSet<(Agent, String)> = selected_sessions
        .iter()
        .filter_map(|session| {
            session
                .cli_version
                .as_ref()
                .filter(|version| !version.is_empty())
                .map(|version| (session.agent, version.clone()))
        })
        .collect();
    if filter.includes(Agent::Claude) {
        versions.extend(
            extra_claude_versions
                .iter()
                .cloned()
                .map(|version| (Agent::Claude, version)),
        );
    }
    let mut cli_versions: Vec<CliVersionInfo> = versions
        .into_iter()
        .map(|(agent, version)| CliVersionInfo { agent, version })
        .collect();
    cli_versions.sort_by(|left, right| {
        left.agent
            .cmp(&right.agent)
            .then_with(|| version_cmp(&right.version, &left.version))
    });

    AppStats {
        total_prompts: selected_prompts.len(),
        total_projects,
        total_sessions: selected_sessions.len(),
        total_messages: selected_sessions
            .iter()
            .map(|session| session.message_count)
            .sum(),
        history_prompts,
        conversation_prompts,
        command_count,
        first_use,
        last_use,
        by_day,
        by_hour,
        by_weekday,
        top_projects,
        cli_versions,
        usage: compute_usage(usage_events, filter, range),
    }
}

#[derive(Default)]
struct UsageAggregate {
    usage: NormalizedUsage,
    messages: usize,
    cost: f64,
    unknown_tokens: u64,
    agents: HashSet<Agent>,
}

impl UsageAggregate {
    fn add(&mut self, event: &UsageEntry, cost: Option<f64>) {
        self.usage.add_assign(event.usage);
        self.messages += 1;
        self.agents.insert(event.agent);
        match cost {
            Some(value) => self.cost += value,
            None => {
                self.unknown_tokens = self
                    .unknown_tokens
                    .saturating_add(event.usage.total_tokens_including_cache());
            }
        }
    }

    fn into_session_usage(self) -> SessionUsage {
        SessionUsage {
            uncached_input: self.usage.uncached_input,
            cache_read: self.usage.cache_read,
            cache_creation: self.usage.cache_creation,
            output: self.usage.output,
            reasoning_output: self.usage.reasoning_output,
            total_tokens_including_cache: self.usage.total_tokens_including_cache(),
            est_cost_usd: self.cost,
            unknown_model_tokens: self.unknown_tokens,
            assistant_messages: self.messages,
        }
    }
}

/// The session that owns a file's usage: Claude sub-agent transcripts live under
/// `<parent session id>/subagents/`, so their calls roll up into the parent session.
fn owner_session_id(result: &ConvFileResult) -> String {
    if result.agent == Agent::Claude && result.is_subagent {
        let components: Vec<&std::ffi::OsStr> = result
            .path
            .components()
            .map(|component| component.as_os_str())
            .collect();
        if let Some(position) = components
            .iter()
            .position(|component| *component == "subagents")
        {
            if let Some(parent) = position
                .checked_sub(1)
                .and_then(|index| components[index].to_str())
                .filter(|parent| !parent.is_empty())
            {
                return parent.to_string();
            }
        }
    }
    result.session_id.clone()
}

/// Attribute every unique usage event to exactly one session. Files are visited from the
/// earliest session onwards, so events copied into a later fork/resume stay with the original.
fn attribute_session_usage(conv: &[ConvFileResult]) -> HashMap<(Agent, String), SessionUsage> {
    let mut ordered: Vec<&ConvFileResult> = conv.iter().collect();
    ordered.sort_by(|left, right| {
        left.started_at
            .cmp(&right.started_at)
            .then_with(|| left.agent.cmp(&right.agent))
            .then_with(|| left.path.cmp(&right.path))
    });
    let mut seen: HashSet<(Agent, &str)> = HashSet::new();
    let mut aggregates: HashMap<(Agent, String), UsageAggregate> = HashMap::new();
    for result in ordered {
        let owner = owner_session_id(result);
        for event in &result.usage_entries {
            if !seen.insert((event.agent, event.dedup_key.as_str())) {
                continue;
            }
            let cost = pricing::estimate_cost(event.agent, &event.model, event.usage);
            aggregates
                .entry((result.agent, owner.clone()))
                .or_default()
                .add(event, cost);
        }
    }
    aggregates
        .into_iter()
        .map(|(key, aggregate)| (key, aggregate.into_session_usage()))
        .collect()
}

/// Every usage event across all files, sorted deterministically and deduplicated by
/// `(agent, dedup_key)` so fork/resume copies count once.
fn collect_unique_events(conv: &[ConvFileResult]) -> Vec<UsageEntry> {
    let mut events: Vec<&UsageEntry> = conv
        .iter()
        .flat_map(|result| result.usage_entries.iter())
        .collect();
    events.sort_by(|left, right| {
        left.agent
            .cmp(&right.agent)
            .then_with(|| left.timestamp.cmp(&right.timestamp))
            .then_with(|| left.dedup_key.cmp(&right.dedup_key))
            .then_with(|| left.project.cmp(&right.project))
    });
    let mut seen: HashSet<(Agent, &str)> = HashSet::new();
    events
        .into_iter()
        .filter(|event| seen.insert((event.agent, event.dedup_key.as_str())))
        .cloned()
        .collect()
}

/// Aggregate already-unique events for one agent filter and optional time range.
fn compute_usage(
    events: &[UsageEntry],
    filter: AgentFilter,
    range: Option<(i64, i64)>,
) -> UsageStats {
    let mut total = NormalizedUsage::default();
    let mut est_cost_usd = 0.0;
    let mut unknown_model_tokens = 0u64;
    let mut assistant_messages = 0usize;
    let mut by_model: HashMap<(Agent, String), UsageAggregate> = HashMap::new();
    let mut by_day: HashMap<String, UsageAggregate> = HashMap::new();
    let mut by_project: HashMap<String, UsageAggregate> = HashMap::new();

    for event in events
        .iter()
        .filter(|event| filter.includes(event.agent))
        .filter(|event| in_range(event.timestamp, range))
    {
        let cost = pricing::estimate_cost(event.agent, &event.model, event.usage);
        total.add_assign(event.usage);
        assistant_messages += 1;
        match cost {
            Some(value) => est_cost_usd += value,
            None => {
                unknown_model_tokens =
                    unknown_model_tokens.saturating_add(event.usage.total_tokens_including_cache());
            }
        }
        by_model
            .entry((event.agent, event.model.clone()))
            .or_default()
            .add(event, cost);
        if let Some(time) = Local.timestamp_millis_opt(event.timestamp).single() {
            by_day
                .entry(time.format("%Y-%m-%d").to_string())
                .or_default()
                .add(event, cost);
        }
        if !event.project.is_empty() {
            by_project
                .entry(event.project.clone())
                .or_default()
                .add(event, cost);
        }
    }

    let mut by_model: Vec<ModelUsage> = by_model
        .into_iter()
        .map(|((agent, model), aggregate)| ModelUsage {
            agent,
            model,
            uncached_input: aggregate.usage.uncached_input,
            cache_read: aggregate.usage.cache_read,
            cache_creation: aggregate.usage.cache_creation,
            output: aggregate.usage.output,
            reasoning_output: aggregate.usage.reasoning_output,
            total_tokens_including_cache: aggregate.usage.total_tokens_including_cache(),
            messages: aggregate.messages,
            est_cost_usd: (aggregate.unknown_tokens == 0).then_some(aggregate.cost),
            unknown_model_tokens: aggregate.unknown_tokens,
        })
        .collect();
    by_model.sort_by(
        |left, right| match (left.est_cost_usd, right.est_cost_usd) {
            (Some(left_cost), Some(right_cost)) => right_cost
                .partial_cmp(&left_cost)
                .unwrap_or(std::cmp::Ordering::Equal),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => right
                .total_tokens_including_cache
                .cmp(&left.total_tokens_including_cache),
        },
    );

    let mut by_day: Vec<DayUsage> = by_day
        .into_iter()
        .map(|(day, aggregate)| DayUsage {
            day,
            uncached_input: aggregate.usage.uncached_input,
            cache_read: aggregate.usage.cache_read,
            cache_creation: aggregate.usage.cache_creation,
            output: aggregate.usage.output,
            reasoning_output: aggregate.usage.reasoning_output,
            total_tokens_including_cache: aggregate.usage.total_tokens_including_cache(),
            est_cost_usd: aggregate.cost,
            unknown_model_tokens: aggregate.unknown_tokens,
        })
        .collect();
    by_day.sort_by(|left, right| left.day.cmp(&right.day));

    let mut by_project: Vec<ProjectUsage> = by_project
        .into_iter()
        .map(|(path, aggregate)| {
            let mut agents: Vec<Agent> = aggregate.agents.into_iter().collect();
            agents.sort();
            ProjectUsage {
                name: project_name(&path),
                path,
                agents,
                uncached_input: aggregate.usage.uncached_input,
                cache_read: aggregate.usage.cache_read,
                cache_creation: aggregate.usage.cache_creation,
                output: aggregate.usage.output,
                reasoning_output: aggregate.usage.reasoning_output,
                total_tokens_including_cache: aggregate.usage.total_tokens_including_cache(),
                est_cost_usd: aggregate.cost,
                unknown_model_tokens: aggregate.unknown_tokens,
            }
        })
        .collect();
    by_project.sort_by(|left, right| {
        right
            .est_cost_usd
            .partial_cmp(&left.est_cost_usd)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                right
                    .total_tokens_including_cache
                    .cmp(&left.total_tokens_including_cache)
            })
    });
    by_project.truncate(8);

    UsageStats {
        uncached_input: total.uncached_input,
        cache_read: total.cache_read,
        cache_creation: total.cache_creation,
        output: total.output,
        reasoning_output: total.reasoning_output,
        total_tokens_including_cache: total.total_tokens_including_cache(),
        est_cost_usd,
        unknown_model_tokens,
        assistant_messages,
        by_model,
        by_day,
        by_project,
    }
}

fn version_cmp(left: &str, right: &str) -> std::cmp::Ordering {
    let parse = |version: &str| {
        version
            .split(|character: char| !character.is_ascii_digit())
            .filter(|part| !part.is_empty())
            .map(|part| part.parse::<u32>().unwrap_or(0))
            .collect::<Vec<_>>()
    };
    parse(left).cmp(&parse(right))
}

fn collect_claude_session_versions(directory: &Path) -> Vec<String> {
    if !directory.is_dir() {
        return Vec::new();
    }
    let mut versions = HashSet::new();
    if let Ok(entries) = fs::read_dir(directory) {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if !path
                .extension()
                .is_some_and(|extension| extension == "json")
            {
                continue;
            }
            if let Ok(file) = File::open(path) {
                if let Ok(value) = serde_json::from_reader::<_, serde_json::Value>(file) {
                    if let Some(version) = value
                        .get("version")
                        .and_then(serde_json::Value::as_str)
                        .filter(|version| !version.is_empty())
                    {
                        versions.insert(version.to_string());
                    }
                }
            }
        }
    }
    let mut versions: Vec<String> = versions.into_iter().collect();
    versions.sort_by(|left, right| version_cmp(right, left));
    versions
}

// ----------------------------- Search -----------------------------

pub(crate) fn fold_char(character: char) -> char {
    character.to_lowercase().next().unwrap_or(character)
}

pub fn search(
    prompts: &[PromptEntry],
    query: &str,
    project_filter: Option<&str>,
    include_commands: bool,
    agent_filter: AgentFilter,
) -> Vec<SearchResult> {
    let tokens: Vec<Vec<char>> = query
        .split_whitespace()
        .map(|token| token.chars().map(fold_char).collect())
        .filter(|token: &Vec<char>| !token.is_empty())
        .collect();
    if tokens.is_empty() {
        return Vec::new();
    }
    let mut results = Vec::new();
    for prompt in prompts {
        if !agent_filter.includes(prompt.agent)
            || project_filter.is_some_and(|project| prompt.project != project)
            || (!include_commands && prompt.is_command)
        {
            continue;
        }
        let folded: Vec<char> = prompt.text.chars().map(fold_char).collect();
        let mut ranges = Vec::new();
        let mut matched = true;
        for token in &tokens {
            let occurrences = find_all(&folded, token);
            if occurrences.is_empty() {
                matched = false;
                break;
            }
            ranges.extend(
                occurrences
                    .into_iter()
                    .map(|start| [start, start + token.len()]),
            );
        }
        if matched {
            results.push(SearchResult {
                entry: prompt.clone(),
                match_ranges: merge_ranges(ranges),
            });
        }
    }
    results
}

pub(crate) fn find_all(haystack: &[char], needle: &[char]) -> Vec<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return Vec::new();
    }
    let mut matches = Vec::new();
    let mut index = 0usize;
    while index <= haystack.len() - needle.len() {
        if haystack[index..].starts_with(needle) {
            matches.push(index);
            index += needle.len();
        } else {
            index += 1;
        }
    }
    matches
}

pub(crate) fn merge_ranges(mut ranges: Vec<[usize; 2]>) -> Vec<[usize; 2]> {
    ranges.sort();
    let mut merged: Vec<[usize; 2]> = Vec::new();
    for range in ranges {
        if let Some(last) = merged.last_mut() {
            if range[0] <= last[1] {
                last[1] = last[1].max(range[1]);
                continue;
            }
        }
        merged.push(range);
    }
    merged
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(agent: Agent, text: &str, project: &str, timestamp: i64, history: bool) -> RawPrompt {
        RawPrompt {
            agent,
            text: text.to_string(),
            project: project.to_string(),
            timestamp,
            session_id: None,
            git_branch: None,
            pasted_count: 0,
            from_history: history,
        }
    }

    fn prompt(agent: Agent, text: &str) -> PromptEntry {
        PromptEntry {
            id: format!("{}:{text}", agent.as_str()),
            agent,
            text: text.to_string(),
            project: "/synthetic/project".to_string(),
            timestamp: 0,
            origin: PromptOrigin::History,
            session_id: None,
            has_conversation: false,
            git_branch: None,
            is_command: false,
            pasted_count: 0,
            char_count: text.chars().count(),
        }
    }

    #[test]
    fn same_prompt_across_agents_is_never_deduplicated() {
        let timestamp = 1_700_000_000_000;
        let prompts = merge_prompts(
            vec![
                raw(Agent::Claude, "same", "/synthetic/project", timestamp, true),
                raw(Agent::Codex, "same", "/synthetic/project", timestamp, true),
            ],
            &[],
        );
        assert_eq!(prompts.len(), 2);
        assert_ne!(prompts[0].id, prompts[1].id);
    }

    #[test]
    fn stable_prompt_id_is_deterministic_and_agent_aware() {
        let first = make_prompt_id(Agent::Claude, "/p", 100, "hello");
        let second = make_prompt_id(Agent::Claude, "/p", 100, "hello");
        let codex = make_prompt_id(Agent::Codex, "/p", 100, "hello");
        assert_eq!(first, second);
        assert_ne!(first, codex);
    }

    #[test]
    fn search_filters_agent_and_merges_ranges() {
        let prompts = vec![
            prompt(Agent::Claude, "foo something bar"),
            prompt(Agent::Codex, "foo something bar"),
        ];
        let results = search(&prompts, "foo bar", None, true, AgentFilter::Codex);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].entry.agent, Agent::Codex);
    }

    fn conv_with_usage(
        session_id: &str,
        path: &str,
        started_at: i64,
        is_subagent: bool,
        keys: &[&str],
    ) -> ConvFileResult {
        ConvFileResult {
            agent: Agent::Claude,
            path: PathBuf::from(path),
            session_id: session_id.to_string(),
            project: Some("/synthetic/project".to_string()),
            git_branch: None,
            version: None,
            source: Some("cli".to_string()),
            models: vec!["claude-sonnet-4-5".to_string()],
            is_subagent,
            started_at,
            ended_at: started_at + 1_000,
            message_count: keys.len(),
            first_prompt: String::new(),
            user_prompts: Vec::new(),
            usage_entries: keys
                .iter()
                .map(|key| UsageEntry {
                    agent: Agent::Claude,
                    dedup_key: (*key).to_string(),
                    model: "claude-sonnet-4-5".to_string(),
                    timestamp: started_at,
                    project: "/synthetic/project".to_string(),
                    usage: NormalizedUsage {
                        uncached_input: 100,
                        cache_read: 50,
                        cache_creation: 10,
                        output: 20,
                        reasoning_output: 0,
                    },
                })
                .collect(),
        }
    }

    #[test]
    fn session_usage_counts_copied_events_once_and_rolls_subagents_into_parent() {
        let original = conv_with_usage(
            "orig",
            "/synthetic/projects/p/orig.jsonl",
            100,
            false,
            &["k1", "k2"],
        );
        let resumed = conv_with_usage(
            "resumed",
            "/synthetic/projects/p/resumed.jsonl",
            200,
            false,
            &["k1", "k2", "k3"],
        );
        let subagent = conv_with_usage(
            "agent-1",
            "/synthetic/projects/p/orig/subagents/agent-1.jsonl",
            150,
            true,
            &["k4"],
        );
        let conv = vec![resumed, subagent, original];
        let usage = attribute_session_usage(&conv);

        let orig = &usage[&(Agent::Claude, "orig".to_string())];
        assert_eq!(orig.assistant_messages, 3, "k1, k2 and the sub-agent's k4");
        assert_eq!(orig.total_tokens_including_cache, 3 * 180);
        assert!(orig.est_cost_usd > 0.0);
        assert_eq!(orig.unknown_model_tokens, 0);
        let resumed = &usage[&(Agent::Claude, "resumed".to_string())];
        assert_eq!(resumed.assistant_messages, 1, "only the new k3 call");
        assert!(!usage.contains_key(&(Agent::Claude, "agent-1".to_string())));

        let global = compute_usage(&collect_unique_events(&conv), AgentFilter::All, None);
        let session_sum: u64 = usage
            .values()
            .map(|entry| entry.total_tokens_including_cache)
            .sum();
        assert_eq!(session_sum, global.total_tokens_including_cache);
        assert_eq!(
            usage
                .values()
                .map(|entry| entry.assistant_messages)
                .sum::<usize>(),
            global.assistant_messages
        );
    }

    #[test]
    fn range_statistics_only_count_records_inside_the_window() {
        let mut early = prompt(Agent::Claude, "early");
        early.timestamp = 1_000;
        early.project = "/synthetic/early".to_string();
        let mut late = prompt(Agent::Codex, "late");
        late.timestamp = 5_000;
        late.project = "/synthetic/late".to_string();
        let prompts = vec![early, late];
        let events = collect_unique_events(&[
            conv_with_usage(
                "s1",
                "/synthetic/projects/p/s1.jsonl",
                1_000,
                false,
                &["k1"],
            ),
            conv_with_usage(
                "s2",
                "/synthetic/projects/p/s2.jsonl",
                5_000,
                false,
                &["k2"],
            ),
        ]);

        let all = compute_stats(&prompts, &[], &events, AgentFilter::All, &[], None);
        assert_eq!(all.total_prompts, 2);
        assert_eq!(all.total_projects, 2);
        assert_eq!(all.usage.assistant_messages, 2);

        let window = compute_stats(
            &prompts,
            &[],
            &events,
            AgentFilter::All,
            &[],
            Some((4_000, 6_000)),
        );
        assert_eq!(window.total_prompts, 1);
        assert_eq!(window.total_projects, 1);
        assert_eq!(window.first_use, 5_000);
        assert_eq!(window.usage.assistant_messages, 1);
        assert_eq!(window.usage.total_tokens_including_cache, 180);

        let empty = compute_stats(
            &prompts,
            &[],
            &events,
            AgentFilter::All,
            &[],
            Some((10_000, 20_000)),
        );
        assert_eq!(empty.total_prompts, 0);
        assert_eq!(empty.first_use, 0);
        assert!(empty.usage.by_day.is_empty());
    }

    #[test]
    fn project_names_and_directory_encoding_cover_windows_and_unicode() {
        assert_eq!(project_name("/Users/me/proj/"), "proj");
        assert_eq!(project_name("C:\\Users\\me\\proj"), "proj");
        assert_eq!(project_name("/"), "/");
        assert_eq!(
            encode_claude_project_dir("/Users/you/Desktop/我的项目/claude-code"),
            "-Users-you-Desktop------claude-code"
        );
        assert_eq!(encode_claude_project_dir("C:\\work"), "C--work");
    }

    #[test]
    fn all_projects_are_a_cwd_union() {
        let prompts = vec![prompt(Agent::Claude, "one"), prompt(Agent::Codex, "two")];
        let all = aggregate_projects(&prompts, &[], AgentFilter::All);
        let claude = aggregate_projects(&prompts, &[], AgentFilter::Claude);
        let codex = aggregate_projects(&prompts, &[], AgentFilter::Codex);
        assert_eq!(all.len(), 1);
        assert_eq!(claude.len(), 1);
        assert_eq!(codex.len(), 1);
        assert_eq!(all[0].prompt_count, 2);
        assert_eq!(all[0].agents, vec![Agent::Claude, Agent::Codex]);
    }
}
