//! 从镜像 `~/.claude/projects` 层级的 zip 导入 Claude Code 会话。
//!
//! 流程分两步：`inspect` 与 `plan` 只读 zip 和本机文件，不写任何东西；`apply` 按计划与用户
//! 对冲突的决定一次性写入。写入只发生在 projects 目录下的 `<目录名>/<会话ID>.jsonl` 与
//! `<目录名>/<会话ID>/…`：覆盖同名文件，绝不删除本机文件，绝不写到 projects 目录之外。
//!
//! 一个会话 = jsonl + 同名旁挂目录，作为整体判定：
//! - 本机没有：新增；
//! - 内容完全相同：跳过（只补导入包里多出的旁挂文件）；
//! - 本机 jsonl 是导入 jsonl 的前缀：覆盖更新；
//! - 导入 jsonl 是本机 jsonl 的前缀：跳过（更旧的快照）；
//! - 其余情况，以及 jsonl 相同但旁挂目录里有内容不同的同名文件：冲突，交给用户决定。
//!
//! 云端会话的 cwd 可以映射到本机目录：每行顶层的 `cwd` 字段按前缀改写，其余字节原样保留，
//! 目标目录名按本机路径重新编码，这样 viewer 归到本机项目，本机 `claude --resume` 也能找到。

use crate::indexer::{encode_claude_project_dir, project_name};
use crate::models::*;
use crate::parser;
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use zip::ZipArchive;

/// 单个条目解压后的上限；会话文件远小于此，超出视为异常包。
const MAX_ENTRY_BYTES: u64 = 1 << 30;
/// 旁挂目录里不参与一致性比较的运行时状态文件（仍会随会话一起写入）。
const UNCOMPARED_SIDE_FILES: &[&str] = &["ccr-tip.json"];
/// 压缩工具留下的垃圾文件，任何位置都跳过。
const JUNK_FILES: &[&str] = &[".DS_Store", "Thumbs.db", "desktop.ini"];
/// 计划里展示的标题最多保留的字符数。
const TITLE_MAX_CHARS: usize = 200;

// ----------------------------- 条目分类 -----------------------------

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct SessionKey {
    dir: String,
    session_id: String,
}

#[derive(Debug, Default)]
struct SessionEntries {
    transcript: Option<usize>,
    /// (旁挂目录内的相对路径，zip 条目序号)
    side: Vec<(String, usize)>,
}

enum EntryKind {
    Transcript {
        dir: String,
        session_id: String,
    },
    SideFile {
        dir: String,
        session_id: String,
        rel: String,
    },
    /// 目录条目等无需处理的内容
    Ignored,
    /// projects/<目录名>/ 之外或无法归入会话的条目：跳过并报告
    Skipped,
}

fn reject(name: &str, why: &str) -> String {
    format!("导入包含不安全的条目「{name}」：{why}。已拒绝整个导入包，未写入任何文件。")
}

/// 判定一个 zip 条目：不安全的路径整体拒绝，范围之外的条目跳过。
fn classify(name: &str) -> Result<EntryKind, String> {
    if name.contains('\0') {
        return Err(reject(name, "路径包含空字符"));
    }
    let normalized = name.replace('\\', "/");
    let is_dir_entry = normalized.ends_with('/');
    let raw_parts: Vec<&str> = normalized.split('/').collect();
    let drive_prefixed = raw_parts
        .first()
        .is_some_and(|first| first.len() == 2 && first.ends_with(':'));
    if normalized.starts_with('/') || drive_prefixed {
        return Err(reject(name, "绝对路径"));
    }
    if raw_parts.contains(&"..") {
        return Err(reject(name, "路径包含 .."));
    }
    let parts: Vec<&str> = raw_parts
        .into_iter()
        .filter(|part| !part.is_empty() && *part != ".")
        .collect();
    let Some(first) = parts.first() else {
        return Ok(EntryKind::Ignored);
    };
    if parts.iter().any(|part| JUNK_FILES.contains(part)) || *first == "__MACOSX" {
        return Ok(EntryKind::Skipped);
    }
    if *first != "projects" {
        return Ok(EntryKind::Skipped);
    }
    if is_dir_entry {
        return Ok(EntryKind::Ignored);
    }
    if parts.len() < 3 {
        return Ok(EntryKind::Skipped);
    }
    let dir = parts[1].to_string();
    if parts.len() == 3 {
        return Ok(match parts[2].strip_suffix(".jsonl") {
            Some(stem) if !stem.is_empty() => EntryKind::Transcript {
                dir,
                session_id: stem.to_string(),
            },
            _ => EntryKind::Skipped,
        });
    }
    Ok(EntryKind::SideFile {
        dir,
        session_id: parts[2].to_string(),
        rel: parts[3..].join("/"),
    })
}

fn is_symlink_mode(mode: Option<u32>) -> bool {
    mode.is_some_and(|mode| mode & 0o170000 == 0o120000)
}

// ----------------------------- 顶层字符串字段扫描 -----------------------------

fn skip_ws(bytes: &[u8], mut index: usize) -> usize {
    while index < bytes.len() && matches!(bytes[index], b' ' | b'\t' | b'\r' | b'\n') {
        index += 1;
    }
    index
}

/// 从 `start`（指向开头引号）起找到字符串字面量的结束位置（闭合引号之后的下标）。
fn string_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut index = start + 1;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index += 2,
            b'"' => return Some(index + 1),
            _ => index += 1,
        }
    }
    None
}

/// 在一行 JSON 对象里找到顶层 `key` 的字符串值字面量（含引号）的字节区间。
/// 只看第一层，嵌套对象里的同名键不会命中；值不是字符串时返回 None。
pub(crate) fn top_level_string_member(text: &str, key: &str) -> Option<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut index = skip_ws(bytes, 0);
    if bytes.get(index) != Some(&b'{') {
        return None;
    }
    index += 1;
    let mut depth = 1usize;
    let mut expect_key = true;
    loop {
        index = skip_ws(bytes, index);
        let byte = *bytes.get(index)?;
        match byte {
            b'"' => {
                let end = string_end(bytes, index)?;
                if depth == 1 && expect_key {
                    let key_text = &text[index + 1..end - 1];
                    let colon = skip_ws(bytes, end);
                    if bytes.get(colon) != Some(&b':') {
                        return None;
                    }
                    let value_start = skip_ws(bytes, colon + 1);
                    if key_text == key {
                        return if bytes.get(value_start) == Some(&b'"') {
                            string_end(bytes, value_start).map(|value_end| (value_start, value_end))
                        } else {
                            None
                        };
                    }
                    expect_key = false;
                    index = value_start;
                    continue;
                }
                index = end;
            }
            b'{' | b'[' => {
                depth += 1;
                index += 1;
            }
            b'}' | b']' => {
                depth -= 1;
                if depth == 0 {
                    return None;
                }
                index += 1;
            }
            b',' => {
                if depth == 1 {
                    expect_key = true;
                }
                index += 1;
            }
            _ => index += 1,
        }
    }
}

