//! Runtime state and read-only data-source path resolution.

use crate::indexer::AppIndex;
use crate::models::SettingsInput;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::async_runtime::{Mutex as AsyncMutex, RwLock};
use tauri::{AppHandle, Manager};

pub struct AppState {
    /// The current in-memory index. Readers clone the `Arc` and never wait for a rebuild.
    pub index: RwLock<Option<Arc<AppIndex>>>,
    /// Serializes index builds so concurrent commands share one build instead of racing.
    pub build_lock: AsyncMutex<()>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            index: RwLock::new(None),
            build_lock: AsyncMutex::new(()),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct ClaudeDataPaths {
    pub root: PathBuf,
    pub history: PathBuf,
    pub projects: PathBuf,
    pub sessions: PathBuf,
}

#[derive(Debug, Clone)]
pub struct CodexDataPaths {
    pub root: PathBuf,
    pub history: PathBuf,
    pub sessions: PathBuf,
    pub archived_sessions: PathBuf,
}

#[derive(Debug, Clone)]
pub struct DataPaths {
    pub claude: ClaudeDataPaths,
    pub codex: CodexDataPaths,
}

fn project_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn legacy_settings_path() -> PathBuf {
    project_root().join("settings.json")
}

pub fn config_file_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|dir| dir.join("settings.json"))
        .map_err(|error| format!("Unable to locate the application config directory: {error}"))
}

fn read_settings_file(path: &Path) -> SettingsInput {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Prefer the app config file but retain the original repository-local development fallback.
pub fn load_settings(app: &AppHandle) -> (SettingsInput, PathBuf) {
    let primary = config_file_path(app).ok();
    if let Some(path) = &primary {
        if path.is_file() {
            return (read_settings_file(path), path.clone());
        }
    }
    let legacy = legacy_settings_path();
    if legacy.is_file() {
        return (read_settings_file(&legacy), legacy);
    }
    (SettingsInput::default(), primary.unwrap_or(legacy))
}

pub fn save_settings(app: &AppHandle, settings: &SettingsInput) -> Result<PathBuf, String> {
    let path = config_file_path(app)?;
    if let Some(directory) = path.parent() {
        fs::create_dir_all(directory)
            .map_err(|error| format!("Unable to create the config directory: {error}"))?;
    }
    let file = fs::File::create(&path)
        .map_err(|error| format!("Unable to write the settings file: {error}"))?;
    serde_json::to_writer_pretty(file, settings)
        .map_err(|error| format!("Unable to serialize settings: {error}"))?;
    Ok(path)
}

/// Pure resolver used by tests. Explicit settings win, then CODEX_HOME, then ~/.codex.
pub fn resolve_from_settings_with(
    settings: &SettingsInput,
    codex_home: Option<OsString>,
    home: Option<PathBuf>,
) -> Result<DataPaths, String> {
    let claude_root = if settings.claude_data_dir.trim().is_empty() {
        home.as_ref()
            .map(|path| path.join(".claude"))
            .ok_or_else(|| "Unable to locate ~/.claude; configure claudeDataDir.".to_string())?
    } else {
        PathBuf::from(settings.claude_data_dir.trim())
    };

    let claude_pick = |explicit: &str, child: &str| {
        if explicit.trim().is_empty() {
            claude_root.join(child)
        } else {
            PathBuf::from(explicit.trim())
        }
    };

    let codex_root = if !settings.codex_data_dir.trim().is_empty() {
        PathBuf::from(settings.codex_data_dir.trim())
    } else if let Some(path) = codex_home.filter(|path| !path.is_empty()) {
        PathBuf::from(path)
    } else {
        home.map(|path| path.join(".codex"))
            .ok_or_else(|| "Unable to locate ~/.codex; configure codexDataDir.".to_string())?
    };

    Ok(DataPaths {
        claude: ClaudeDataPaths {
            root: claude_root.clone(),
            history: claude_pick(&settings.history_file, "history.jsonl"),
            projects: claude_pick(&settings.projects_dir, "projects"),
            sessions: claude_pick(&settings.sessions_dir, "sessions"),
        },
        codex: CodexDataPaths {
            history: codex_root.join("history.jsonl"),
            sessions: codex_root.join("sessions"),
            archived_sessions: codex_root.join("archived_sessions"),
            root: codex_root,
        },
    })
}

pub fn resolve_from_settings(settings: &SettingsInput) -> Result<DataPaths, String> {
    resolve_from_settings_with(settings, std::env::var_os("CODEX_HOME"), dirs::home_dir())
}

pub fn resolve_data_paths(app: &AppHandle) -> Result<DataPaths, String> {
    let (settings, _) = load_settings(app);
    resolve_from_settings(&settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_settings_shape_migrates_with_empty_codex_dir() {
        let value = r#"{
          "claudeDataDir": "/synthetic/claude",
          "historyFile": "",
          "projectsDir": "",
          "sessionsDir": ""
        }"#;
        let settings: SettingsInput = serde_json::from_str(value).unwrap();
        assert_eq!(settings.claude_data_dir, "/synthetic/claude");
        assert!(settings.codex_data_dir.is_empty());
    }

    #[test]
    fn codex_root_precedence_is_setting_then_env_then_home() {
        let mut settings = SettingsInput::default();
        let home = Some(PathBuf::from("/synthetic/home"));

        let from_home = resolve_from_settings_with(&settings, None, home.clone()).unwrap();
        assert_eq!(
            from_home.codex.root,
            PathBuf::from("/synthetic/home/.codex")
        );

        let from_env = resolve_from_settings_with(
            &settings,
            Some(OsString::from("/synthetic/env-codex")),
            home.clone(),
        )
        .unwrap();
        assert_eq!(from_env.codex.root, PathBuf::from("/synthetic/env-codex"));

        settings.codex_data_dir = "/synthetic/setting-codex".to_string();
        let explicit = resolve_from_settings_with(
            &settings,
            Some(OsString::from("/synthetic/env-codex")),
            home,
        )
        .unwrap();
        assert_eq!(
            explicit.codex.root,
            PathBuf::from("/synthetic/setting-codex")
        );
    }
}
