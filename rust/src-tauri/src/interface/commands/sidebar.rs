//! Sidebar 写命令入口：负责把浮窗启停、联动启动、窗口控制和手动翻译通过 Tauri 暴露给前端，
//! 让 sidebar 相关写操作逐步从旧 `commands/sidebar.rs` 迁移到 `interface/commands/sidebar.rs`。
use crate::commands;
use crate::config::ConfigDir;
use crate::sidebar_window::SidebarWindowState;
use crate::task_manager::TaskManager;
use std::sync::Arc;

/// 启用 sidebar 运行态，并按当前翻译配置装配译文器与目标会话集合。
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn sidebar_start(
    config_dir: tauri::State<'_, ConfigDir>,
    manager: tauri::State<'_, TaskManager>,
    targets: Option<Vec<String>>,
    translate_enabled: Option<bool>,
    provider: Option<String>,
    deeplx_url: Option<String>,
    ai_provider_id: Option<String>,
    ai_model_id: Option<String>,
    ai_api_key: Option<String>,
    ai_base_url: Option<String>,
    source_lang: Option<String>,
    target_lang: Option<String>,
    timeout_seconds: Option<f64>,
    max_concurrency: Option<usize>,
    max_requests_per_second: Option<usize>,
    image_capture: Option<bool>,
) -> Result<serde_json::Value, String> {
    commands::sidebar::sidebar_start(
        config_dir,
        manager,
        targets,
        translate_enabled,
        provider,
        deeplx_url,
        ai_provider_id,
        ai_model_id,
        ai_api_key,
        ai_base_url,
        source_lang,
        target_lang,
        timeout_seconds,
        max_concurrency,
        max_requests_per_second,
        image_capture,
    )
    .await
}

/// 关闭 sidebar 运行态并收起浮窗窗口，恢复到无浮窗的运行状态。
#[tauri::command]
pub async fn sidebar_stop(
    app: tauri::AppHandle,
    manager: tauri::State<'_, TaskManager>,
    sidebar_state: tauri::State<'_, Arc<SidebarWindowState>>,
) -> Result<serde_json::Value, String> {
    commands::sidebar::sidebar_stop(app, manager, sidebar_state).await
}

/// 一次性启动监听与 sidebar，供主窗口进入实时浮窗模式时使用。
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn live_start(
    app: tauri::AppHandle,
    config_dir: tauri::State<'_, ConfigDir>,
    manager: tauri::State<'_, TaskManager>,
    sidebar_state: tauri::State<'_, Arc<SidebarWindowState>>,
    translate_enabled: Option<bool>,
    provider: Option<String>,
    deeplx_url: Option<String>,
    ai_provider_id: Option<String>,
    ai_model_id: Option<String>,
    ai_api_key: Option<String>,
    ai_base_url: Option<String>,
    source_lang: Option<String>,
    target_lang: Option<String>,
    interval_seconds: Option<f64>,
    timeout_seconds: Option<f64>,
    max_concurrency: Option<usize>,
    max_requests_per_second: Option<usize>,
    image_capture: Option<bool>,
    window_mode: Option<String>,
) -> Result<serde_json::Value, String> {
    commands::sidebar::live_start(
        app,
        config_dir,
        manager,
        sidebar_state,
        translate_enabled,
        provider,
        deeplx_url,
        ai_provider_id,
        ai_model_id,
        ai_api_key,
        ai_base_url,
        source_lang,
        target_lang,
        interval_seconds,
        timeout_seconds,
        max_concurrency,
        max_requests_per_second,
        image_capture,
        window_mode,
    )
    .await
}

/// 仅打开 sidebar 窗口，不变更监听状态，供独立窗口模式和调试入口使用。
#[tauri::command]
pub async fn sidebar_window_open(
    app: tauri::AppHandle,
    config_dir: tauri::State<'_, ConfigDir>,
    state: tauri::State<'_, Arc<SidebarWindowState>>,
    width: Option<f64>,
    window_mode: Option<String>,
) -> Result<serde_json::Value, String> {
    commands::sidebar::sidebar_window_open(app, config_dir, state, width, window_mode).await
}

/// 关闭 sidebar 窗口，但不主动停掉监听任务。
#[tauri::command]
pub async fn sidebar_window_close(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<SidebarWindowState>>,
) -> Result<serde_json::Value, String> {
    commands::sidebar::sidebar_window_close(app, state).await
}

/// 测试当前翻译配置是否可用，供设置页即时校验外部翻译依赖。
#[tauri::command]
pub async fn translate_test(
    provider: Option<String>,
    deeplx_url: Option<String>,
    ai_provider_id: Option<String>,
    ai_model_id: Option<String>,
    ai_api_key: Option<String>,
    ai_base_url: Option<String>,
    source_lang: Option<String>,
    target_lang: Option<String>,
    timeout_seconds: Option<f64>,
) -> Result<serde_json::Value, String> {
    commands::sidebar::translate_test(
        provider,
        deeplx_url,
        ai_provider_id,
        ai_model_id,
        ai_api_key,
        ai_base_url,
        source_lang,
        target_lang,
        timeout_seconds,
    )
    .await
}

/// 对单条 sidebar 消息发起手动翻译，并复用后端统一的译文写回链路。
#[tauri::command]
pub async fn translate_sidebar_message(
    app: tauri::AppHandle,
    manager: tauri::State<'_, TaskManager>,
    message_id: u64,
    chat_name: String,
    sender: String,
    content: String,
    detected_at: String,
) -> Result<(), String> {
    commands::sidebar::translate_sidebar_message(
        app,
        manager,
        message_id,
        chat_name,
        sender,
        content,
        detected_at,
    )
    .await
}