/// 顶层字符串字段的解码值。
fn top_level_string(text: &str, key: &str) -> Option<String> {
    let (start, end) = top_level_string_member(text, key)?;
    serde_json::from_str(&text[start..end]).ok()
}

// ----------------------------- cwd 改写 -----------------------------

fn trim_trailing_separators(path: &str) -> &str {
    let trimmed = path.trim_end_matches(['/', '\\']);
    if trimmed.is_empty() {
        path
    } else {
        trimmed
    }
}

/// 把 `value` 从云端根目录 `from` 映射到本机目录 `to`：等于根目录或位于其子目录时改写，
/// 子路径的分隔符跟随本机路径的风格；否则返回 None（原样保留）。
pub(crate) fn map_cwd(value: &str, from: &str, to: &str) -> Option<String> {
    let from = trim_trailing_separators(from);
    let to = trim_trailing_separators(to);
    if value == from {
        return Some(to.to_string());
    }
    let rest = value.strip_prefix(from)?;
    if !rest.starts_with(['/', '\\']) {
        return None;
    }
    let separator = if to.contains('\\') && !to.contains('/') {
        '\\'
    } else {
        '/'
    };
    let mapped: String = rest
        .chars()
        .map(|character| {
            if character == '/' || character == '\\' {
                separator
            } else {
                character
            }
        })
        .collect();
    Some(format!("{to}{mapped}"))
}

/// 改写一行的顶层 `cwd`；没有 cwd 或不在映射范围内时返回 None（保留原字节）。
fn rewrite_line(line: &[u8], from: &str, to: &str) -> Option<Vec<u8>> {
    let text = std::str::from_utf8(line).ok()?;
    let (start, end) = top_level_string_member(text, "cwd")?;
    let value: String = serde_json::from_str(&text[start..end]).ok()?;
    let mapped = map_cwd(&value, from, to)?;
    let literal = serde_json::to_string(&mapped).ok()?;
    let mut out = String::with_capacity(text.len() + literal.len());
    out.push_str(&text[..start]);
    out.push_str(&literal);
    out.push_str(&text[end..]);
    Some(out.into_bytes())
}

/// 逐行改写 jsonl 的顶层 cwd，其余字节（含行尾）原样保留，结果是确定性的。
pub(crate) fn rewrite_transcript(bytes: &[u8], from: &str, to: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len() + 64);
    for chunk in bytes.split_inclusive(|byte| *byte == b'\n') {
        let body_end = chunk
            .iter()
            .rposition(|byte| !matches!(byte, b'\n' | b'\r'))
            .map_or(0, |position| position + 1);
        let (body, terminator) = chunk.split_at(body_end);
        match rewrite_line(body, from, to) {
            Some(rewritten) => out.extend_from_slice(&rewritten),
            None => out.extend_from_slice(body),
        }
        out.extend_from_slice(terminator);
    }
    out
}

// ----------------------------- 会话内容摘要 -----------------------------

#[derive(Deserialize)]
struct TitleLine {
    #[serde(rename = "type")]
    kind: Option<String>,
    #[serde(rename = "isSidechain")]
    is_sidechain: Option<bool>,
    message: Option<TitleMessage>,
}

#[derive(Deserialize)]
struct TitleMessage {
    role: Option<String>,
    content: Option<serde_json::Value>,
}

fn lines(bytes: &[u8]) -> impl Iterator<Item = &str> {
    bytes
        .split(|byte| *byte == b'\n')
        .filter_map(|line| std::str::from_utf8(line).ok())
        .map(str::trim)
        .filter(|line| !line.is_empty())
}

/// 行数、最后一条带时间戳记录的时间与字节数。
fn transcript_side(bytes: &[u8]) -> SessionSide {
    let mut side = SessionSide {
        lines: 0,
        last_timestamp: 0,
        bytes: bytes.len() as u64,
    };
    for line in lines(bytes) {
        side.lines += 1;
        if let Some(timestamp) =
            top_level_string(line, "timestamp").and_then(|s| parser::iso_to_ms(&s))
        {
            side.last_timestamp = timestamp;
        }
    }
    side
}

/// 第一条真实用户 Prompt（与索引的判定一致：排除 sidechain 与纯 tool_result）。
fn first_user_prompt(bytes: &[u8]) -> String {
    for line in lines(bytes) {
        if !line.contains("\"user\"") {
            continue;
        }
        let Ok(parsed) = serde_json::from_str::<TitleLine>(line) else {
            continue;
        };
        if parsed.kind.as_deref() != Some("user") || parsed.is_sidechain.unwrap_or(false) {
            continue;
        }
        let Some(message) = parsed.message else {
            continue;
        };
        if message.role.as_deref().is_some_and(|role| role != "user") {
            continue;
        }
        if let Some(text) = message
            .content
            .as_ref()
            .and_then(parser::extract_prompt_text)
        {
            return text.chars().take(TITLE_MAX_CHARS).collect();
        }
    }
    String::new()
}

// ----------------------------- zip 读取 -----------------------------

struct Archive {
    zip: ZipArchive<File>,
    sessions: BTreeMap<SessionKey, SessionEntries>,
    skipped: Vec<String>,
    /// 每个 projects 子目录对应的根 cwd（编码后等于目录名的那个 cwd）。
    roots: BTreeMap<String, String>,
}

