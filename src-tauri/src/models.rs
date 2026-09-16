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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    pub claude_data_dir: String,
    pub codex_data_dir: String,
    pub history_file: String,
    pub projects_dir: String,
    pub sessions_dir: String,
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
