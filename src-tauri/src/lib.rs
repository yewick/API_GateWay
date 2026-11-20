mod adaptor;
mod client_setup;
mod commands;
mod core;
mod db;
mod protocol;
mod security;
mod server;
mod services;
mod tray;
mod utils;

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::RwLock;
use tauri::Manager;

/// 全局应用状态
pub struct AppState {
    pub db: Arc<db::Database>,
    pub server_port: Arc<tokio::sync::RwLock<u16>>,
    pub server_running: Arc<AtomicBool>,
    pub server_handle: Arc<RwLock<Option<tauri::async_runtime::JoinHandle<()>>>>,
    pub server_error: Arc<tokio::sync::RwLock<Option<server::ServerErrorInfo>>>,
    pub server_started_at: Arc<tokio::sync::RwLock<Option<String>>>,
    pub server_shutdown: Arc<tokio::sync::Mutex<Option<tokio::sync::oneshot::Sender<()>>>>,
    pub reconcile_report:
        Arc<tokio::sync::RwLock<Option<client_setup::reconcile::ReconcileReport>>>,
}

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();
    // 单实例只在发布构建启用：tauri dev 热重启时旧进程可能尚未完全退出，
    // 新实例会误判为双开并直接退出，导致 dev 会话中断。
    let builder = if cfg!(debug_assertions) {
        builder
    } else {
        builder.plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            let _ = tray::restore_main_window(app);
        }))
    };

    builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None::<Vec<&str>>,
        ))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            greet,
            // 渠道
            commands::channel::get_channels,
            commands::channel::create_channel,
            commands::channel::test_channel,
            commands::channel::toggle_channel,
            commands::channel::delete_channel,
            commands::channel::get_channel,
            commands::channel::update_channel,
            commands::channel::reorder_channels,
            // 密钥
            commands::api_key::get_api_keys,
            commands::api_key::create_api_key,
            commands::api_key::update_api_key,
            commands::api_key::delete_api_key,
            // 客户端接入
            commands::client_setup::client_setup_status,
            commands::client_setup::client_setup_list_models,
            commands::client_setup::client_setup_preview,
            commands::client_setup::client_setup_apply,
            commands::client_setup::client_setup_disconnect,
            commands::client_setup::client_setup_test,
            commands::client_setup::client_setup_open_dir,
            commands::client_setup::client_setup_reconcile,
            commands::client_setup::client_setup_last_report,
            commands::client_setup::client_setup_dismiss_keep_alive_hint,
            // 日志
            commands::logs::get_logs,
            commands::logs::get_log,
            commands::logs::delete_log,
            commands::logs::get_log_sessions,
            commands::logs::get_log_session,
            commands::logs::get_log_stats,
            commands::logs::get_mode_stats,
            commands::logs::get_log_findings,
            // 导出
            commands::export::write_text_file,
            // 仪表盘
            commands::dashboard::get_dashboard_stats,
            // 设置
            commands::settings::get_settings,
            commands::settings::save_settings,
            // 服务器
            commands::server::restart_server,
            commands::server::server_status,
            // 安全规则
            commands::security::get_builtin_security_rules,
            commands::security::update_builtin_security_rule,
            commands::security::reset_builtin_security_rules,
            commands::security::get_custom_security_rules,
            commands::security::create_custom_security_rule,
            commands::security::toggle_custom_security_rule,
            commands::security::delete_custom_security_rule,
            // 测试台
            commands::test::send_test_request,
            // MCP 测试台
            commands::mcp::send_mcp_request,
            // 服务状态
            commands::services::get_service_statuses,
            // 知识库
            commands::knowledge::get_knowledge_bases,
            commands::knowledge::get_knowledge_base,
            commands::knowledge::create_knowledge_base,
            commands::knowledge::update_knowledge_base,
            commands::knowledge::delete_knowledge_base,
            commands::knowledge::ask_knowledge_base,
            commands::knowledge::get_kb_conversations,
            commands::knowledge::delete_kb_conversations,
            commands::knowledge::list_kb_documents,
            commands::knowledge::get_kb_document,
            commands::knowledge::get_kb_document_content,
            commands::knowledge::list_kb_document_chunks,
            commands::knowledge::upload_kb_document,
            commands::knowledge::ingest_kb_document,
            commands::knowledge::delete_kb_document,
            commands::knowledge::get_kb_stats,
            commands::knowledge::build_kb_index,
            commands::knowledge::get_kb_index,
            commands::knowledge::search_kb,
            commands::knowledge::import_kb_source,
            commands::knowledge::list_kb_sources,
            commands::knowledge::delete_kb_source,
        ])
        .setup(|app| {
            let handle = app.handle().clone();

            tauri::async_runtime::block_on(async move {
                let db = db::Database::new(&handle).await;

                // FTS5 索引从「原文」一次性重建为「CJK bigram」内容（幂等，见 migration 017）
                crate::services::knowledge::ensure_fts_bigram_index(&db.pool).await;

                let state = Arc::new(AppState {
                    db: Arc::new(db),
                    server_port: Arc::new(tokio::sync::RwLock::new(0)),
                    server_running: Arc::new(AtomicBool::new(false)),
                    server_handle: Arc::new(RwLock::new(None)),
                    server_error: Arc::new(tokio::sync::RwLock::new(None)),
                    server_started_at: Arc::new(tokio::sync::RwLock::new(None)),
                    server_shutdown: Arc::new(tokio::sync::Mutex::new(None)),
                    reconcile_report: Arc::new(tokio::sync::RwLock::new(None)),
                });

                handle.manage(state.clone());

                // 系统托盘 + 窗口事件（关闭/最小化到托盘）
                tray::setup_tray(&handle)?;
                tray::setup_window_events(&handle)?;

                // 启动本地 HTTP 服务：先完成绑定再继续；失败会记录到状态并通知前端。
                if let Err(err) = server::start(handle.clone(), state.clone()).await {
                    tracing::error!("本地网关启动失败: {err}");
                }

                // 开机自启（仅 release 构建同步系统登录项，避免污染开发机）。
                commands::settings::sync_autostart(&handle);

                // 启动自检：幂等修复自己的客户端配置，不覆盖外部工具写入。
                let reconcile_app = handle.clone();
                let reconcile_state = state.clone();
                tauri::async_runtime::spawn(async move {
                    let report = client_setup::reconcile::reconcile_all(
                        &reconcile_app,
                        &reconcile_state,
                        client_setup::reconcile::RepairMode::Auto,
                    )
                    .await;
                    *reconcile_state.reconcile_report.write().await = Some(report);
                });

                // 历史日志会话字段补算（分批、幂等、不阻塞启动）。
                let backfill_repo =
                    std::sync::Arc::new(db::repository::Repository::new(state.db.pool.clone()));
                tauri::async_runtime::spawn(async move {
                    server::session_backfill::backfill_sessions(backfill_repo).await;
                });

                Ok::<(), tauri::Error>(())
            })?;

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = event {
                let _ = tray::restore_main_window(app);
            }
        });
}