impl Archive {
    fn open(path: &Path) -> Result<Self, String> {
        let file = File::open(path).map_err(|error| format!("无法打开 zip 文件：{error}"))?;
        let mut zip =
            ZipArchive::new(file).map_err(|error| format!("无法读取 zip 文件：{error}"))?;
        let mut sessions: BTreeMap<SessionKey, SessionEntries> = BTreeMap::new();
        let mut skipped = Vec::new();
        for index in 0..zip.len() {
            let entry = zip
                .by_index(index)
                .map_err(|error| format!("无法读取 zip 条目 #{index}：{error}"))?;
            let name = entry.name().to_string();
            if is_symlink_mode(entry.unix_mode()) {
                return Err(reject(&name, "符号链接"));
            }
            match classify(&name)? {
                EntryKind::Transcript { dir, session_id } => {
                    sessions
                        .entry(SessionKey { dir, session_id })
                        .or_default()
                        .transcript = Some(index);
                }
                EntryKind::SideFile {
                    dir,
                    session_id,
                    rel,
                } => {
                    if entry.is_dir() {
                        continue;
                    }
                    sessions
                        .entry(SessionKey { dir, session_id })
                        .or_default()
                        .side
                        .push((rel, index));
                }
                EntryKind::Ignored => {}
                EntryKind::Skipped => skipped.push(name),
            }
        }
        // 只有旁挂目录、没有 jsonl 的会话无法导入
        let orphans: Vec<SessionKey> = sessions
            .iter()
            .filter(|(_, entries)| entries.transcript.is_none())
            .map(|(key, _)| key.clone())
            .collect();
        for key in orphans {
            if let Some(entries) = sessions.remove(&key) {
                for (rel, _) in entries.side {
                    skipped.push(format!(
                        "projects/{}/{}/{rel}（没有对应的 {}.jsonl）",
                        key.dir, key.session_id, key.session_id
                    ));
                }
            }
        }
        let mut archive = Self {
            zip,
            sessions,
            skipped,
            roots: BTreeMap::new(),
        };
        let dirs: Vec<String> = archive
            .sessions
            .keys()
            .map(|key| key.dir.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        for dir in dirs {
            let root = archive.root_cwd(&dir)?;
            archive.roots.insert(dir, root);
        }
        Ok(archive)
    }

    /// 找到编码后等于目录名的 cwd；找不到时用该目录下第一条 cwd，再退回目录名本身。
    fn root_cwd(&mut self, dir: &str) -> Result<String, String> {
        let transcripts: Vec<usize> = self
            .sessions
            .iter()
            .filter(|(key, _)| key.dir == dir)
            .filter_map(|(_, entries)| entries.transcript)
            .collect();
        let mut first_seen: Option<String> = None;
        for index in transcripts {
            let entry = self
                .zip
                .by_index(index)
                .map_err(|error| format!("无法读取 zip 条目 #{index}：{error}"))?;
            let mut reader = BufReader::new(entry);
            let mut buffer = Vec::new();
            loop {
                buffer.clear();
                let read = reader
                    .read_until(b'\n', &mut buffer)
                    .map_err(|error| format!("读取会话内容失败：{error}"))?;
                if read == 0 {
                    break;
                }
                let Ok(line) = std::str::from_utf8(&buffer) else {
                    continue;
                };
                let Some(cwd) = top_level_string(line.trim(), "cwd") else {
                    continue;
                };
                if cwd.is_empty() {
                    continue;
                }
                if encode_claude_project_dir(&cwd) == dir {
                    return Ok(cwd);
                }
                if first_seen.is_none() {
                    first_seen = Some(cwd);
                }
            }
        }
        Ok(first_seen.unwrap_or_else(|| dir.to_string()))
    }

    fn read_entry(&mut self, index: usize) -> Result<Vec<u8>, String> {
        let entry = self
            .zip
            .by_index(index)
            .map_err(|error| format!("无法读取 zip 条目 #{index}：{error}"))?;
        if entry.size() > MAX_ENTRY_BYTES {
            return Err(format!("zip 条目「{}」过大，已拒绝。", entry.name()));
        }
        let mut bytes = Vec::with_capacity(entry.size() as usize);
        entry
            .take(MAX_ENTRY_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| format!("解压 zip 条目失败：{error}"))?;
        if bytes.len() as u64 > MAX_ENTRY_BYTES {
            return Err("zip 条目解压后过大，已拒绝。".to_string());
        }
        Ok(bytes)
    }
}

// ----------------------------- 映射 -----------------------------

fn expand_home(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")) {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest).to_string_lossy().to_string();
        }
    }
    path.to_string()
}

/// 校验并归一化用户给出的映射：本机路径必须是绝对路径（支持 `~/`），去掉末尾分隔符；
/// 空值表示保持原路径。返回 云端根目录 -> 本机目录。
pub fn normalize_mappings(mappings: &[ProjectMapping]) -> Result<HashMap<String, String>, String> {
    let mut out = HashMap::new();
    for mapping in mappings {
        let cloud = trim_trailing_separators(mapping.cloud_cwd.trim()).to_string();
        let Some(local) = mapping
            .local_cwd
            .as_deref()
            .map(str::trim)
            .filter(|local| !local.is_empty())
        else {
            continue;
        };
        let local = expand_home(local);
        let local = trim_trailing_separators(&local).to_string();
        if !Path::new(&local).is_absolute() {
            return Err(format!("本机路径必须是绝对路径：{local}"));
        }
        if local == cloud {
            continue;
        }
        out.insert(cloud, local);
    }
    Ok(out)
}

// ----------------------------- 检查 -----------------------------

/// 列出 zip 里的云端项目并给出映射建议。`remembered` 是设置里记住的映射，
/// `candidates` 是本机已索引的 Claude 项目 (名称, 路径)，按最近活跃排序。
pub fn inspect(
    zip_path: &Path,
    remembered: &BTreeMap<String, String>,
    candidates: &[(String, String)],
) -> Result<ImportInspection, String> {
    let archive = Archive::open(zip_path)?;
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for key in archive.sessions.keys() {
        *counts.entry(key.dir.as_str()).or_insert(0) += 1;
    }
    let projects = counts
        .into_iter()
        .map(|(dir, session_count)| {
            let cloud_cwd = archive
                .roots
                .get(dir)
                .cloned()
                .unwrap_or_else(|| dir.to_string());
            let (suggested_local, suggestion_source) = match remembered.get(&cloud_cwd) {
                Some(local) => (Some(local.clone()), Some("remembered".to_string())),
                None => {
                    let name = project_name(&cloud_cwd);
                    match candidates.iter().find(|(candidate_name, path)| {
                        *candidate_name == name && *path != cloud_cwd
                    }) {
                        Some((_, path)) => (Some(path.clone()), Some("name".to_string())),
                        None => (None, None),
                    }
                }
            };
            ImportProject {
                cloud_dir: dir.to_string(),
                cloud_cwd,
                session_count,
                suggested_local,
                suggestion_source,
            }
        })
        .collect();
    Ok(ImportInspection {
        zip_path: zip_path.to_string_lossy().to_string(),
        session_count: archive.sessions.len(),
        projects,
        skipped_entries: archive.skipped,
    })
}

// ----------------------------- 计划 -----------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WriteSet {
    /// jsonl 与全部旁挂文件
    All,
    /// 只补本机缺少的旁挂文件
    MissingSideOnly,
    Nothing,
}

struct SessionPlan {
    key: SessionKey,
    dto: PlannedSession,
    /// (云端根目录, 本机目录)；None 表示不改写
    rewrite: Option<(String, String)>,
    write_set: WriteSet,
    missing_side: Vec<String>,
}

