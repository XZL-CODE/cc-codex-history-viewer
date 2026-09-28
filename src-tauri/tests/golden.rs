//! Golden 测试：用「完全合成」的 fixture 锁定 JSONL 解析行为。
//! Claude Code 的 JSONL 格式是本工具最大的外部依赖——格式一变（或解析逻辑被误改），
//! 这里会第一时间报警。fixture 不含任何真实个人数据。

use cc_history_viewer_lib::parser;
use std::path::PathBuf;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn iso_ms(s: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(s)
        .unwrap()
        .timestamp_millis()
}

#[test]
fn history_golden() {
    let out = parser::parse_history(&fixture("sample_history.jsonl"));
    assert_eq!(out.len(), 3);

    assert_eq!(out[0].text, "帮我重构 parser 模块");
    assert_eq!(out[0].project, "/Users/dev/alpha");
    assert_eq!(out[0].timestamp, 1747363200000);
    assert_eq!(out[0].session_id.as_deref(), Some("sess-aaaa"));
    assert_eq!(out[0].pasted_count, 0, "空 pastedContents 应为 0");
    assert!(out[0].from_history);

    assert_eq!(out[1].text, "/clear");
    assert!(out[1].session_id.is_none());

    assert_eq!(out[2].project, "/Users/dev/beta");
    assert_eq!(out[2].pasted_count, 1, "pastedContents 有 1 个键");
}

#[test]
fn session_golden() {
    let path = fixture("f1e2d3c4-aaaa-bbbb-cccc-000000000001.jsonl");
    let r = parser::parse_conversation_file(&path).expect("fixture 应能解析");

    // 会话元信息
    assert_eq!(r.session_id, "f1e2d3c4-aaaa-bbbb-cccc-000000000001");
    assert_eq!(r.project.as_deref(), Some("/Users/dev/alpha"));
    assert_eq!(r.git_branch.as_deref(), Some("main"));
    assert_eq!(r.version.as_deref(), Some("2.0.0"));
    assert_eq!(r.started_at, iso_ms("2026-05-16T02:00:00.000Z"));
    assert_eq!(r.ended_at, iso_ms("2026-05-16T02:01:35.000Z"));
    // 8 行 user/assistant 计入消息数（system/local_command 行不算消息）
    assert_eq!(r.message_count, 8);

    // prompt 提取：tool_result-only 与 sidechain 的 user 行不算 prompt；
    // <command-name> 包裹的斜杠命令提取为命令名本身
    let texts: Vec<&str> = r.user_prompts.iter().map(|p| p.text.as_str()).collect();
    assert_eq!(texts, vec!["帮我重构 parser 模块", "/model"]);
    assert_eq!(r.first_prompt, "帮我重构 parser 模块");
    assert!(r
        .user_prompts
        .iter()
        .all(|p| p.project == "/Users/dev/alpha"));

    // 用量提取：msg_001 被拆成两行（resume/分块复制场景）只记一次；
    // sidechain 的 assistant 行同样计入用量
    assert_eq!(r.usage_entries.len(), 3);
    let keys: Vec<&str> = r
        .usage_entries
        .iter()
        .map(|e| e.dedup_key.as_str())
        .collect();
    assert_eq!(
        keys,
        vec![
            "claude:id:msg_001",
            "claude:id:msg_002",
            "claude:id:msg_003"
        ]
    );

    let input: u64 = r.usage_entries.iter().map(|e| e.usage.uncached_input).sum();
    let output: u64 = r.usage_entries.iter().map(|e| e.usage.output).sum();
    let cache_read: u64 = r.usage_entries.iter().map(|e| e.usage.cache_read).sum();
    let cache_creation: u64 = r.usage_entries.iter().map(|e| e.usage.cache_creation).sum();
    assert_eq!(input, 100 + 10 + 7);
    assert_eq!(output, 200 + 20 + 5);
    assert_eq!(cache_read, 1000);
    assert_eq!(cache_creation, 50);
    let total: u64 = r
        .usage_entries
        .iter()
        .map(|entry| entry.usage.total_tokens_including_cache())
        .sum();
    assert_eq!(
        total,
        input + cache_read + cache_creation + output,
        "Claude cache reads and cache creation count toward total tokens"
    );
    assert!(r
        .usage_entries
        .iter()
        .all(|entry| entry.usage.reasoning_output == 0));

    assert!(r.usage_entries[0].model.starts_with("claude-sonnet-4-5"));
    assert!(r.usage_entries[1].model.starts_with("claude-opus-4-5"));
}

