//! 显式 opt-in 的真实导入包冒烟：`CC_IMPORT_SMOKE_ZIP=/path/to/export.zip cargo test --test import_smoke`。
//! 一切写入临时目录里的 projects，不碰真实 `~/.claude`；只输出计数，不输出任何会话内容或路径。

use cc_history_viewer_lib::models::{ImportAction, ProjectMapping};
use cc_history_viewer_lib::{import, parser, persisted};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

fn basename(path: &str) -> String {
    path.trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(path)
        .to_string()
}

#[test]
fn real_export_zip_round_trips_through_import() {
    let Ok(zip) = std::env::var("CC_IMPORT_SMOKE_ZIP") else {
        eprintln!("CC_IMPORT_SMOKE_ZIP not set; skipping the real-data import smoke test");
        return;
    };
    let zip = Path::new(&zip);
    let root = std::env::temp_dir().join(format!("cc-history-import-smoke-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let projects = root.join("projects");
    let local_root = root.join("local");

    let inspection = import::inspect(zip, &BTreeMap::new(), &[]).unwrap();
    assert!(
        inspection.session_count > 0,
        "the zip holds no importable session"
    );
    let mappings: Vec<ProjectMapping> = inspection
        .projects
        .iter()
        .map(|project| ProjectMapping {
            cloud_cwd: project.cloud_cwd.clone(),
            local_cwd: Some(
                local_root
                    .join(basename(&project.cloud_cwd))
                    .to_string_lossy()
                    .to_string(),
            ),
        })
        .collect();

    let plan = import::plan(zip, &projects, &mappings).unwrap();
    assert!(plan.sessions.iter().all(|s| s.action == ImportAction::Add));
    assert!(!projects.exists(), "planning must not write");
    let result = import::apply(zip, &projects, &mappings, &[]).unwrap();
    assert_eq!(result.added, inspection.session_count);

    let again = import::plan(zip, &projects, &mappings).unwrap();
    assert!(again
        .sessions
        .iter()
        .all(|s| s.action == ImportAction::SkipIdentical));

    let mut messages = 0usize;
    let mut persisted_refs = 0usize;
    let mut resolved = 0usize;
    let mut attachments = 0usize;
    for session in &plan.sessions {
        let file: PathBuf = projects
            .join(&session.target_dir)
            .join(format!("{}.jsonl", session.session_id));
        let summary = parser::parse_conversation_file(&file).expect("imported transcript parses");
        assert_eq!(
            summary.project.as_deref(),
            Some(session.target_cwd.as_str())
        );
        let detail = parser::parse_conversation_detail(&file).expect("imported detail parses");
        assert_eq!(detail.project, session.target_cwd);
        assert!(detail.messages.iter().flat_map(|m| m.blocks.iter()).all(
            |b| b.kind != "thinking" || b.text.as_deref().is_some_and(|t| !t.trim().is_empty())
        ));
        messages += detail.messages.len();
        for block in detail.messages.iter().flat_map(|m| m.blocks.iter()) {
            if block.kind == "attachment" {
                attachments += 1;
            }
            if let Some(reference) = &block.persisted_output {
                persisted_refs += 1;
                if let Some((_, relative)) = persisted::resolve(&reference.path, &projects, &file) {
                    let (text, truncated, size) =
                        persisted::read(&projects, &relative, 8 * 1024 * 1024).unwrap();
                    assert!(!text.is_empty() && !truncated && size > 0);
                    resolved += 1;
                }
            }
        }
    }
    println!(
        "import smoke: sessions={} files_written={} messages={} attachments={} persisted_refs={} resolved={}",
        result.added, result.files_written, messages, attachments, persisted_refs, resolved
    );
    let _ = fs::remove_dir_all(&root);
}