/// 本机 projects 目录下已有的会话：会话 ID -> 所在目录名列表。
fn existing_sessions(projects_dir: &Path) -> HashMap<String, Vec<String>> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    let Ok(dirs) = fs::read_dir(projects_dir) else {
        return map;
    };
    for dir in dirs.filter_map(Result::ok) {
        let dir_path = dir.path();
        if !dir_path.is_dir() {
            continue;
        }
        let Ok(files) = fs::read_dir(&dir_path) else {
            continue;
        };
        for file in files.filter_map(Result::ok) {
            let path = file.path();
            if path.is_file()
                && path
                    .extension()
                    .is_some_and(|extension| extension == "jsonl")
            {
                if let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) {
                    map.entry(stem.to_string())
                        .or_default()
                        .push(dir.file_name().to_string_lossy().to_string());
                }
            }
        }
    }
    map
}

fn is_compared_side_file(rel: &str) -> bool {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    !UNCOMPARED_SIDE_FILES.contains(&name)
}

fn evaluate_session(
    archive: &mut Archive,
    key: &SessionKey,
    projects_dir: &Path,
    mappings: &HashMap<String, String>,
    existing: &HashMap<String, Vec<String>>,
) -> Result<SessionPlan, String> {
    let (transcript_index, side): (usize, Vec<(String, usize)>) = {
        let entries = archive
            .sessions
            .get(key)
            .ok_or_else(|| "导入包内容在计划后发生了变化，请重新开始导入。".to_string())?;
        (
            entries
                .transcript
                .ok_or_else(|| "会话缺少 jsonl。".to_string())?,
            entries.side.clone(),
        )
    };
    let cloud_cwd = archive
        .roots
        .get(&key.dir)
        .cloned()
        .unwrap_or_else(|| key.dir.clone());
    let rewrite = mappings
        .get(&cloud_cwd)
        .map(|local| (cloud_cwd.clone(), local.clone()));
    let (target_dir, target_cwd) = match &rewrite {
        Some((_, local)) => (encode_claude_project_dir(local), local.clone()),
        None => (key.dir.clone(), cloud_cwd.clone()),
    };

    let raw = archive.read_entry(transcript_index)?;
    let imported_bytes = match &rewrite {
        Some((from, to)) => rewrite_transcript(&raw, from, to),
        None => raw,
    };
    let imported = transcript_side(&imported_bytes);
    let title = first_user_prompt(&imported_bytes);

    let session_dir = projects_dir.join(&target_dir);
    let local_transcript = session_dir.join(format!("{}.jsonl", key.session_id));
    let mut local: Option<SessionSide> = None;
    let mut missing_side = Vec::new();
    let (action, write_set) = if local_transcript.is_file() {
        let local_bytes =
            fs::read(&local_transcript).map_err(|error| format!("读取本机会话失败：{error}"))?;
        local = Some(transcript_side(&local_bytes));
        if local_bytes == imported_bytes {
            // jsonl 相同：比较旁挂目录，同名文件内容不同才算冲突，缺少的直接补上
            let mut conflict = false;
            for (rel, index) in &side {
                if !is_compared_side_file(rel) {
                    continue;
                }
                let local_file = session_dir.join(&key.session_id).join(rel);
                if !local_file.exists() {
                    missing_side.push(rel.clone());
                    continue;
                }
                let local_side = fs::read(&local_file)
                    .map_err(|error| format!("读取本机旁挂文件失败：{error}"))?;
                if local_side != archive.read_entry(*index)? {
                    conflict = true;
                    break;
                }
            }
            if conflict {
                (ImportAction::Conflict, WriteSet::Nothing)
            } else if missing_side.is_empty() {
                (ImportAction::SkipIdentical, WriteSet::Nothing)
            } else {
                (ImportAction::Update, WriteSet::MissingSideOnly)
            }
        } else if imported_bytes.starts_with(&local_bytes) {
            (ImportAction::Update, WriteSet::All)
        } else if local_bytes.starts_with(&imported_bytes) {
            (ImportAction::SkipOlder, WriteSet::Nothing)
        } else {
            (ImportAction::Conflict, WriteSet::Nothing)
        }
    } else {
        (ImportAction::Add, WriteSet::All)
    };

    let existing_elsewhere = existing
        .get(&key.session_id)
        .and_then(|dirs| dirs.iter().find(|dir| **dir != target_dir).cloned());

    Ok(SessionPlan {
        key: key.clone(),
        dto: PlannedSession {
            session_id: key.session_id.clone(),
            cloud_dir: key.dir.clone(),
            cloud_cwd,
            target_dir,
            target_cwd,
            title,
            action,
            imported,
            local,
            side_files: side.len(),
            existing_elsewhere,
        },
        rewrite,
        write_set,
        missing_side,
    })
}

fn plan_sessions(
    archive: &mut Archive,
    projects_dir: &Path,
    mappings: &HashMap<String, String>,
) -> Result<Vec<SessionPlan>, String> {
    if archive.sessions.is_empty() {
        return Err(
            "导入包里没有 projects/<目录名>/<会话ID>.jsonl 形式的会话，没有可导入的内容。"
                .to_string(),
        );
    }
    let existing = existing_sessions(projects_dir);
    let keys: Vec<SessionKey> = archive.sessions.keys().cloned().collect();
    keys.iter()
        .map(|key| evaluate_session(archive, key, projects_dir, mappings, &existing))
        .collect()
}

/// 生成导入计划：不写任何文件。
pub fn plan(
    zip_path: &Path,
    projects_dir: &Path,
    mappings: &[ProjectMapping],
) -> Result<ImportPlan, String> {
    let mappings = normalize_mappings(mappings)?;
    let mut archive = Archive::open(zip_path)?;
    let sessions = plan_sessions(&mut archive, projects_dir, &mappings)?;
    let mut counts = ImportPlanCounts::default();
    for session in &sessions {
        match session.dto.action {
            ImportAction::Add => counts.add += 1,
            ImportAction::Update => counts.update += 1,
            ImportAction::SkipIdentical | ImportAction::SkipOlder => counts.skip += 1,
            ImportAction::Conflict => counts.conflict += 1,
        }
    }
    Ok(ImportPlan {
        zip_path: zip_path.to_string_lossy().to_string(),
        sessions: sessions.into_iter().map(|session| session.dto).collect(),
        counts,
        skipped_entries: archive.skipped,
    })
}

// ----------------------------- 写入 -----------------------------

/// `apply` 的结果；command 层再补上重建后的索引元信息。
pub struct ImportOutcome {
    pub added: usize,
    pub updated: usize,
    pub skipped: usize,
    pub kept_local: usize,
    pub overwritten: usize,
    pub files_written: usize,
    pub skipped_entries: Vec<String>,
    pub projects: Vec<ImportedProject>,
}

