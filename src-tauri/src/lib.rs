// 模块声明为 pub：tests/ 下的集成测试（golden 测试）需要访问 parser 等模块。
pub mod codex_parser;
pub mod commands;
pub mod content_search;
pub mod export;
pub mod import;
pub mod indexer;
pub mod models;
pub mod parser;
pub mod persisted;
pub mod pricing;
pub mod state;

use state::AppState;

/// Tauri 应用入口
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::get_projects,
            commands::get_project_prompts,
            commands::get_recent_prompts,
            commands::search_prompts,
            commands::search_conversations,
            commands::get_stats,
            commands::get_project_sessions,
            commands::get_conversation,
            commands::get_index_meta,
            commands::refresh_index,
            commands::get_settings,
            commands::set_settings,
            commands::build_prompt_export,
            commands::export_search_results,
            commands::export_conversation,
            commands::export_sessions,
            commands::reveal_path,
            commands::read_persisted_output,
            commands::inspect_session_import,
            commands::plan_session_import,
            commands::apply_session_import,
        ])
        .run(tauri::generate_context!())
        .expect("启动 Tauri 应用失败");
}
