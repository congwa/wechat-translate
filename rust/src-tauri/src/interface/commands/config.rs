//! 配置写命令入口：负责把设置保存、原始配置读写和默认配置模板通过 Tauri 暴露给前端，
//! 让“配置真相”这条写链路逐步从旧 `commands/*` 目录迁到 `interface/commands`。
use crate::commands;
use crate::config::ConfigDir;
use crate::task_manager::TaskManager;
use crate::CloseToTray;

/// 保存结构化 settings，并同步触发 settings/runtime 快照更新。
#[tauri::command]
pub async fn settings_update(
    app: tauri::AppHandle,
    config_dir: tauri::State<'_, ConfigDir>,
    manager: tauri::State<'_, TaskManager>,
    close_to_tray: tauri::State<'_, CloseToTray>,
    settings: serde_json::Value,
) -> Result<serde_json::Value, String> {
    commands::app_state::settings_update(app, config_dir, manager, close_to_tray, settings).await
}

/// 读取当前配置原始 JSON，供设置页高级编辑器和兼容调试入口使用。
#[tauri::command]
pub async fn config_get(
    config_dir: tauri::State<'_, ConfigDir>,
) -> Result<serde_json::Value, String> {
    commands::config::config_get(config_dir).await
}

/// 保存完整配置 JSON，并触发后端运行态按新配置重建或刷新。
#[tauri::command]
pub async fn config_put(
    app: tauri::AppHandle,
    config_dir: tauri::State<'_, ConfigDir>,
    manager: tauri::State<'_, TaskManager>,
    config: serde_json::Value,
) -> Result<serde_json::Value, String> {
    commands::config::config_put(app, config_dir, manager, config).await
}

/// 返回默认配置模板，供高级配置编辑器一键恢复默认值。
#[tauri::command]
pub async fn config_default() -> Result<serde_json::Value, String> {
    commands::config::config_default().await
}
