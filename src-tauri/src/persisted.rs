//! `<persisted-output>` 引用的定位与读取。
//!
//! Claude Code 把超大的工具输出写到 `projects/<目录名>/<会话ID>/tool-results/*.txt`，并在
//! tool_result 正文里留下生成时的绝对路径。云端导出、跨机器导入或改写 cwd 之后，这个
//! 绝对路径在本机通常不存在，因此展示层按"会话自己的目录"和"projects/ 之后的部分"
//! 两种方式在当前 projects 目录下重新定位；读取时再次校验路径落在 projects 目录之内。

use std::fs::{self, File};
use std::io::Read;
use std::path::{Component, Path, PathBuf};

/// tool_result 正文里持久化输出块的开头标记。
pub const MARKER: &str = "<persisted-output>";
const SAVED_TO: &str = "Full output saved to:";

/// 从 tool_result 正文里取出持久化输出文件的原始路径（生成时的绝对路径）。
pub fn path_from_text(text: &str) -> Option<String> {
    let head = text.trim_start();
    if !head.starts_with(MARKER) {
        return None;
    }
    let after = &head[head.find(SAVED_TO)? + SAVED_TO.len()..];
    let line = after.lines().next()?.trim();
    (!line.is_empty()).then(|| line.to_string())
}

/// 同时接受 `/` 与 `\` 的路径切分，丢弃空段。
fn components(raw: &str) -> Vec<&str> {
    raw.split(['/', '\\'])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect()
}

/// 把原始绝对路径在本机 projects 目录下重新定位。
///
/// 返回存在的文件路径与相对 projects 目录的 `/` 分隔相对路径。候选顺序：
/// 1. 当前会话所在的项目目录 + `<会话ID>/tool-results/...`（项目目录被映射改名后仍然有效）；
/// 2. projects 目录 + 原路径中 `projects/` 之后的全部片段。
pub fn resolve(raw: &str, projects_dir: &Path, session_file: &Path) -> Option<(PathBuf, String)> {
    let parts = components(raw);
    let marker = parts.iter().rposition(|part| *part == "projects")?;
    let remainder = &parts[marker + 1..];
    if remainder.len() < 2
        || remainder
            .iter()
            .any(|part| *part == ".." || *part == "." || part.contains(':'))
    {
        return None;
    }

    let mut candidates = Vec::with_capacity(2);
    if let Some(project_dir) = session_project_dir(projects_dir, session_file) {
        let mut candidate = project_dir;
        for part in &remainder[1..] {
            candidate.push(part);
        }
        candidates.push(candidate);
    }
    let mut literal = projects_dir.to_path_buf();
    for part in remainder {
        literal.push(part);
    }
    candidates.push(literal);

    candidates.into_iter().find_map(|candidate| {
        if !candidate.is_file() {
            return None;
        }
        let relative = contained_relative(projects_dir, &candidate)?;
        Some((candidate, relative))
    })
}

/// 会话文件所属的项目目录：projects 目录下的第一层子目录；不在 projects 目录内时退回其父目录。
fn session_project_dir(projects_dir: &Path, session_file: &Path) -> Option<PathBuf> {
    if let Ok(relative) = session_file.strip_prefix(projects_dir) {
        if let Some(Component::Normal(first)) = relative.components().next() {
            return Some(projects_dir.join(first));
        }
    }
    session_file.parent().map(Path::to_path_buf)
}

/// 校验 `candidate` 真实位于 `projects_dir` 之内（解析符号链接后），返回 `/` 分隔的相对路径。
fn contained_relative(projects_dir: &Path, candidate: &Path) -> Option<String> {
    let root = fs::canonicalize(projects_dir).ok()?;
    let real = fs::canonicalize(candidate).ok()?;
    let relative = real.strip_prefix(&root).ok()?;
    let joined: Vec<String> = relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect();
    (!joined.is_empty()).then(|| joined.join("/"))
}