#[test]
fn session_detail_golden() {
    let path = fixture("f1e2d3c4-aaaa-bbbb-cccc-000000000001.jsonl");
    let d = parser::parse_conversation_detail(&path).expect("fixture 应能解析");

    assert_eq!(d.project, "/Users/dev/alpha");
    // 8 条 user/assistant + 1 条 system/local_command 命令标记
    assert_eq!(d.messages.len(), 9);

    // 第 1 条：user 文本块
    assert_eq!(d.messages[0].role, "user");
    assert_eq!(d.messages[0].blocks[0].kind, "text");

    // 第 3 条（a-1b）：tool_use 块带工具名
    let tool_msg = &d.messages[2];
    assert_eq!(tool_msg.blocks[0].kind, "tool_use");
    assert_eq!(tool_msg.blocks[0].tool_name.as_deref(), Some("Bash"));

    // 斜杠命令的 user 消息在展示层被美化为「命令名」（标签噪声被剥离）
    assert_eq!(d.messages[5].blocks[0].text.as_deref(), Some("/model"));

    // system/local_command 行呈现为 role="system"，命令与参数拼接展示
    let sys_msg = &d.messages[6];
    assert_eq!(sys_msg.role, "system");
    assert_eq!(
        sys_msg.blocks[0].text.as_deref(),
        Some("/btw 顺便问一下进度")
    );

    // sidechain 标记保留
    assert!(d.messages[7].is_sidechain);

    // 用量挂在每次调用的首条记录上：msg_001 拆成 a-1 / a-1b 两行，只有 a-1 带用量
    let first_call = d.messages[1].usage.as_ref().expect("a-1 carries msg_001");
    assert_eq!(first_call.call_key, "claude:id:msg_001");
    assert_eq!(first_call.model, "claude-sonnet-4-5-20250929");
    assert_eq!(first_call.usage.uncached_input, 100);
    assert_eq!(first_call.usage.cache_read, 1_000);
    assert_eq!(first_call.usage.cache_creation, 50);
    assert_eq!(first_call.usage.cache_creation_1h, 0);
    assert_eq!(first_call.usage.output, 200);
    assert!(first_call.est_cost_usd.is_none(), "the parser never prices");
    assert!(
        first_call.attributed,
        "attribution is decided later by the index"
    );
    assert!(d.messages[2].usage.is_none(), "a-1b repeats msg_001");
    assert!(d.messages[3].usage.is_none(), "tool results carry no usage");
    assert_eq!(
        d.messages[4].usage.as_ref().map(|u| u.call_key.as_str()),
        Some("claude:id:msg_002")
    );
    assert_eq!(
        d.messages[8].usage.as_ref().map(|u| u.usage.output),
        Some(5),
        "sidechain calls keep their usage"
    );
    // 详情与索引对同一次调用算出同一个指纹
    let detail_keys: Vec<&str> = d
        .messages
        .iter()
        .filter_map(|m| m.usage.as_ref())
        .map(|u| u.call_key.as_str())
        .collect();
    let index_keys: Vec<String> = parser::parse_conversation_file(&path)
        .unwrap()
        .usage_entries
        .into_iter()
        .map(|e| e.dedup_key)
        .collect();
    assert_eq!(detail_keys, index_keys);
}
