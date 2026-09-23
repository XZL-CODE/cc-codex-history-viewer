//! 暴露给前端的 Tauri Commands。
//!
//! 所有会触碰索引或会话文件的 command 都是 async：索引构建与会话解析在 Tauri 的阻塞线程池中
//! 执行，不会卡住主线程；构建期间通过 `index-progress` 事件向前端汇报进度。

use crate::content_search;
use crate::export::{self, ExportParams, Lang};
use crate::indexer::{self, AppIndex};
use crate::models::*;
use crate::state::{self, load_settings, resolve_data_paths, resolve_from_settings, AppState};
use crate::{codex_parser, import, parser, persisted};
use rayon::prelude::*;
use serde::Serialize;
use std::cmp::Reverse;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};

/// Event name for index build progress; the payload is an [`IndexProgress`].
pub const INDEX_PROGRESS_EVENT: &str = "index-progress";

/// Event name for batch export progress; the payload is an [`ExportProgress`].
pub const EXPORT_PROGRESS_EVENT: &str = "export-progress";

/// 详情页按需读取持久化工具输出的上限（字符数上限之外的字节保险）。
const DISPLAY_PERSISTED_MAX_BYTES: usize = 4 * 1024 * 1024;
/// 导出内联持久化输出的上限；导出要求完整，只防御异常大的文件。
const EXPORT_PERSISTED_MAX_BYTES: usize = 64 * 1024 * 1024;

/// Agent-aware, file-level cache owned by this application.
fn cache_file(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|d| d.join("index_cache_v5.json"))
}

/// Remove only obsolete cache files written by this application.
fn cleanup_legacy_cache(app: &AppHandle) {
    if let Ok(dir) = app.path().app_data_dir() {
        for name in ["index_cache.json", "index_cache_v2.json"] {
            let legacy = dir.join(name);
            if legacy.exists() {
                let _ = std::fs::remove_file(legacy);
            }
        }
    }
}

/// Rate-limited event emitter shared by parser worker threads: `boundary` payloads (start,
/// end, phase changes) always go out, the rest at most once per 80 ms.
struct ThrottledEmitter {
    app: AppHandle,
    event: &'static str,
    last_emit: Mutex<Option<Instant>>,
}

impl ThrottledEmitter {
    const MIN_INTERVAL: Duration = Duration::from_millis(80);

    fn new(app: AppHandle, event: &'static str) -> Self {
        Self {
            app,
            event,
            last_emit: Mutex::new(None),
        }
    }

    fn emit<T: Serialize + Clone>(&self, payload: T, boundary: bool) {
        {
            let mut last = self
                .last_emit
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if !boundary && last.is_some_and(|at| at.elapsed() < Self::MIN_INTERVAL) {
                return;
            }
            *last = Some(Instant::now());
        }
        let _ = self.app.emit(self.event, payload);
    }
}

/// Index build progress reporter.
struct ProgressReporter(ThrottledEmitter);

impl ProgressReporter {
    fn new(app: AppHandle) -> Self {
        Self(ThrottledEmitter::new(app, INDEX_PROGRESS_EVENT))
    }

    fn report(&self, progress: IndexProgress) {
        let boundary = progress.phase != IndexPhase::Parsing
            || progress.done == 0
            || progress.done == progress.total;
        self.0.emit(progress, boundary);
    }
}

/// Build (or incrementally refresh) the index on the blocking thread pool.
async fn build_index(app: &AppHandle, force: bool) -> Result<AppIndex, String> {
    let paths = resolve_data_paths(app)?;
    let claude_exists = paths.claude.history.is_file() || paths.claude.projects.is_dir();
    let codex_exists = paths.codex.history.is_file()
        || paths.codex.sessions.is_dir()
        || paths.codex.archived_sessions.is_dir();
    if !claude_exists && !codex_exists {
        return Err(format!(
            "No Claude or Codex history source was found (checked {} and {}).",
            paths.claude.root.display(),
            paths.codex.root.display()
        ));
    }
    cleanup_legacy_cache(app);
    let cache = cache_file(app);
    let reporter = ProgressReporter::new(app.clone());
    tauri::async_runtime::spawn_blocking(move || {
        indexer::build_with_progress(&paths, cache.as_deref(), force, &|progress| {
            reporter.report(progress)
        })
    })
    .await
    .map_err(|error| format!("Index build task failed: {error}"))
}