/// 先写临时文件再改名，避免留下半截文件；改名会覆盖同名文件。
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("目标路径无效：{}", path.display()))?;
    fs::create_dir_all(parent).map_err(|error| format!("创建目录失败：{error}"))?;
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    let temporary = path.with_file_name(format!("{file_name}.import-tmp"));
    let written = File::create(&temporary)
        .and_then(|mut file| file.write_all(bytes).and_then(|_| file.sync_all()))
        .map_err(|error| format!("写入文件失败：{error}"));
    if let Err(error) = written {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(format!("替换文件失败：{error}"));
    }
    Ok(())
}

/// 确认目标始终落在 projects 目录之内（防御性检查，路径已在分类阶段过滤）。
fn ensure_inside(projects_dir: &Path, target: &Path) -> Result<(), String> {
    if target.starts_with(projects_dir)
        && !target
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        Ok(())
    } else {
        Err(format!(
            "拒绝写入 projects 目录之外的路径：{}",
            target.display()
        ))
    }
}

/// 按计划与冲突决定写入。所有会话先校验决定齐全再开始写；写入过程中出错立即停止并报告。
pub fn apply(
    zip_path: &Path,
    projects_dir: &Path,
    mappings: &[ProjectMapping],
    decisions: &[ConflictDecision],
) -> Result<ImportOutcome, String> {
    let mappings = normalize_mappings(mappings)?;
    let mut archive = Archive::open(zip_path)?;
    let sessions = plan_sessions(&mut archive, projects_dir, &mappings)?;
    let decided: HashMap<(String, String), bool> = decisions
        .iter()
        .map(|decision| {
            (
                (decision.cloud_dir.clone(), decision.session_id.clone()),
                decision.keep_local,
            )
        })
        .collect();
    for session in &sessions {
        if session.dto.action == ImportAction::Conflict
            && !decided.contains_key(&(session.key.dir.clone(), session.key.session_id.clone()))
        {
            return Err(format!(
                "会话 {} 存在冲突但没有收到处理决定；导入包或本机文件可能在计划后发生了变化，请重新开始导入。",
                session.key.session_id
            ));
        }
    }

    let mut outcome = ImportOutcome {
        added: 0,
        updated: 0,
        skipped: 0,
        kept_local: 0,
        overwritten: 0,
        files_written: 0,
        skipped_entries: archive.skipped.clone(),
        projects: Vec::new(),
    };
    let mut touched_projects: BTreeMap<String, ImportedProject> = BTreeMap::new();
    for session in sessions {
        let write_set = match session.dto.action {
            ImportAction::Add => {
                outcome.added += 1;
                WriteSet::All
            }
            ImportAction::Update => {
                outcome.updated += 1;
                session.write_set
            }
            ImportAction::SkipIdentical | ImportAction::SkipOlder => {
                outcome.skipped += 1;
                WriteSet::Nothing
            }
            ImportAction::Conflict => {
                let keep_local =
                    decided[&(session.key.dir.clone(), session.key.session_id.clone())];
                if keep_local {
                    outcome.kept_local += 1;
                    WriteSet::Nothing
                } else {
                    outcome.overwritten += 1;
                    WriteSet::All
                }
            }
        };
        if write_set == WriteSet::Nothing {
            continue;
        }
        let entries = archive
            .sessions
            .get(&session.key)
            .map(|entries| (entries.transcript, entries.side.clone()))
            .ok_or_else(|| "导入包内容在计划后发生了变化，请重新开始导入。".to_string())?;
        let session_dir = projects_dir.join(&session.dto.target_dir);
        if write_set == WriteSet::All {
            let transcript_index = entries.0.ok_or_else(|| "会话缺少 jsonl。".to_string())?;
            let raw = archive.read_entry(transcript_index)?;
            let bytes = match &session.rewrite {
                Some((from, to)) => rewrite_transcript(&raw, from, to),
                None => raw,
            };
            let target = session_dir.join(format!("{}.jsonl", session.key.session_id));
            ensure_inside(projects_dir, &target)?;
            write_atomic(&target, &bytes)?;
            outcome.files_written += 1;
        }
        for (rel, index) in entries.1 {
            if write_set == WriteSet::MissingSideOnly && !session.missing_side.contains(&rel) {
                continue;
            }
            let bytes = archive.read_entry(index)?;
            let target = session_dir.join(&session.key.session_id).join(&rel);
            ensure_inside(projects_dir, &target)?;
            write_atomic(&target, &bytes)?;
            outcome.files_written += 1;
        }
        touched_projects
            .entry(session.dto.target_cwd.clone())
            .or_insert_with(|| ImportedProject {
                name: project_name(&session.dto.target_cwd),
                path: session.dto.target_cwd.clone(),
            });
    }
    outcome.projects = touched_projects.into_values().collect();
    Ok(outcome)
}

