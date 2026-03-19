//! 运行时命令入口：负责把监听启停、运行态探活和授权恢复这类写操作通过 Tauri 暴露给前端，
//! 让运行时生命周期的写边界落在 `interface/commands/runtime`。
use crate::commands;
use crate::config::ConfigDir;
use crate::task_manager::TaskManager;
use serde_json::Value;

/// 启动监听主循环，开始轮询微信会话与消息变化。
#[tauri::command]
pub async fn listen_start(
    config_dir: tauri::State<'_, ConfigDir>,
    manager: tauri::State<'_, TaskManager>,
    interval_seconds: Option<f64>,
) -> Result<serde_json::Value, String> {
    commands::listen::listen_start(config_dir, manager, interval_seconds).await
}

/// 停止监听主循环，释放当前消息监听任务。
#[tauri::command]
pub async fn listen_stop(
    manager: tauri::State<'_, TaskManager>,
) -> Result<serde_json::Value, String> {
    commands::listen::listen_stop(manager).await
}

/// 查询当前监听/浮窗等任务状态，供设置页和健康检查展示运行态。
#[tauri::command]
pub async fn get_task_status(
    manager: tauri::State<'_, TaskManager>,
) -> Result<serde_json::Value, String> {
    commands::listen::get_task_status(manager).await
}

/// 返回统一的运行时健康快照，供探活和诊断面板消费。
#[tauri::command]
pub async fn health_check(
    manager: tauri::State<'_, TaskManager>,
) -> Result<serde_json::Value, String> {
    commands::listen::health_check(manager).await
}

/// 在辅助功能权限恢复后触发监听重建，避免当前进程继续沿用旧的不可用运行态。
#[tauri::command]
pub async fn accessibility_recover_listener(
    app: tauri::AppHandle,
    config_dir: tauri::State<'_, ConfigDir>,
    manager: tauri::State<'_, TaskManager>,
) -> Result<Value, String> {
    commands::preflight::accessibility_recover_listener(app, config_dir, manager).await
}