/// 确保索引已构建（懒加载）。并发调用共享同一次构建，而不是各自重建。
async fn ensure_index(state: &AppState, app: &AppHandle) -> Result<Arc<AppIndex>, String> {
    if let Some(index) = state.index.read().await.as_ref() {
        return Ok(Arc::clone(index));
    }
    let _build = state.build_lock.lock().await;
    if let Some(index) = state.index.read().await.as_ref() {
        return Ok(Arc::clone(index));
    }
    let index = Arc::new(build_index(app, false).await?);
    *state.index.write().await = Some(Arc::clone(&index));
    Ok(index)
}

/// 在索引快照上执行只读闭包
async fn read_index<F, R>(state: &AppState, app: &AppHandle, f: F) -> Result<R, String>
where
    F: FnOnce(&AppIndex) -> R,
{
    let index = ensure_index(state, app).await?;
    Ok(f(&index))
}

fn index_meta(index: &AppIndex) -> IndexMeta {
    IndexMeta {
        built_at: index.built_at,
        from_cache: index.from_cache,
        source_files: index.source_files,
        reparsed_files: index.reparsed_files,
    }
}

fn sort_prompts(v: &mut [PromptEntry], sort: Option<&str>) {
    match sort {
        Some("oldest") => v.sort_by_key(|entry| entry.timestamp),
        Some("longest") => v.sort_by_key(|entry| Reverse(entry.char_count)),
        _ => v.sort_by_key(|entry| Reverse(entry.timestamp)),
    }
}