// ----------------------------- 测试 -----------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    const DIR: &str = "-home-user-repo";
    const CLOUD: &str = "/home/user/repo";
    const SID: &str = "11111111-2222-3333-4444-555555555555";

    fn temp_root(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("cc-history-import-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn line(kind: &str, cwd: &str, ts: &str, extra: &str) -> String {
        format!("{{\"type\":\"{kind}\",\"cwd\":\"{cwd}\",\"timestamp\":\"{ts}\"{extra}}}\n")
    }

    fn user_line(cwd: &str, ts: &str, text: &str) -> String {
        line(
            "user",
            cwd,
            ts,
            &format!(",\"message\":{{\"role\":\"user\",\"content\":\"{text}\"}}"),
        )
    }

    fn transcript(prompts: &[(&str, &str)]) -> String {
        let mut out = String::from("{\"type\":\"queue-operation\",\"sessionId\":\"x\"}\n");
        for (ts, text) in prompts {
            out.push_str(&user_line(CLOUD, ts, text));
            out.push_str(&line(
                "assistant",
                &format!("{CLOUD}/sub/dir"),
                ts,
                ",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"ok\"}]}",
            ));
        }
        out
    }

    struct ZipBuilder {
        writer: ZipWriter<File>,
        path: PathBuf,
    }

    impl ZipBuilder {
        fn new(root: &Path, name: &str) -> Self {
            let path = root.join(name);
            Self {
                writer: ZipWriter::new(File::create(&path).unwrap()),
                path,
            }
        }
        fn file(mut self, name: &str, bytes: &[u8]) -> Self {
            self.writer
                .start_file(name, SimpleFileOptions::default())
                .unwrap();
            self.writer.write_all(bytes).unwrap();
            self
        }
        fn dir(mut self, name: &str) -> Self {
            self.writer
                .add_directory(name, SimpleFileOptions::default())
                .unwrap();
            self
        }
        fn symlink(mut self, name: &str, target: &str) -> Self {
            self.writer
                .add_symlink(name, target, SimpleFileOptions::default())
                .unwrap();
            self
        }
        fn finish(self) -> PathBuf {
            self.writer.finish().unwrap();
            self.path
        }
    }

    fn session_zip(root: &Path, name: &str, content: &str, side: &[(&str, &str)]) -> PathBuf {
        let mut builder = ZipBuilder::new(root, name)
            .dir("projects/")
            .dir(&format!("projects/{DIR}/"))
            .file(&format!("projects/{DIR}/{SID}.jsonl"), content.as_bytes());
        for (rel, body) in side {
            builder = builder.file(&format!("projects/{DIR}/{SID}/{rel}"), body.as_bytes());
        }
        builder.finish()
    }

    fn keep() -> Vec<ProjectMapping> {
        Vec::new()
    }

    fn mapped(local: &str) -> Vec<ProjectMapping> {
        vec![ProjectMapping {
            cloud_cwd: CLOUD.to_string(),
            local_cwd: Some(local.to_string()),
        }]
    }

    #[test]
    fn top_level_scanner_only_matches_depth_one_string_values() {
        let text = r#"{"a":{"cwd":"/inner"},"list":[{"cwd":"/in-list"}],"n":1,"cwd":"/outer\"q","z":"cwd"}"#;
        let (start, end) = top_level_string_member(text, "cwd").unwrap();
        assert_eq!(&text[start..end], r#""/outer\"q""#);
        assert_eq!(top_level_string(text, "cwd").as_deref(), Some("/outer\"q"));
        assert_eq!(top_level_string_member(text, "missing"), None);
        assert_eq!(top_level_string_member(r#"{"cwd":123}"#, "cwd"), None);
        assert_eq!(top_level_string_member(r#"[{"cwd":"x"}]"#, "cwd"), None);
        assert_eq!(
            top_level_string_member(r#"{"cwd":"unterminated"#, "cwd"),
            None
        );
        assert_eq!(
            top_level_string(r#" { "x" : "y" , "cwd" : "/spaced" } "#, "cwd").as_deref(),
            Some("/spaced")
        );
    }

    #[test]
    fn cwd_mapping_covers_root_subpaths_and_windows_targets() {
        assert_eq!(
            map_cwd("/home/user/repo", "/home/user/repo", "/Users/me/repo").as_deref(),
            Some("/Users/me/repo")
        );
        assert_eq!(
            map_cwd("/home/user/repo/a/b", "/home/user/repo/", "/Users/me/repo/").as_deref(),
            Some("/Users/me/repo/a/b")
        );
        assert_eq!(
            map_cwd(
                "/home/user/repo/a/b",
                "/home/user/repo",
                "C:\\Users\\me\\repo"
            )
            .as_deref(),
            Some("C:\\Users\\me\\repo\\a\\b")
        );
        assert_eq!(
            map_cwd("/home/user/repo-other", "/home/user/repo", "/x"),
            None
        );
        assert_eq!(map_cwd("/elsewhere", "/home/user/repo", "/x"), None);
    }

    #[test]
    fn transcript_rewrite_only_touches_top_level_cwd_and_keeps_every_other_byte() {
        let original = concat!(
            "{\"type\":\"user\",\"cwd\":\"/home/user/repo\",\"message\":{\"content\":\"/home/user/repo stays\",\"cwd\":\"/home/user/repo\"}}\n",
            "{\"type\":\"x\",\"cwd\":\"/home/user/repo/子目录\",\"n\":1}\r\n",
            "not json at all\n",
            "{\"type\":\"y\",\"cwd\":\"/other/place\"}"
        );
        let rewritten =
            rewrite_transcript(original.as_bytes(), "/home/user/repo", "/Users/me/repo");
        let text = String::from_utf8(rewritten.clone()).unwrap();
        let expected = concat!(
            "{\"type\":\"user\",\"cwd\":\"/Users/me/repo\",\"message\":{\"content\":\"/home/user/repo stays\",\"cwd\":\"/home/user/repo\"}}\n",
            "{\"type\":\"x\",\"cwd\":\"/Users/me/repo/子目录\",\"n\":1}\r\n",
            "not json at all\n",
            "{\"type\":\"y\",\"cwd\":\"/other/place\"}"
        );
        assert_eq!(text, expected);
        // 确定性：再改写一次得到同样的字节
        assert_eq!(
            rewrite_transcript(&rewritten, "/home/user/repo", "/Users/me/repo"),
            rewritten
        );
    }

    #[test]
    fn unsafe_entries_reject_the_whole_archive() {
        let root = temp_root("unsafe");
        let content = transcript(&[("2026-09-22T07:22:15.169Z", "hi")]);
        let cases: Vec<(&str, PathBuf)> = vec![
            (
                "dotdot",
                ZipBuilder::new(&root, "dotdot.zip")
                    .file(&format!("projects/{DIR}/{SID}.jsonl"), content.as_bytes())
                    .file("projects/../settings.json", b"{}")
                    .finish(),
            ),
            (
                "absolute",
                ZipBuilder::new(&root, "absolute.zip")
                    .file("/etc/passwd", b"x")
                    .finish(),
            ),
            (
                "drive",
                ZipBuilder::new(&root, "drive.zip")
                    .file("C:\\Users\\x\\.claude\\settings.json", b"{}")
                    .finish(),
            ),
            (
                "symlink",
                ZipBuilder::new(&root, "symlink.zip")
                    .file(&format!("projects/{DIR}/{SID}.jsonl"), content.as_bytes())
                    .symlink(
                        &format!("projects/{DIR}/{SID}/tool-results/link.txt"),
                        "/etc/passwd",
                    )
                    .finish(),
            ),
        ];
        for (label, zip) in cases {
            let error = inspect(&zip, &BTreeMap::new(), &[])
                .err()
                .unwrap_or_default();
            assert!(error.contains("已拒绝整个导入包"), "{label}: {error}");
            let projects = root.join(format!("projects-{label}"));
            assert!(plan(&zip, &projects, &keep()).is_err(), "{label}");
            assert!(apply(&zip, &projects, &keep(), &[]).is_err(), "{label}");
            assert!(!projects.exists(), "{label}: nothing may be written");
        }
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn out_of_scope_entries_are_skipped_and_reported() {
        let root = temp_root("skip");
        let content = transcript(&[("2026-09-22T07:22:15.169Z", "hi")]);
        let zip = ZipBuilder::new(&root, "mixed.zip")
            .dir("__MACOSX/")
            .file("__MACOSX/._x", b"junk")
            .file(".DS_Store", b"junk")
            .file("README.txt", b"hello")
            .file("history.jsonl", b"{}")
            .file(&format!("projects/{DIR}/notes.txt"), b"stray")
            .file(&format!("projects/{DIR}/{SID}.jsonl"), content.as_bytes())
            .file(&format!("projects/{DIR}/{SID}/tool-results/a.txt"), b"A")
            .file(&format!("projects/{DIR}/{SID}/.DS_Store"), b"junk")
            .file(
                &format!("projects/{DIR}/orphan-session/tool-results/b.txt"),
                b"B",
            )
            .finish();
        let inspection = inspect(&zip, &BTreeMap::new(), &[]).unwrap();
        assert_eq!(inspection.session_count, 1);
        assert_eq!(inspection.projects.len(), 1);
        assert_eq!(inspection.projects[0].cloud_cwd, CLOUD);
        assert_eq!(inspection.projects[0].cloud_dir, DIR);
        let skipped = inspection.skipped_entries.join("\n");
        for name in [
            "__MACOSX/._x",
            ".DS_Store",
            "README.txt",
            "history.jsonl",
            "notes.txt",
            "orphan-session",
        ] {
            assert!(skipped.contains(name), "missing {name} in {skipped}");
        }
        assert!(!skipped.contains("tool-results/a.txt"));

        let projects = root.join("projects");
        let result = apply(&zip, &projects, &keep(), &[]).unwrap();
        assert_eq!(result.added, 1);
        assert_eq!(result.files_written, 2, "jsonl + a.txt only");
        assert!(projects.join(DIR).join(format!("{SID}.jsonl")).is_file());
        assert!(projects
            .join(DIR)
            .join(SID)
            .join("tool-results")
            .join("a.txt")
            .is_file());
        assert!(!projects.join(DIR).join("notes.txt").exists());
        assert!(!projects.join(DIR).join(SID).join(".DS_Store").exists());
        assert!(!root.join("README.txt").exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn suggestions_prefer_remembered_mappings_then_matching_folder_names() {
        let root = temp_root("suggest");
        let zip = session_zip(
            &root,
            "s.zip",
            &transcript(&[("2026-09-22T07:22:15.169Z", "hi")]),
            &[],
        );
        let candidates = vec![
            ("other".to_string(), "/Users/me/other".to_string()),
            ("repo".to_string(), "/Users/me/work/repo".to_string()),
        ];
        let by_name = inspect(&zip, &BTreeMap::new(), &candidates).unwrap();
        assert_eq!(
            by_name.projects[0].suggested_local.as_deref(),
            Some("/Users/me/work/repo")
        );
        assert_eq!(
            by_name.projects[0].suggestion_source.as_deref(),
            Some("name")
        );

        let mut remembered = BTreeMap::new();
        remembered.insert(CLOUD.to_string(), "/Users/me/remembered/repo".to_string());
        let by_memory = inspect(&zip, &remembered, &candidates).unwrap();
        assert_eq!(
            by_memory.projects[0].suggested_local.as_deref(),
            Some("/Users/me/remembered/repo")
        );
        assert_eq!(
            by_memory.projects[0].suggestion_source.as_deref(),
            Some("remembered")
        );

        let none = inspect(&zip, &BTreeMap::new(), &[]).unwrap();
        assert_eq!(none.projects[0].suggested_local, None);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn plan_distinguishes_add_update_identical_older_and_conflict() {
        let root = temp_root("plan");
        let projects = root.join("projects");
        let older = transcript(&[("2026-09-22T07:22:15.169Z", "first")]);
        let newer = transcript(&[
            ("2026-09-22T07:22:15.169Z", "first"),
            ("2026-09-23T09:29:45.811Z", "second"),
        ]);
        let diverged = transcript(&[
            ("2026-09-22T07:22:15.169Z", "first"),
            ("2026-09-24T00:00:00.000Z", "elsewhere"),
        ]);
        let zip_older = session_zip(&root, "older.zip", &older, &[("tool-results/a.txt", "A")]);
        let zip_newer = session_zip(
            &root,
            "newer.zip",
            &newer,
            &[
                ("tool-results/a.txt", "A"),
                ("ccr-tip.json", "{\"updatedAt\":1}"),
            ],
        );

        // 本机没有 → 新增
        let first = plan(&zip_older, &projects, &keep()).unwrap();
        assert_eq!(first.sessions[0].action, ImportAction::Add);
        assert_eq!(first.sessions[0].title, "first");
        assert_eq!(first.sessions[0].imported.lines, 3);
        assert_eq!(first.sessions[0].local, None);
        assert_eq!(first.sessions[0].target_dir, DIR);
        assert_eq!(first.sessions[0].target_cwd, CLOUD);
        assert!(first.sessions[0].imported.last_timestamp > 0);
        assert!(!projects.exists(), "plan must not write");
        assert_eq!(
            (
                first.counts.add,
                first.counts.update,
                first.counts.skip,
                first.counts.conflict
            ),
            (1, 0, 0, 0)
        );

        apply(&zip_older, &projects, &keep(), &[]).unwrap();

        // 完全相同 → 跳过
        let same = plan(&zip_older, &projects, &keep()).unwrap();
        assert_eq!(same.sessions[0].action, ImportAction::SkipIdentical);
        assert_eq!(same.sessions[0].local.unwrap().lines, 3);

        // 本机是导入的前缀 → 覆盖更新
        let update = plan(&zip_newer, &projects, &keep()).unwrap();
        assert_eq!(update.sessions[0].action, ImportAction::Update);
        assert_eq!(update.sessions[0].imported.lines, 5);
        assert!(
            update.sessions[0].imported.last_timestamp
                > update.sessions[0].local.unwrap().last_timestamp
        );
        let applied = apply(&zip_newer, &projects, &keep(), &[]).unwrap();
        assert_eq!(applied.updated, 1);
        assert_eq!(
            fs::read_to_string(projects.join(DIR).join(format!("{SID}.jsonl"))).unwrap(),
            newer
        );
        assert!(projects.join(DIR).join(SID).join("ccr-tip.json").is_file());

        // 导入的是更旧快照 → 跳过
        let old_again = plan(&zip_older, &projects, &keep()).unwrap();
        assert_eq!(old_again.sessions[0].action, ImportAction::SkipOlder);
        let applied = apply(&zip_older, &projects, &keep(), &[]).unwrap();
        assert_eq!(applied.skipped, 1);
        assert_eq!(applied.files_written, 0);

        // jsonl 相同、ccr-tip.json 不同 → 仍然跳过（不参与比较）
        let zip_tip = session_zip(
            &root,
            "tip.zip",
            &newer,
            &[
                ("tool-results/a.txt", "A"),
                ("ccr-tip.json", "{\"updatedAt\":2}"),
            ],
        );
        assert_eq!(
            plan(&zip_tip, &projects, &keep()).unwrap().sessions[0].action,
            ImportAction::SkipIdentical
        );

        // jsonl 相同、导入包多出旁挂文件 → 更新，只补缺的
        let zip_extra = session_zip(
            &root,
            "extra.zip",
            &newer,
            &[("tool-results/a.txt", "A"), ("tool-results/b.txt", "B")],
        );
        let extra = plan(&zip_extra, &projects, &keep()).unwrap();
        assert_eq!(extra.sessions[0].action, ImportAction::Update);
        let before = fs::metadata(projects.join(DIR).join(format!("{SID}.jsonl")))
            .unwrap()
            .modified()
            .unwrap();
        let applied = apply(&zip_extra, &projects, &keep(), &[]).unwrap();
        assert_eq!((applied.updated, applied.files_written), (1, 1));
        assert!(projects
            .join(DIR)
            .join(SID)
            .join("tool-results")
            .join("b.txt")
            .is_file());
        let after = fs::metadata(projects.join(DIR).join(format!("{SID}.jsonl")))
            .unwrap()
            .modified()
            .unwrap();
        assert_eq!(before, after, "the identical transcript is left untouched");

        // jsonl 相同、同名旁挂文件内容不同 → 冲突
        let zip_side = session_zip(
            &root,
            "side.zip",
            &newer,
            &[("tool-results/a.txt", "A-changed")],
        );
        assert_eq!(
            plan(&zip_side, &projects, &keep()).unwrap().sessions[0].action,
            ImportAction::Conflict
        );

        // 互不为前缀 → 冲突
        let zip_diverged = session_zip(&root, "diverged.zip", &diverged, &[]);
        let conflict = plan(&zip_diverged, &projects, &keep()).unwrap();
        assert_eq!(conflict.sessions[0].action, ImportAction::Conflict);
        assert_eq!(conflict.counts.conflict, 1);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn conflicts_need_decisions_and_never_delete_local_files() {
        let root = temp_root("conflict");
        let projects = root.join("projects");
        let local = transcript(&[
            ("2026-09-22T07:22:15.169Z", "first"),
            ("2026-09-23T00:00:00.000Z", "local"),
        ]);
        let imported = transcript(&[
            ("2026-09-22T07:22:15.169Z", "first"),
            ("2026-09-24T00:00:00.000Z", "cloud"),
        ]);
        apply(
            &session_zip(
                &root,
                "local.zip",
                &local,
                &[("tool-results/only-local.txt", "keep me")],
            ),
            &projects,
            &keep(),
            &[],
        )
        .unwrap();
        let zip = session_zip(
            &root,
            "cloud.zip",
            &imported,
            &[("tool-results/cloud.txt", "C")],
        );

        let missing = apply(&zip, &projects, &keep(), &[]).err().unwrap();
        assert!(missing.contains("没有收到处理决定"));
        assert_eq!(
            fs::read_to_string(projects.join(DIR).join(format!("{SID}.jsonl"))).unwrap(),
            local
        );

        let decision = |keep_local: bool| {
            vec![ConflictDecision {
                cloud_dir: DIR.to_string(),
                session_id: SID.to_string(),
                keep_local,
            }]
        };
        let kept = apply(&zip, &projects, &keep(), &decision(true)).unwrap();
        assert_eq!((kept.kept_local, kept.files_written), (1, 0));
        assert_eq!(
            fs::read_to_string(projects.join(DIR).join(format!("{SID}.jsonl"))).unwrap(),
            local
        );

        let overwritten = apply(&zip, &projects, &keep(), &decision(false)).unwrap();
        assert_eq!((overwritten.overwritten, overwritten.files_written), (1, 2));
        assert_eq!(
            fs::read_to_string(projects.join(DIR).join(format!("{SID}.jsonl"))).unwrap(),
            imported
        );
        let side = projects.join(DIR).join(SID).join("tool-results");
        assert_eq!(fs::read_to_string(side.join("cloud.txt")).unwrap(), "C");
        assert_eq!(
            fs::read_to_string(side.join("only-local.txt")).unwrap(),
            "keep me",
            "local extras survive"
        );
        assert!(fs::read_dir(projects.join(DIR)).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".import-tmp")
        }));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn mapped_projects_are_rewritten_and_written_under_the_local_directory_name() {
        let root = temp_root("mapping");
        let projects = root.join("projects");
        let content = transcript(&[("2026-09-22T07:22:15.169Z", "hi")]);
        let zip = session_zip(&root, "m.zip", &content, &[("tool-results/a.txt", "A")]);
        let local = if cfg!(windows) {
            "C:\\Users\\me\\work\\repo"
        } else {
            "/Users/me/work/repo"
        };

        let planned = plan(&zip, &projects, &mapped(local)).unwrap();
        let session = &planned.sessions[0];
        assert_eq!(session.cloud_cwd, CLOUD);
        assert_eq!(session.target_cwd, local);
        assert_eq!(session.target_dir, encode_claude_project_dir(local));
        assert_eq!(session.action, ImportAction::Add);

        let result = apply(&zip, &projects, &mapped(local), &[]).unwrap();
        assert_eq!(result.projects.len(), 1);
        assert_eq!(result.projects[0].path, local);
        assert_eq!(result.projects[0].name, "repo");
        let target_dir = projects.join(encode_claude_project_dir(local));
        let written = fs::read_to_string(target_dir.join(format!("{SID}.jsonl"))).unwrap();
        assert!(
            !written.contains(&format!("\"cwd\":\"{CLOUD}")),
            "{written}"
        );
        assert!(written.contains(&format!(
            "\"cwd\":{}",
            serde_json::to_string(local).unwrap()
        )));
        assert!(written.contains(
            &serde_json::to_string(&format!(
                "{local}{}sub{}dir",
                if cfg!(windows) { "\\" } else { "/" },
                if cfg!(windows) { "\\" } else { "/" }
            ))
            .unwrap()
        ));
        assert!(
            written.starts_with("{\"type\":\"queue-operation\""),
            "lines without cwd are untouched"
        );
        assert!(target_dir
            .join(SID)
            .join("tool-results")
            .join("a.txt")
            .is_file());
        assert!(
            !projects.join(DIR).exists(),
            "the cloud directory name is not created"
        );

        // 同一映射再导入一次 → 完全相同，跳过；换回不映射 → 另一个目录里是新增，并提示已存在于别处
        let again = plan(&zip, &projects, &mapped(local)).unwrap();
        assert_eq!(again.sessions[0].action, ImportAction::SkipIdentical);
        let unmapped = plan(&zip, &projects, &keep()).unwrap();
        assert_eq!(unmapped.sessions[0].action, ImportAction::Add);
        assert_eq!(
            unmapped.sessions[0].existing_elsewhere.as_deref(),
            Some(encode_claude_project_dir(local).as_str())
        );

        assert!(plan(
            &zip,
            &projects,
            &[ProjectMapping {
                cloud_cwd: CLOUD.into(),
                local_cwd: Some("relative/path".into())
            }]
        )
        .is_err());
        let _ = fs::remove_dir_all(&root);
    }
}