/// 读取 projects 目录下的持久化输出。`relative` 必须是 [`resolve`] 返回的相对路径形状：
/// 不接受绝对路径、`..` 或空段；最多读取 `max_bytes` 字节，超出时标记 truncated。
pub fn read(
    projects_dir: &Path,
    relative: &str,
    max_bytes: usize,
) -> Result<(String, bool, u64), String> {
    let parts = components(relative);
    if parts.is_empty()
        || relative.starts_with(['/', '\\'])
        || parts
            .iter()
            .any(|part| *part == ".." || *part == "." || part.contains(':'))
    {
        return Err("持久化输出路径无效。".to_string());
    }
    let mut target = projects_dir.to_path_buf();
    for part in &parts {
        target.push(part);
    }
    if !target.is_file() {
        return Err("持久化输出文件不存在或已被清理。".to_string());
    }
    contained_relative(projects_dir, &target)
        .ok_or_else(|| "持久化输出文件不在 projects 目录内。".to_string())?;
    let size = fs::metadata(&target).map(|m| m.len()).unwrap_or(0);
    let mut bytes = Vec::new();
    File::open(&target)
        .and_then(|file| file.take(max_bytes as u64 + 1).read_to_end(&mut bytes))
        .map_err(|error| format!("读取持久化输出失败：{error}"))?;
    let truncated = bytes.len() > max_bytes;
    if truncated {
        bytes.truncate(max_bytes);
        // 不要切在多字节字符中间
        while !bytes.is_empty() && std::str::from_utf8(&bytes).is_err() {
            bytes.pop();
        }
    }
    Ok((
        String::from_utf8_lossy(&bytes).into_owned(),
        truncated,
        size,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "cc-history-persisted-{}-{name}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn extracts_the_saved_path_from_the_preview_text() {
        let text = "<persisted-output>\nOutput too large (34.5KB). Full output saved to: /root/.claude/projects/-home-user-x/abc/tool-results/bk.txt\n\nPreview (first 2KB):\nhello";
        assert_eq!(
            path_from_text(text).as_deref(),
            Some("/root/.claude/projects/-home-user-x/abc/tool-results/bk.txt")
        );
        assert_eq!(path_from_text("plain output"), None);
        assert_eq!(path_from_text("<persisted-output>\nno path here"), None);
    }

    #[test]
    fn resolves_by_session_directory_first_then_literal_remainder() {
        let root = temp_root("resolve");
        let projects = root.join("projects");
        // 映射后的项目目录名与云端不同：只有"会话自己的目录"这条路能找到
        let mapped = projects
            .join("-Users-me-repo")
            .join("sid-1")
            .join("tool-results");
        fs::create_dir_all(&mapped).unwrap();
        fs::write(mapped.join("a.txt"), "full a").unwrap();
        let session_file = projects.join("-Users-me-repo").join("sid-1.jsonl");
        fs::write(&session_file, "{}").unwrap();

        let unix = "/root/.claude/projects/-home-user-repo/sid-1/tool-results/a.txt";
        let (path, relative) = resolve(unix, &projects, &session_file).unwrap();
        assert_eq!(path, mapped.join("a.txt"));
        assert_eq!(relative, "-Users-me-repo/sid-1/tool-results/a.txt");

        let windows =
            "C:\\Users\\me\\.claude\\projects\\-home-user-repo\\sid-1\\tool-results\\a.txt";
        assert_eq!(
            resolve(windows, &projects, &session_file).unwrap().1,
            relative
        );

        // 原目录名仍存在时，字面路径也能命中
        let literal_dir = projects
            .join("-home-user-repo")
            .join("sid-9")
            .join("tool-results");
        fs::create_dir_all(&literal_dir).unwrap();
        fs::write(literal_dir.join("b.txt"), "full b").unwrap();
        let other = "/root/.claude/projects/-home-user-repo/sid-9/tool-results/b.txt";
        assert_eq!(
            resolve(other, &projects, &session_file).unwrap().1,
            "-home-user-repo/sid-9/tool-results/b.txt"
        );

        assert!(resolve(
            "/root/.claude/projects/-x/sid/tool-results/missing.txt",
            &projects,
            &session_file
        )
        .is_none());
        assert!(resolve(
            "/root/.claude/projects/../secrets.txt",
            &projects,
            &session_file
        )
        .is_none());
        assert!(resolve("/no/marker/here.txt", &projects, &session_file).is_none());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn read_stays_inside_projects_and_caps_size() {
        let root = temp_root("read");
        let projects = root.join("projects");
        let dir = projects.join("-p").join("sid").join("tool-results");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("out.txt"), "héllo world").unwrap();
        fs::write(root.join("secret.txt"), "nope").unwrap();

        let (text, truncated, size) = read(&projects, "-p/sid/tool-results/out.txt", 1024).unwrap();
        assert_eq!(text, "héllo world");
        assert!(!truncated);
        assert_eq!(size, "héllo world".len() as u64);

        let (clipped, truncated, _) = read(&projects, "-p/sid/tool-results/out.txt", 2).unwrap();
        assert_eq!(clipped, "h", "must not cut inside the two-byte é");
        assert!(truncated);

        assert!(read(&projects, "../secret.txt", 1024).is_err());
        assert!(read(&projects, "/etc/passwd", 1024).is_err());
        assert!(read(&projects, "-p/sid/tool-results/missing.txt", 1024).is_err());
        let _ = fs::remove_dir_all(&root);
    }
}
