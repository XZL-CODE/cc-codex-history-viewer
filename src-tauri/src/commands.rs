//! 暴露给前端的 Tauri Commands。
//!
//! 所有会触碰索引或会话文件的 command 都是 async：索引构建与会话解析在 Tauri 的阻塞线程池中
//! 执行，不会卡住主线程；构建期间通过 `index-progress` 事件向前端汇报进度。

use crate::content_search;
use crate::export::{self, ExportParams, Lang};
use crate::indexer::{self, AppIndex};
use crate::models::*;
use crate::state::{self, load_settings, resolve_data_paths, resolve_from_settings, AppState};
use crate::{codex_parser, parser};
use std::cmp::Reverse;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};

/// Event name for index build progress; the payload is an [`IndexProgress`].
pub const INDEX_PROGRESS_EVENT: &str = "index-progress";

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

/// Throttled progress emitter shared by the parser worker threads.
struct ProgressReporter {
    app: AppHandle,
    last_emit: Mutex<Option<Instant>>,
}

impl ProgressReporter {
    const MIN_INTERVAL: Duration = Duration::from_millis(80);

    fn new(app: AppHandle) -> Self {
        Self {
            app,
            last_emit: Mutex::new(None),
        }
    }

    fn report(&self, progress: IndexProgress) {
        let boundary = progress.phase != IndexPhase::Parsing
            || progress.done == 0
            || progress.done == progress.total;
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
        let _ = self.app.emit(INDEX_PROGRESS_EVENT, progress);
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

/// 解析会话详情并附上索引中归属该会话的用量。
async fn load_conversation(
    state: &AppState,
    app: &AppHandle,
    agent: Agent,
    session_id: &str,
) -> Result<ConversationDetail, String> {
    let index = ensure_index(state, app).await?;
    let file = lookup_session_file(&index, agent, session_id)?;
    let mut detail = parse_detail(agent, file).await?;
    detail.usage = index
        .session_usage
        .get(&(agent, session_id.to_string()))
        .cloned()
        .unwrap_or_default();
    Ok(detail)
}

/// 在阻塞线程池中解析单个会话文件的完整内容
async fn parse_detail(agent: Agent, file: String) -> Result<ConversationDetail, String> {
    tauri::async_runtime::spawn_blocking(move || match agent {
        Agent::Claude => parser::parse_conversation_detail(Path::new(&file)),
        Agent::Codex => codex_parser::parse_rollout_detail(Path::new(&file)),
    })
    .await
    .map_err(|error| error.to_string())?
    .ok_or_else(|| "对话文件解析失败".to_string())
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
    load_conversation(&state, &app, agent, &session_id).await
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

/// 导出单个会话的完整对话为 Markdown。
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
    let detail = load_conversation(&state, &app, agent, &session_id).await?;
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

/// 在系统文件管理器中定位某个文件（macOS：Finder 选中）。
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
    let dir = dirs::download_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."));
    let mut candidate = dir.join(format!("{base}.md"));
    let mut n = 2;
    while candidate.exists() {
        candidate = dir.join(format!("{base} ({n}).md"));
        n += 1;
    }
    candidate
}