/// 文件夹（项目）列表
#[tauri::command]
pub async fn get_projects(
    agent_filter: Option<AgentFilter>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<Vec<ProjectInfo>, String> {
    let filter = agent_filter.unwrap_or_default();
    read_index(&state, &app, |idx| idx.projects_for(filter).to_vec()).await
}

/// 指定文件夹下的 prompt 列表
#[tauri::command]
pub async fn get_project_prompts(
    project: String,
    sort: Option<String>,
    include_commands: Option<bool>,
    agent_filter: Option<AgentFilter>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<Vec<PromptEntry>, String> {
    let inc = include_commands.unwrap_or(true);
    let filter = agent_filter.unwrap_or_default();
    read_index(&state, &app, |idx| {
        let mut v: Vec<PromptEntry> = idx
            .prompts
            .iter()
            .filter(|p| p.project == project)
            .filter(|p| filter.includes(p.agent))
            .filter(|p| inc || !p.is_command)
            .cloned()
            .collect();
        sort_prompts(&mut v, sort.as_deref());
        v
    })
    .await
}

/// 全局最近的 prompt（已按时间倒序）
#[tauri::command]
pub async fn get_recent_prompts(
    limit: Option<usize>,
    include_commands: Option<bool>,
    agent_filter: Option<AgentFilter>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<Vec<PromptEntry>, String> {
    let lim = limit.unwrap_or(30);
    let inc = include_commands.unwrap_or(true);
    let filter = agent_filter.unwrap_or_default();
    read_index(&state, &app, |idx| {
        idx.prompts
            .iter()
            .filter(|p| filter.includes(p.agent))
            .filter(|p| inc || !p.is_command)
            .take(lim)
            .cloned()
            .collect()
    })
    .await
}

/// 模糊搜索（全局 / 文件夹内）
#[tauri::command]
pub async fn search_prompts(
    query: String,
    project_filter: Option<String>,
    include_commands: Option<bool>,
    agent_filter: Option<AgentFilter>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<Vec<SearchResult>, String> {
    let inc = include_commands.unwrap_or(true);
    let filter = agent_filter.unwrap_or_default();
    read_index(&state, &app, |idx| {
        indexer::search(&idx.prompts, &query, project_filter.as_deref(), inc, filter)
    })
    .await
}

/// 全文搜索会话内容：并行流式扫描会话文件，覆盖用户消息、助手回复、思考与工具调用参数。
#[tauri::command]
pub async fn search_conversations(
    query: String,
    agent_filter: Option<AgentFilter>,
    project_filter: Option<String>,
    limit: Option<usize>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<ConversationSearchResponse, String> {
    let index = ensure_index(&state, &app).await?;
    let filter = agent_filter.unwrap_or_default();
    tauri::async_runtime::spawn_blocking(move || {
        content_search::search_index(&index, &query, filter, project_filter.as_deref(), limit)
    })
    .await
    .map_err(|error| format!("Conversation search task failed: {error}"))
}

/// 把可选的 YYYY-MM-DD 起止日期解析为本地时区的毫秒闭区间；两端都为空表示不限范围。
fn parse_range(start: Option<&str>, end: Option<&str>) -> Result<Option<(i64, i64)>, String> {
    let start = start.map(str::trim).filter(|value| !value.is_empty());
    let end = end.map(str::trim).filter(|value| !value.is_empty());
    if start.is_none() && end.is_none() {
        return Ok(None);
    }
    let start_ms = match start {
        Some(value) => {
            export::day_start_ms(value).ok_or_else(|| format!("起始日期无法解析：{value}"))?
        }
        None => i64::MIN,
    };
    let end_ms = match end {
        Some(value) => {
            export::day_end_ms(value).ok_or_else(|| format!("结束日期无法解析：{value}"))?
        }
        None => i64::MAX,
    };
    if start_ms > end_ms {
        return Err("起始日期不能晚于结束日期。".to_string());
    }
    Ok(Some((start_ms, end_ms)))
}

/// 统计信息。不传日期时返回预计算的全量统计；传了日期则按范围即时计算。
#[tauri::command]
pub async fn get_stats(
    agent_filter: Option<AgentFilter>,
    start_date: Option<String>,
    end_date: Option<String>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<AppStats, String> {
    let filter = agent_filter.unwrap_or_default();
    let range = parse_range(start_date.as_deref(), end_date.as_deref())?;
    read_index(&state, &app, |idx| match range {
        None => idx.stats_for(filter).clone(),
        Some((start_ms, end_ms)) => indexer::compute_stats_in_range(idx, filter, start_ms, end_ms),
    })
    .await
}

/// 指定文件夹下的会话列表
#[tauri::command]
pub async fn get_project_sessions(
    project: String,
    agent_filter: Option<AgentFilter>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<Vec<SessionSummary>, String> {
    let filter = agent_filter.unwrap_or_default();
    read_index(&state, &app, |idx| {
        let mut v: Vec<SessionSummary> = idx
            .sessions
            .iter()
            .filter(|s| s.project == project)
            .filter(|s| filter.includes(s.agent))
            .cloned()
            .collect();
        v.sort_by_key(|session| Reverse(session.started_at));
        v
    })
    .await
}

/// 按 (agent, sessionId) 找到对话文件路径
fn lookup_session_file(index: &AppIndex, agent: Agent, session_id: &str) -> Result<String, String> {
    index
        .session_files
        .get(&(agent, session_id.to_string()))
        .cloned()
        .ok_or_else(|| format!("Conversation not found: {}:{session_id}", agent.as_str()))
}

/// 解析会话详情并附上索引中归属该会话的用量。`block_limit` 为 None 时不截断任何内容块。
async fn load_conversation(
    state: &AppState,
    app: &AppHandle,
    agent: Agent,
    session_id: &str,
    block_limit: parser::BlockLimit,
) -> Result<ConversationDetail, String> {
    let index = ensure_index(state, app).await?;
    let projects_dir = resolve_data_paths(app)?.claude.projects;
    let file = lookup_session_file(&index, agent, session_id)?;
    let mut detail = parse_detail(agent, file, block_limit, projects_dir).await?;
    detail.usage = index
        .session_usage
        .get(&(agent, session_id.to_string()))
        .cloned()
        .unwrap_or_default();
    Ok(detail)
}

/// 同步解析一个会话文件；单会话详情、单会话导出与批量导出共用。
/// Claude 会话随后按本机 projects 目录重新定位持久化的超大工具输出：
/// 展示路径（有截断上限）只标记可读取；导出路径（不截断）把完整内容内联进正文。
fn parse_detail_sync(
    agent: Agent,
    file: &Path,
    block_limit: parser::BlockLimit,
    projects_dir: &Path,
) -> Option<ConversationDetail> {
    let mut detail = match agent {
        Agent::Claude => parser::parse_conversation_detail_with_limit(file, block_limit),
        Agent::Codex => codex_parser::parse_rollout_detail_with_limit(file, block_limit),
    }?;
    if agent == Agent::Claude {
        resolve_persisted_outputs(&mut detail, projects_dir, file, block_limit.is_none());
    }
    Some(detail)
}

/// 把解析层记下的原始路径换成本机 projects 目录下的相对路径；找不到文件时清掉标记，
/// 只保留原预览。`inline` 为 true（导出）时用文件的完整内容替换预览正文。
fn resolve_persisted_outputs(
    detail: &mut ConversationDetail,
    projects_dir: &Path,
    session_file: &Path,
    inline: bool,
) {
    for message in &mut detail.messages {
        for block in &mut message.blocks {
            let Some(raw) = block.persisted_output.take() else {
                continue;
            };
            let Some((path, relative)) = persisted::resolve(&raw.path, projects_dir, session_file)
            else {
                continue;
            };
            let mut size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            let mut inlined = false;
            if inline {
                if let Ok((text, _, file_size)) =
                    persisted::read(projects_dir, &relative, EXPORT_PERSISTED_MAX_BYTES)
                {
                    block.text = Some(text);
                    block.truncated = false;
                    size = file_size;
                    inlined = true;
                }
            }
            block.persisted_output = Some(PersistedOutput {
                path: relative,
                size,
                inlined,
            });
        }
    }
}

/// 在阻塞线程池中解析单个会话文件的完整内容
async fn parse_detail(
    agent: Agent,
    file: String,
    block_limit: parser::BlockLimit,
    projects_dir: PathBuf,
) -> Result<ConversationDetail, String> {
    tauri::async_runtime::spawn_blocking(move || {
        parse_detail_sync(agent, Path::new(&file), block_limit, &projects_dir)
    })
    .await
    .map_err(|error| error.to_string())?
    .ok_or_else(|| "对话文件解析失败".to_string())
}

/// 读取详情页里某个持久化的超大工具输出。`path` 是详情返回的相对路径，
/// 后端再次校验它落在 projects 目录之内。
#[tauri::command]
pub async fn read_persisted_output(
    path: String,
    app: AppHandle,
) -> Result<PersistedOutputText, String> {
    let projects_dir = resolve_data_paths(&app)?.claude.projects;
    tauri::async_runtime::spawn_blocking(move || {
        persisted::read(&projects_dir, &path, DISPLAY_PERSISTED_MAX_BYTES).map(
            |(text, truncated, size)| PersistedOutputText {
                text,
                truncated,
                size,
            },
        )
    })
    .await
    .map_err(|error| error.to_string())?
}

/// 单个会话的完整对话详情
#[tauri::command]
pub async fn get_conversation(
    session_id: String,
    agent: Option<Agent>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<ConversationDetail, String> {
    let agent = agent.unwrap_or(Agent::Claude);
    load_conversation(
        &state,
        &app,
        agent,
        &session_id,
        parser::DISPLAY_BLOCK_LIMIT,
    )
    .await
}

/// 索引元信息
#[tauri::command]
pub async fn get_index_meta(
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<IndexMeta, String> {
    read_index(&state, &app, index_meta).await
}

/// 重建索引。默认增量：只重解析指纹变化的文件；`full=true` 时忽略缓存全量重解析。
/// 重建期间旧索引继续可用，完成后原子替换。
#[tauri::command]
pub async fn refresh_index(
    full: Option<bool>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<IndexMeta, String> {
    let _build = state.build_lock.lock().await;
    let index = build_index(&app, full.unwrap_or(false)).await?;
    let meta = index_meta(&index);
    *state.index.write().await = Some(Arc::new(index));
    Ok(meta)
}

// ----------------------------- 设置 -----------------------------

/// 由设置内容组装 SettingsView（含解析后的路径与存在性）。
fn settings_view(s: &SettingsInput, config_path: &Path) -> Result<SettingsView, String> {
    let paths = resolve_from_settings(s)?;
    Ok(SettingsView {
        claude_data_dir: s.claude_data_dir.clone(),
        codex_data_dir: s.codex_data_dir.clone(),
        history_file: s.history_file.clone(),
        projects_dir: s.projects_dir.clone(),
        sessions_dir: s.sessions_dir.clone(),
        import_path_mappings: s.import_path_mappings.clone(),
        config_path: config_path.to_string_lossy().to_string(),
        resolved: ResolvedPaths {
            claude: ResolvedClaudePaths {
                history: paths.claude.history.to_string_lossy().to_string(),
                projects: paths.claude.projects.to_string_lossy().to_string(),
                sessions: paths.claude.sessions.to_string_lossy().to_string(),
                history_exists: paths.claude.history.is_file(),
                projects_exists: paths.claude.projects.is_dir(),
                sessions_exists: paths.claude.sessions.is_dir(),
            },
            codex: ResolvedCodexPaths {
                root: paths.codex.root.to_string_lossy().to_string(),
                history: paths.codex.history.to_string_lossy().to_string(),
                sessions: paths.codex.sessions.to_string_lossy().to_string(),
                archived_sessions: paths.codex.archived_sessions.to_string_lossy().to_string(),
                root_exists: paths.codex.root.is_dir(),
                history_exists: paths.codex.history.is_file(),
                sessions_exists: paths.codex.sessions.is_dir(),
                archived_sessions_exists: paths.codex.archived_sessions.is_dir(),
            },
        },
    })
}

/// 读取当前设置（含实际生效的配置文件路径与解析结果）
#[tauri::command]
pub fn get_settings(app: AppHandle) -> Result<SettingsView, String> {
    let (s, path) = load_settings(&app);
    settings_view(&s, &path)
}

/// 保存设置并使索引失效（下次查询时按新数据源懒重建）
#[tauri::command]
pub async fn set_settings(
    settings: SettingsInput,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<SettingsView, String> {
    let path = state::save_settings(&app, &settings)?;
    *state.index.write().await = None;
    settings_view(&settings, &path)
}

// ----------------------------- 导出 -----------------------------

/// 按日期范围导出 prompt。
/// write=false 仅生成预览与统计；write=true 额外把完整 Markdown 写入 ~/Downloads。
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn build_prompt_export(
    start_date: String,
    end_date: String,
    project: Option<String>,
    include_commands: bool,
    group_by: Option<String>,
    write: bool,
    lang: Option<String>,
    agent_filter: Option<AgentFilter>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<ExportResult, String> {
    let start_ms = export::day_start_ms(&start_date)
        .ok_or_else(|| format!("起始日期无法解析：{start_date}"))?;
    let end_ms =
        export::day_end_ms(&end_date).ok_or_else(|| format!("结束日期无法解析：{end_date}"))?;
    if start_ms > end_ms {
        return Err("起始日期不能晚于结束日期。".to_string());
    }
    let group = group_by.unwrap_or_else(|| "project".to_string());
    let lang = Lang::from_opt(lang.as_deref());
    let filter = agent_filter.unwrap_or_default();

    let data = read_index(&state, &app, |idx| {
        export::build(
            &idx.prompts,
            &ExportParams {
                start_ms,
                end_ms,
                project: project.as_deref(),
                include_commands,
                group_by: &group,
                start_date: &start_date,
                end_date: &end_date,
                lang,
                agent_filter: filter,
            },
        )
    })
    .await?;

    let mut path: Option<String> = None;
    if write {
        if data.prompt_count == 0 {
            return Err("该范围内没有可导出的 prompt。".to_string());
        }
        let base = format!("Coding-Agent-Prompts_{start_date}_{end_date}");
        let target = unique_export_path(&base);
        std::fs::write(&target, &data.markdown).map_err(|e| format!("写入文件失败：{e}"))?;
        path = Some(target.to_string_lossy().to_string());
    }

    Ok(ExportResult {
        preview: data.preview(),
        path,
        prompt_count: data.prompt_count,
        folder_count: data.folder_count,
        day_count: data.day_count,
    })
}

/// 把当前搜索命中的全部 prompt 导出为 Markdown（按文件夹分组）。
/// write=false 仅生成预览与统计；write=true 额外写入 ~/Downloads。
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn export_search_results(
    query: String,
    project_filter: Option<String>,
    include_commands: bool,
    write: bool,
    lang: Option<String>,
    agent_filter: Option<AgentFilter>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<ExportResult, String> {
    let lang = Lang::from_opt(lang.as_deref());
    let filter = agent_filter.unwrap_or_default();
    let data = read_index(&state, &app, |idx| {
        let results = indexer::search(
            &idx.prompts,
            &query,
            project_filter.as_deref(),
            include_commands,
            filter,
        );
        let items: Vec<&PromptEntry> = results.iter().map(|r| &r.entry).collect();
        export::build_search_export(&items, &query, project_filter.as_deref(), lang)
    })
    .await?;

    let mut path: Option<String> = None;
    if write {
        if data.prompt_count == 0 {
            return Err("没有可导出的搜索结果。".to_string());
        }
        let date = chrono::Local::now().format("%Y-%m-%d");
        let base = format!(
            "Coding-Agent-Search_{}_{date}",
            sanitize_for_filename(&query)
        );
        let target = unique_export_path(&base);
        std::fs::write(&target, &data.markdown).map_err(|e| format!("写入文件失败：{e}"))?;
        path = Some(target.to_string_lossy().to_string());
    }

    Ok(ExportResult {
        preview: data.preview(),
        path,
        prompt_count: data.prompt_count,
        folder_count: data.folder_count,
        day_count: data.day_count,
    })
}

/// 把搜索词压成安全的文件名片段：保留字母数字与 CJK，其余替换为 '-'，最长 24 字符。
fn sanitize_for_filename(q: &str) -> String {
    let cleaned: String = q
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    let trimmed: String = cleaned.trim_matches('-').chars().take(24).collect();
    if trimmed.is_empty() {
        "query".to_string()
    } else {
        trimmed
    }
}

/// 导出单个会话的完整对话为 Markdown（解析不截断，导出的是完整内容）。
/// write=false 仅生成预览；write=true 额外写入 ~/Downloads。
#[tauri::command]
pub async fn export_conversation(
    session_id: String,
    agent: Option<Agent>,
    include_tools: bool,
    write: bool,
    lang: Option<String>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<ConversationExportResult, String> {
    let agent = agent.unwrap_or(Agent::Claude);
    let detail = load_conversation(&state, &app, agent, &session_id, None).await?;
    let lang = Lang::from_opt(lang.as_deref());
    let markdown = export::build_conversation_markdown(&detail, include_tools, lang);

    let mut path: Option<String> = None;
    if write {
        let short_id: String = session_id.chars().take(8).collect();
        let date = chrono::Local::now().format("%Y-%m-%d");
        let base = format!("{}-Conversation_{short_id}_{date}", agent.as_str());
        let target = unique_export_path(&base);
        std::fs::write(&target, &markdown).map_err(|e| format!("写入文件失败：{e}"))?;
        path = Some(target.to_string_lossy().to_string());
    }

    Ok(ConversationExportResult {
        preview: export::truncate_preview(&markdown, lang),
        path,
        message_count: detail.messages.len(),
    })
}

/// 批量导出多个会话。merge=true 合成一份 Markdown；否则在下载目录下新建文件夹，
/// 每个会话一个文件并附 index.md。解析走不截断路径，在阻塞线程池中并行进行，
/// 通过 `export-progress` 事件汇报进度。总是写文件，没有预览模式。
#[tauri::command]
pub async fn export_sessions(
    project: String,
    sessions: Vec<SessionRef>,
    merge: bool,
    include_tools: bool,
    lang: Option<String>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<SessionsExportResult, String> {
    if sessions.is_empty() {
        return Err("没有选中任何会话。".to_string());
    }
    let lang = Lang::from_opt(lang.as_deref());
    let index = ensure_index(&state, &app).await?;

    // 先解析出全部文件路径和列表标题：选择集过期时在写任何东西之前就失败。
    let mut jobs: Vec<(SessionRef, String, String)> = Vec::with_capacity(sessions.len());
    for session in sessions {
        let file = lookup_session_file(&index, session.agent, &session.session_id)?;
        let title = index
            .sessions
            .iter()
            .find(|s| s.agent == session.agent && s.session_id == session.session_id)
            .map(|s| s.title.clone())
            .unwrap_or_default();
        jobs.push((session, file, title));
    }
    let total = jobs.len();
    let projects_dir = resolve_data_paths(&app)?.claude.projects;
    let reporter = ThrottledEmitter::new(app.clone(), EXPORT_PROGRESS_EVENT);
    reporter.emit(ExportProgress { done: 0, total }, true);

    let items = tauri::async_runtime::spawn_blocking(move || {
        let done = AtomicUsize::new(0);
        let parsed: Vec<Result<export::SessionExportItem, String>> = jobs
            .par_iter()
            .map(|(session, file, title)| {
                let detail = parse_detail_sync(session.agent, Path::new(file), None, &projects_dir);
                let finished = done.fetch_add(1, Ordering::Relaxed) + 1;
                reporter.emit(
                    ExportProgress {
                        done: finished,
                        total,
                    },
                    finished == total,
                );
                let mut detail = detail.ok_or_else(|| {
                    format!(
                        "对话文件解析失败：{}:{}",
                        session.agent.as_str(),
                        session.session_id
                    )
                })?;
                detail.usage = index
                    .session_usage
                    .get(&(session.agent, session.session_id.clone()))
                    .cloned()
                    .unwrap_or_default();
                Ok(export::SessionExportItem {
                    detail,
                    title: title.clone(),
                })
            })
            .collect();
        parsed.into_iter().collect::<Result<Vec<_>, String>>()
    })
    .await
    .map_err(|error| format!("Export task failed: {error}"))??;

    let session_count = items.len();
    let message_count: usize = items.iter().map(|item| item.detail.messages.len()).sum();
    let params = export::SessionsExportParams {
        project: &project,
        include_tools,
        lang,
    };
    let folder_name = export::filename_fragment(&indexer::project_name(&project), 40);
    let base = format!(
        "{}_Sessions_{}",
        if folder_name.is_empty() {
            "Coding-Agent".to_string()
        } else {
            folder_name
        },
        chrono::Local::now().format("%Y-%m-%d")
    );

    if merge {
        let markdown = export::build_sessions_merged(&items, &params);
        let target = unique_export_path(&base);
        std::fs::write(&target, markdown).map_err(|e| format!("写入文件失败：{e}"))?;
        return Ok(SessionsExportResult {
            path: target.to_string_lossy().to_string(),
            merged: true,
            session_count,
            message_count,
            file_count: 1,
        });
    }

    let folder = export::build_sessions_folder(&items, &params);
    let target = unique_export_dir(&base);
    std::fs::create_dir_all(&target).map_err(|e| format!("创建文件夹失败：{e}"))?;
    for file in &folder.files {
        std::fs::write(target.join(&file.file_name), &file.markdown)
            .map_err(|e| format!("写入文件失败：{e}"))?;
    }
    std::fs::write(target.join("index.md"), &folder.index_markdown)
        .map_err(|e| format!("写入文件失败：{e}"))?;
    Ok(SessionsExportResult {
        path: target.to_string_lossy().to_string(),
        merged: false,
        session_count,
        message_count,
        file_count: folder.files.len() + 1,
    })
}

// ----------------------------- 导入 Claude Code 会话 -----------------------------

/// 第一步（只读）：列出 zip 里的云端项目、会话数与映射建议。
/// 建议来自设置里记住的映射，其次是本机已索引的同名 Claude 项目；索引不可用时没有建议。
#[tauri::command]
pub async fn inspect_session_import(
    zip_path: String,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<ImportInspection, String> {
    let (settings, _) = load_settings(&app);
    let candidates: Vec<(String, String)> = match ensure_index(&state, &app).await {
        Ok(index) => index
            .projects_for(AgentFilter::Claude)
            .iter()
            .map(|project| (project.name.clone(), project.path.clone()))
            .collect(),
        Err(_) => Vec::new(),
    };
    tauri::async_runtime::spawn_blocking(move || {
        import::inspect(
            Path::new(&zip_path),
            &settings.import_path_mappings,
            &candidates,
        )
    })
    .await
    .map_err(|error| format!("Import inspection task failed: {error}"))?
}

/// 第二步（只读）：按映射生成导入计划，逐会话给出新增 / 更新 / 跳过 / 冲突。
#[tauri::command]
pub async fn plan_session_import(
    zip_path: String,
    mappings: Vec<ProjectMapping>,
    app: AppHandle,
) -> Result<ImportPlan, String> {
    let projects_dir = resolve_data_paths(&app)?.claude.projects;
    tauri::async_runtime::spawn_blocking(move || {
        import::plan(Path::new(&zip_path), &projects_dir, &mappings)
    })
    .await
    .map_err(|error| format!("Import planning task failed: {error}"))?
}

/// 第三步：按计划与冲突决定写入 projects 目录，记住映射，然后增量重建索引。
/// 写入前重新校验 zip 与计划一致；冲突缺少决定时不写任何文件。
#[tauri::command]
pub async fn apply_session_import(
    zip_path: String,
    mappings: Vec<ProjectMapping>,
    decisions: Vec<ConflictDecision>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<ImportResult, String> {
    let projects_dir = resolve_data_paths(&app)?.claude.projects;
    let mapping_copy = mappings.clone();
    let outcome = tauri::async_runtime::spawn_blocking(move || {
        import::apply(
            Path::new(&zip_path),
            &projects_dir,
            &mapping_copy,
            &decisions,
        )
    })
    .await
    .map_err(|error| format!("Import task failed: {error}"))??;

    // 记住这次确认的映射（归一化后的本机路径）：选了本机目录的写入，选了保持原路径的忘掉旧记录
    let normalized = import::normalize_mappings(&mappings)?;
    let (mut settings, _) = load_settings(&app);
    let mut changed = false;
    for mapping in &mappings {
        let key = mapping
            .cloud_cwd
            .trim()
            .trim_end_matches(['/', '\\'])
            .to_string();
        if key.is_empty() {
            continue;
        }
        match normalized.get(&key) {
            Some(local) => {
                changed |= settings
                    .import_path_mappings
                    .insert(key, local.clone())
                    .as_ref()
                    != Some(local);
            }
            None => changed |= settings.import_path_mappings.remove(&key).is_some(),
        }
    }
    if changed {
        state::save_settings(&app, &settings)?;
    }

    let _build = state.build_lock.lock().await;
    let index = build_index(&app, false).await?;
    let meta = index_meta(&index);
    *state.index.write().await = Some(Arc::new(index));
    Ok(ImportResult {
        added: outcome.added,
        updated: outcome.updated,
        skipped: outcome.skipped,
        kept_local: outcome.kept_local,
        overwritten: outcome.overwritten,
        files_written: outcome.files_written,
        skipped_entries: outcome.skipped_entries,
        projects: outcome.projects,
        index: meta,
    })
}

/// 在系统文件管理器中定位某个文件或文件夹（macOS：Finder 选中）。
#[tauri::command]
pub fn reveal_path(path: String) -> Result<(), String> {
    let p = PathBuf::from(&path);
    if !p.exists() {
        return Err("文件不存在或已被移动。".to_string());
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-R")
            .arg(&p)
            .spawn()
            .map_err(|e| format!("无法打开 Finder：{e}"))?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg("/select,")
            .arg(&p)
            .spawn()
            .map_err(|e| format!("无法打开资源管理器：{e}"))?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let dir = p.parent().unwrap_or(&p);
        std::process::Command::new("xdg-open")
            .arg(dir)
            .spawn()
            .map_err(|e| format!("无法打开文件管理器：{e}"))?;
    }
    Ok(())
}

/// 下载目录下生成不冲突的导出文件路径：base.md → base (2).md → …
fn unique_export_path(base: &str) -> PathBuf {
    unique_export_target(base, ".md")
}

/// 下载目录下生成不冲突的导出文件夹路径：base → base (2) → …
fn unique_export_dir(base: &str) -> PathBuf {
    unique_export_target(base, "")
}

fn unique_export_target(base: &str, extension: &str) -> PathBuf {
    let dir = dirs::download_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."));
    let mut candidate = dir.join(format!("{base}{extension}"));
    let mut n = 2;
    while candidate.exists() {
        candidate = dir.join(format!("{base} ({n}){extension}"));
        n += 1;
    }
    candidate
}
