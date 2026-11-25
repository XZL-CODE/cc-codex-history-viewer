//! 应用内更新：读取本仓库 GitHub Release 上的 `latest.json`，下载更新包，用内置公钥核对签名后安装。
//!
//! 应用只在这里联网，请求只取版本清单和更新包，不带任何历史内容。检查到的更新先记下来，用户点
//! 「立即更新」时才下载安装；下载进度通过 `update-progress` 事件汇报。
//!
//! 清单的 `history` 是发版工作流写入的最近几版更新说明（新的在前），用来列出用户跳过的版本；
//! 更新组件本身不读这个字段。

use semver::Version;
use serde::{Deserialize, Serialize};
use serde_json::Value;
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
    /// 比当前版本新的各版更新说明，新的在前，第一项是最新版；清单没带历史时只有最新版
    pub releases: Vec<ReleaseNotes>,
}

/// 一个版本的更新说明，即该版 Release 正文
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseNotes {
    pub version: String,
    /// 发布时间（RFC 3339），清单里是 `pub_date`
    #[serde(alias = "pub_date")]
    pub date: Option<String>,
    pub notes: Option<String>,
}

/// 用户跳过的各版更新说明：最新版用清单正文，再接上 `history` 里介于当前版与最新版之间的版本，
/// 新的在前。历史缺失、格式不对或版本号解析不了的条目直接略过，不影响检查结果。
fn missed_releases(
    raw_json: &Value,
    current_version: &str,
    latest: ReleaseNotes,
) -> Vec<ReleaseNotes> {
    let parse = |version: &str| Version::parse(version.trim().trim_start_matches('v')).ok();
    let (Some(current), Some(newest)) = (parse(current_version), parse(&latest.version)) else {
        return vec![latest];
    };
    let mut older: Vec<(Version, ReleaseNotes)> = raw_json
        .get("history")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| serde_json::from_value::<ReleaseNotes>(entry.clone()).ok())
        .filter_map(|release| {
            let version = parse(&release.version)?;
            (version > current && version < newest).then_some((version, release))
        })
        .collect();
    older.sort_by(|a, b| b.0.cmp(&a.0));
    older.dedup_by(|a, b| a.0 == b.0);

    let mut releases = vec![latest];
    releases.extend(older.into_iter().map(|(_, release)| release));
    releases
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
    let info = update.as_ref().map(|update| {
        let date = update
            .raw_json
            .get("pub_date")
            .and_then(|date| date.as_str())
            .map(str::to_string);
        let latest = ReleaseNotes {
            version: update.version.clone(),
            date: date.clone(),
            notes: update.body.clone(),
        };
        UpdateInfo {
            version: update.version.clone(),
            current_version: update.current_version.clone(),
            date,
            releases: missed_releases(&update.raw_json, &update.current_version, latest),
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn latest(version: &str) -> ReleaseNotes {
        ReleaseNotes {
            version: version.into(),
            date: Some("2026-11-25T00:00:00Z".into()),
            notes: Some(format!("notes {version}")),
        }
    }

    fn versions(releases: &[ReleaseNotes]) -> Vec<&str> {
        releases
            .iter()
            .map(|release| release.version.as_str())
            .collect()
    }

    #[test]
    fn lists_versions_between_current_and_latest_newest_first() {
        let manifest = json!({
            "version": "0.18.0",
            "history": [
                { "version": "0.18.0", "pub_date": "2026-11-25T00:00:00Z", "notes": "from history" },
                { "version": "0.16.0", "pub_date": "2026-11-21T00:00:00Z", "notes": "notes 0.16.0" },
                { "version": "0.17.0", "pub_date": "2026-11-23T00:00:00Z", "notes": "notes 0.17.0" },
                { "version": "0.15.0", "pub_date": "2026-11-20T00:00:00Z", "notes": "notes 0.15.0" },
                { "version": "0.14.0", "pub_date": "2026-11-19T00:00:00Z", "notes": "notes 0.14.0" }
            ]
        });
        let releases = missed_releases(&manifest, "0.15.0", latest("0.18.0"));
        assert_eq!(versions(&releases), ["0.18.0", "0.17.0", "0.16.0"]);
        // 最新版用清单正文，不用 history 里的副本
        assert_eq!(releases[0].notes.as_deref(), Some("notes 0.18.0"));
        assert_eq!(releases[1].date.as_deref(), Some("2026-11-23T00:00:00Z"));
    }

    #[test]
    fn compares_versions_as_semver() {
        let manifest = json!({
            "history": [
                { "version": "v0.10.0", "notes": "tag spelling" },
                { "version": "0.9.0", "notes": "older" },
                { "version": "0.10.0", "notes": "duplicate" }
            ]
        });
        let releases = missed_releases(&manifest, "0.9.0", latest("0.11.0"));
        assert_eq!(versions(&releases), ["0.11.0", "v0.10.0"]);
    }

    #[test]
    fn falls_back_to_latest_notes_without_usable_history() {
        for manifest in [
            json!({ "version": "0.16.0" }),
            json!({ "history": "not a list" }),
            json!({ "history": [{ "notes": "no version" }, { "version": "next", "notes": "bad" }] }),
        ] {
            let releases = missed_releases(&manifest, "0.15.0", latest("0.16.0"));
            assert_eq!(releases, [latest("0.16.0")]);
        }
        let manifest = json!({ "history": [{ "version": "0.15.5" }] });
        assert_eq!(
            missed_releases(&manifest, "not semver", latest("0.16.0")),
            [latest("0.16.0")]
        );
    }
}
