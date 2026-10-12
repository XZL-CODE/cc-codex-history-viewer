//! 应用内更新：读取本仓库 GitHub Release 上的 `latest.json`，下载更新包，用内置公钥核对签名后安装。
//!
//! 应用只在这里联网，请求只取版本清单和更新包，不带任何历史内容。检查到的更新先记下来，用户点
//! 「立即更新」时才下载安装；下载进度通过 `update-progress` 事件汇报。

use serde::Serialize;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_updater::{Update, UpdaterExt};
use tauri_plugin_window_state::{AppHandleExt, StateFlags};

/// Event name for update download progress; the payload is an [`UpdateProgress`].
pub const UPDATE_PROGRESS_EVENT: &str = "update-progress";

/// 取清单的超时：连不上 GitHub 时尽快放弃，不让检查一直挂着。
const CHECK_TIMEOUT: Duration = Duration::from_secs(20);
/// 下载更新包的超时：包有几 MB，网速慢时也要能下完。
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(15 * 60);
/// 下载进度事件的最小间隔
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

/// 检查到、尚未安装的更新
#[derive(Default)]
pub struct PendingUpdate(Mutex<Option<Update>>);

impl PendingUpdate {
    fn lock(&self) -> MutexGuard<'_, Option<Update>> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub current_version: String,
    /// 发布时间（RFC 3339），清单里没有时为空
    pub date: Option<String>,
    /// 更新说明，即 Release 正文
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum UpdatePhase {
    Downloading,
    /// 下载完成、签名已核对，正在安装
    Installing,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProgress {
    pub phase: UpdatePhase,
    pub downloaded: u64,
    /// 服务器没给文件大小时为空
    pub total: Option<u64>,
}

/// 检查有没有比当前更高的版本；有就记下来等用户安装，返回版本信息。
#[tauri::command]
pub async fn check_for_update(
    app: AppHandle,
    pending: State<'_, PendingUpdate>,
) -> Result<Option<UpdateInfo>, String> {
    let update = app
        .updater_builder()
        .timeout(CHECK_TIMEOUT)
        .build()
        .map_err(|error| error.to_string())?
        .check()
        .await
        .map_err(|error| error.to_string())?;
    let info = update.as_ref().map(|update| UpdateInfo {
        version: update.version.clone(),
        current_version: update.current_version.clone(),
        date: update
            .raw_json
            .get("pub_date")
            .and_then(|date| date.as_str())
            .map(str::to_string),
        notes: update.body.clone(),
    });
    *pending.lock() = update;
    Ok(info)
}

/// 下载并安装检查到的更新，然后重启应用。
///
/// 下载后先核对签名，对不上就报错、不安装。Windows 上安装程序启动后应用直接退出，由安装程序装完
/// 再打开；macOS 原地替换应用后在这里重启。开发构建不安装：可执行文件不在应用包里，替换的位置不对。
#[tauri::command]
pub async fn install_update(
    app: AppHandle,
    pending: State<'_, PendingUpdate>,
) -> Result<(), String> {
    if cfg!(debug_assertions) {
        return Err("Updates can only be installed from a packaged release build.".into());
    }
    let mut update = pending
        .lock()
        .clone()
        .ok_or("No update is pending. Check for updates first.")?;
    update.timeout = Some(DOWNLOAD_TIMEOUT);

    let mut downloaded = 0u64;
    let mut last_emit: Option<Instant> = None;
    // download 读完之后才核对签名（下载完成的回调在核对之前），所以「安装中」等它返回再报
    let bytes = update
        .download(
            |chunk, total| {
                downloaded += chunk as u64;
                if last_emit.is_some_and(|at| at.elapsed() < PROGRESS_INTERVAL) {
                    return;
                }
                last_emit = Some(Instant::now());
                let _ = app.emit(
                    UPDATE_PROGRESS_EVENT,
                    UpdateProgress {
                        phase: UpdatePhase::Downloading,
                        downloaded,
                        total,
                    },
                );
            },
            || {},
        )
        .await
        .map_err(|error| error.to_string())?;

    // 安装会结束当前进程，先保存窗口位置与尺寸
    let _ = app.save_window_state(StateFlags::all());
    let _ = app.emit(
        UPDATE_PROGRESS_EVENT,
        UpdateProgress {
            phase: UpdatePhase::Installing,
            downloaded: bytes.len() as u64,
            total: None,
        },
    );
    update.install(bytes).map_err(|error| error.to_string())?;
    app.restart()
}
