//! Shared domain models serialized to the React frontend with camelCase fields.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Agent {
    Claude,
    Codex,
}

impl Agent {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentFilter {
    Claude,
    Codex,
    #[default]
    All,
}

impl AgentFilter {
    pub const fn includes(self, agent: Agent) -> bool {
        matches!(self, Self::All)
            || matches!(
                (self, agent),
                (Self::Claude, Agent::Claude) | (Self::Codex, Agent::Codex)
            )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PromptOrigin {
    History,
    Conversation,
    Both,
}

/// A normalized prompt merged from an agent's history and conversation records.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptEntry {
    /// Stable hash containing the agent, cwd, timestamp, and text.
    pub id: String,
    pub agent: Agent,
    pub text: String,
    pub project: String,
    /// Unix timestamp in milliseconds.
    pub timestamp: i64,
    pub origin: PromptOrigin,
    pub session_id: Option<String>,
    /// True when the session file behind `session_id` exists in the index.
    #[serde(default)]
    pub has_conversation: bool,
    pub git_branch: Option<String>,
    pub is_command: bool,
    pub pasted_count: usize,
    pub char_count: usize,
}

/// A cwd-based project. In the `all` view, agents sharing a cwd are merged here.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInfo {
    pub path: String,
    pub name: String,
    pub agents: Vec<Agent>,
    pub prompt_count: usize,
    pub command_count: usize,
    pub session_count: usize,
    pub first_active: i64,
    pub last_active: i64,
    pub has_conversations: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub entry: PromptEntry,
    /// Character ranges in the original prompt, represented as [start, end).
    pub match_ranges: Vec<[usize; 2]>,
}

/// One full-text hit inside a conversation file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationHit {
    pub agent: Agent,
    pub session_id: String,
    pub project: String,
    pub session_title: String,
    pub session_started_at: i64,
    /// Equals `ChatMessage.uuid` in the detail view when that message survived detail merging.
    pub message_uuid: String,
    pub timestamp: i64,
    /// user | assistant
    pub role: String,
    /// text | thinking | tool_use
    pub kind: String,
    pub tool_name: Option<String>,
    pub snippet: String,
    /// Character ranges inside `snippet`, represented as [start, end).
    pub match_ranges: Vec<[usize; 2]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationSearchResponse {
    pub hits: Vec<ConversationHit>,
    pub scanned_files: usize,
    pub matched_sessions: usize,
    /// True when a per-session cap or the global limit dropped hits.
    pub truncated: bool,
    pub elapsed_ms: u64,
}

/// Token usage attributed to one session. Copied fork/resume events count once, in the earliest
/// session that recorded them, so session totals add up to the global totals.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionUsage {
    pub uncached_input: u64,
    pub cache_read: u64,
    pub cache_creation: u64,
    pub output: u64,
    pub reasoning_output: u64,
    pub total_tokens_including_cache: u64,
    /// Known-model API-equivalent cost only.
    pub est_cost_usd: f64,
    pub unknown_model_tokens: u64,
    pub assistant_messages: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub agent: Agent,
    pub session_id: String,
    pub project: String,
    pub title: String,
    pub started_at: i64,
    pub ended_at: i64,
    pub message_count: usize,
    pub git_branch: Option<String>,
    pub cli_version: Option<String>,
    /// Codex session_meta.source (or `cli` for Claude data).
    pub source: Option<String>,
    pub models: Vec<String>,
    #[serde(default)]
    pub usage: SessionUsage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationDetail {
    pub agent: Agent,
    pub session_id: String,
    pub project: String,
    pub git_branch: Option<String>,
    pub started_at: i64,
    pub ended_at: i64,
    pub cli_version: Option<String>,
    pub source: Option<String>,
    pub models: Vec<String>,
    pub messages: Vec<ChatMessage>,
    /// Filled from the index after parsing; parsers leave it at the default.
    #[serde(default)]
    pub usage: SessionUsage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub agent: Agent,
    pub uuid: String,
    /// user | assistant | system
    pub role: String,
    pub timestamp: i64,
    pub is_sidechain: bool,
    pub blocks: Vec<ContentBlock>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentBlock {
    /// text | thinking | tool_use | tool_result | image
    pub kind: String,
    pub text: Option<String>,
    pub tool_name: Option<String>,
    pub tool_input: Option<serde_json::Value>,
    /// True when `text` was cut at the display limit.
    #[serde(default)]
    pub truncated: bool,
    /// Set on a `tool_result` whose full output Claude Code persisted to a file under the
    /// projects directory; see [`PersistedOutput`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persisted_output: Option<PersistedOutput>,
}

/// Payload of `read_persisted_output`: the file's text, capped at the display limit.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedOutputText {
    pub text: String,
    pub truncated: bool,
    pub size: u64,
}

/// A tool result whose complete output lives in `projects/<dir>/<session>/tool-results/*.txt`.
/// The parser records the absolute path written into the transcript; the command layer
/// re-resolves it against the current projects directory and drops it when the file is absent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedOutput {
    /// Path relative to the Claude projects directory, `/`-separated (after resolution).
    pub path: String,
    /// File size in bytes (0 until resolved).
    pub size: u64,
    /// True when `ContentBlock.text` was replaced by the file's full content (export path).
    #[serde(default)]
    pub inlined: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppStats {
    pub total_prompts: usize,
    pub total_projects: usize,
    pub total_sessions: usize,
    pub total_messages: usize,
    pub history_prompts: usize,
    pub conversation_prompts: usize,
    pub command_count: usize,
    pub first_use: i64,
    pub last_use: i64,
    pub by_day: Vec<DayCount>,
    pub by_hour: Vec<HourCount>,
    pub by_weekday: Vec<WeekdayCount>,
    pub top_projects: Vec<ProjectCount>,
    pub cli_versions: Vec<CliVersionInfo>,
    pub usage: UsageStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DayCount {
    pub day: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HourCount {
    pub hour: u8,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeekdayCount {
    /// 0 = Monday, 6 = Sunday.
    pub weekday: u8,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectCount {
    pub path: String,
    pub name: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CliVersionInfo {
    pub agent: Agent,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexMeta {
    pub built_at: i64,
    pub from_cache: bool,
    pub source_files: usize,
    pub reparsed_files: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IndexPhase {
    Scanning,
    Parsing,
    Assembling,
    Done,
}

/// Payload of the `index-progress` event emitted while an index builds.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexProgress {
    pub phase: IndexPhase,
    /// Files parsed so far in the `parsing` phase.
    pub done: usize,
    /// Files that need parsing in this build (0 while scanning).
    pub total: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub preview: String,
    pub path: Option<String>,
    pub prompt_count: usize,
    pub folder_count: usize,
    pub day_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationExportResult {
    pub preview: String,
    pub path: Option<String>,
    pub message_count: usize,
}

/// One session picked for a batch export, addressed by agent + session ID.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRef {
    pub agent: Agent,
    pub session_id: String,
}

/// Payload of the `export-progress` event emitted while a batch export parses sessions.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportProgress {
    pub done: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionsExportResult {
    /// The merged Markdown file, or the folder holding one file per session plus index.md.
    pub path: String,
    pub merged: bool,
    pub session_count: usize,
    pub message_count: usize,
    /// Markdown files written; index.md is included in folder mode.
    pub file_count: usize,
}

// ----------------------------- Session import -----------------------------

/// One cloud project found in an import zip, keyed by the project's root `cwd`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportProject {
    /// Directory name under `projects/` inside the zip.
    pub cloud_dir: String,
    /// Root working directory recorded in the sessions (decodes to `cloud_dir`); falls back
    /// to `cloud_dir` when no session line carries a cwd.
    pub cloud_cwd: String,
    pub session_count: usize,
    /// Suggested local working directory: a remembered mapping, else a local Claude project
    /// with the same folder name.
    pub suggested_local: Option<String>,
    /// `remembered` | `name` | null
    pub suggestion_source: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportInspection {
    pub zip_path: String,
    pub projects: Vec<ImportProject>,
    pub session_count: usize,
    /// Entries outside `projects/<dir>/` (or junk such as `.DS_Store`) that will be ignored.
    pub skipped_entries: Vec<String>,
}

/// The user's mapping for one cloud project; `local_cwd` empty/null keeps the original path.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectMapping {
    pub cloud_cwd: String,
    #[serde(default)]
    pub local_cwd: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportAction {
    /// The session does not exist locally.
    Add,
    /// The local transcript is a prefix of the imported one, or only side files are missing.
    Update,
    /// Identical transcript and side files.
    SkipIdentical,
    /// The imported transcript is an older snapshot (a prefix of the local one).
    SkipOlder,
    /// Neither side is a prefix of the other, or a same-named side file differs.
    Conflict,
}

/// Line count and last record time of one side of a session, for the conflict dialog.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSide {
    pub lines: usize,
    /// Unix milliseconds of the last record carrying a timestamp; 0 when none.
    pub last_timestamp: i64,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedSession {
    pub session_id: String,
    pub cloud_dir: String,
    pub cloud_cwd: String,
    /// Directory name the session will be written to under the local projects directory.
    pub target_dir: String,
    /// Working directory the session will carry locally (rewritten when mapped).
    pub target_cwd: String,
    /// First user prompt of the imported transcript, for recognition.
    pub title: String,
    pub action: ImportAction,
    pub imported: SessionSide,
    pub local: Option<SessionSide>,
    /// Files in the side directory that will be written.
    pub side_files: usize,
    /// Directory name of another local project already holding a session with this ID.
    pub existing_elsewhere: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPlanCounts {
    pub add: usize,
    pub update: usize,
    pub skip: usize,
    pub conflict: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPlan {
    pub zip_path: String,
    pub sessions: Vec<PlannedSession>,
    pub counts: ImportPlanCounts,
    pub skipped_entries: Vec<String>,
}

/// The user's choice for one conflicting session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictDecision {
    pub cloud_dir: String,
    pub session_id: String,
    /// True keeps the local session untouched; false overwrites it with the imported one.
    pub keep_local: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedProject {
    pub path: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub added: usize,
    pub updated: usize,
    pub skipped: usize,
    pub kept_local: usize,
    pub overwritten: usize,
    /// Transcript and side files written in total.
    pub files_written: usize,
    pub skipped_entries: Vec<String>,
    /// Local projects that received at least one written session.
    pub projects: Vec<ImportedProject>,
    pub index: IndexMeta,
}

// ----------------------------- Settings -----------------------------

/// Old four-field Claude settings remain valid because every new field defaults to empty.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SettingsInput {
    pub claude_data_dir: String,
    pub codex_data_dir: String,
    /// Legacy-compatible optional Claude path overrides.
    pub history_file: String,
    pub projects_dir: String,
    pub sessions_dir: String,
    /// Remembered import mappings: cloud root cwd (for example `/home/user/repo`) to the
    /// local working directory it was imported as. Confirmed on every import.
    pub import_path_mappings: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    pub claude_data_dir: String,
    pub codex_data_dir: String,
    pub history_file: String,
    pub projects_dir: String,
    pub sessions_dir: String,
    #[serde(default)]
    pub import_path_mappings: std::collections::BTreeMap<String, String>,
    pub config_path: String,
    pub resolved: ResolvedPaths,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedPaths {
    pub claude: ResolvedClaudePaths,
    pub codex: ResolvedCodexPaths,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedClaudePaths {
    pub history: String,
    pub projects: String,
    pub sessions: String,
    pub history_exists: bool,
    pub projects_exists: bool,
    pub sessions_exists: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedCodexPaths {
    pub root: String,
    pub history: String,
    pub sessions: String,
    pub archived_sessions: String,
    pub root_exists: bool,
    pub history_exists: bool,
    pub sessions_exists: bool,
    pub archived_sessions_exists: bool,
}

// ----------------------------- Token usage -----------------------------

/// Product-neutral token accounting. `reasoning_output` is a subset of `output`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NormalizedUsage {
    pub uncached_input: u64,
    pub cache_read: u64,
    pub cache_creation: u64,
    pub output: u64,
    pub reasoning_output: u64,
}

impl NormalizedUsage {
    pub const fn total_tokens_including_cache(self) -> u64 {
        self.uncached_input
            .saturating_add(self.cache_read)
            .saturating_add(self.cache_creation)
            .saturating_add(self.output)
    }

    pub const fn cache_hit_rate(self) -> Option<f64> {
        let denominator = self.uncached_input.saturating_add(self.cache_read);
        if denominator == 0 {
            None
        } else {
            Some(self.cache_read as f64 / denominator as f64)
        }
    }

    pub fn add_assign(&mut self, other: Self) {
        self.uncached_input = self.uncached_input.saturating_add(other.uncached_input);
        self.cache_read = self.cache_read.saturating_add(other.cache_read);
        self.cache_creation = self.cache_creation.saturating_add(other.cache_creation);
        self.output = self.output.saturating_add(other.output);
        self.reasoning_output = self.reasoning_output.saturating_add(other.reasoning_output);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageStats {
    pub uncached_input: u64,
    pub cache_read: u64,
    pub cache_creation: u64,
    pub output: u64,
    pub reasoning_output: u64,
    pub total_tokens_including_cache: u64,
    /// Known-model API-equivalent cost only.
    pub est_cost_usd: f64,
    pub unknown_model_tokens: u64,
    pub assistant_messages: usize,
    pub by_model: Vec<ModelUsage>,
    pub by_day: Vec<DayUsage>,
    pub by_project: Vec<ProjectUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelUsage {
    pub agent: Agent,
    pub model: String,
    pub uncached_input: u64,
    pub cache_read: u64,
    pub cache_creation: u64,
    pub output: u64,
    pub reasoning_output: u64,
    pub total_tokens_including_cache: u64,
    pub messages: usize,
    pub est_cost_usd: Option<f64>,
    pub unknown_model_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DayUsage {
    pub day: String,
    pub uncached_input: u64,
    pub cache_read: u64,
    pub cache_creation: u64,
    pub output: u64,
    pub reasoning_output: u64,
    pub total_tokens_including_cache: u64,
    pub est_cost_usd: f64,
    pub unknown_model_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectUsage {
    pub path: String,
    pub name: String,
    pub agents: Vec<Agent>,
    pub uncached_input: u64,
    pub cache_read: u64,
    pub cache_creation: u64,
    pub output: u64,
    pub reasoning_output: u64,
    pub total_tokens_including_cache: u64,
    pub est_cost_usd: f64,
    pub unknown_model_tokens: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalized_total_includes_both_cache_categories_not_reasoning_twice() {
        let usage = NormalizedUsage {
            uncached_input: 100,
            cache_read: 40,
            cache_creation: 20,
            output: 30,
            reasoning_output: 10,
        };
        assert_eq!(usage.total_tokens_including_cache(), 190);
        assert_eq!(usage.cache_hit_rate(), Some(40.0 / 140.0));
    }

    #[test]
    fn zero_input_has_no_cache_hit_rate() {
        assert_eq!(NormalizedUsage::default().cache_hit_rate(), None);
    }
}
