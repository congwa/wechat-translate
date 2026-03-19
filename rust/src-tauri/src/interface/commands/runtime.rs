//! 运行时命令入口：负责把监听启停、运行态探活和授权恢复这类写操作通过 Tauri 暴露给前端，
//! 让运行时生命周期的写边界落在 `interface/commands/runtime`。
use crate::application::runtime::service::RuntimeService;
use crate::config::ConfigDir;
use crate::task_manager::TaskManager;

/// 启动监听主循环，开始轮询微信会话与消息变化。
#[tauri::command]
pub async fn listen_start(
    config_dir: tauri::State<'_, ConfigDir>,
    manager: tauri::State<'_, TaskManager>,
    interval_seconds: Option<f64>,
) -> Result<serde_json::Value, String> {
    let runtime = RuntimeService::new(manager.inner().clone());
    let config = crate::config::load_app_config(&config_dir.0).map_err(|e| e.to_string())?;
    runtime
        .set_use_right_panel_details(config.listen.use_right_panel_details)
        .await;
    let interval = interval_seconds.unwrap_or(1.0);
    runtime
        .start_monitoring(interval)
        .await
        .map_err(|e| e.to_string())?;

    Ok(serde_json::json!({ "ok": true, "message": "monitoring started" }))
}

/// 停止监听主循环，释放当前消息监听任务。
#[tauri::command]
pub async fn listen_stop(
    manager: tauri::State<'_, TaskManager>,
) -> Result<serde_json::Value, String> {
    let runtime = RuntimeService::new(manager.inner().clone());
    runtime.stop_monitoring().await.map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "ok": true, "message": "monitoring stopped" }))
}

/// 查询当前监听/浮窗等任务状态，供设置页和健康检查展示运行态。
#[tauri::command]
pub async fn get_task_status(
    manager: tauri::State<'_, TaskManager>,
) -> Result<serde_json::Value, String> {
    let runtime = RuntimeService::new(manager.inner().clone());
    let status = runtime.service_status().await;
    Ok(serde_json::json!({ "ok": true, "data": status }))
}

/// 返回统一的运行时健康快照，供探活和诊断面板消费。
#[tauri::command]
pub async fn health_check(
    manager: tauri::State<'_, TaskManager>,
) -> Result<serde_json::Value, String> {
    let runtime = RuntimeService::new(manager.inner().clone());
    let status = runtime.service_status().await;
    Ok(serde_json::json!({ "status": "ok", "service_status": status }))
}
