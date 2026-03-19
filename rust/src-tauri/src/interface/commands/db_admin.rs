//! 数据库管理命令入口：负责把清库重启这类运维动作通过 Tauri 暴露给前端，
//! 避免运维写操作继续混在历史查询模块中。
use crate::application::runtime::service::RuntimeService;
use crate::db::MessageDb;
use crate::events::{EventStore, EventType};
use crate::task_manager::TaskManager;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};

/// 清空本地消息库并重启监听。
/// 业务约束：必须先停运行态、再清库、再重启监听，避免后台任务继续往刚清空的数据库写入脏数据。
pub(crate) async fn clear_restart_command(
    app: AppHandle,
    db: Arc<MessageDb>,
    manager: TaskManager,
) -> Result<serde_json::Value, String> {
    let runtime = RuntimeService::new(manager);
    runtime
        .stop_all_and_wait(std::time::Duration::from_secs(3))
        .await
        .map_err(|e| e.to_string())?;
    db.clear_all().map_err(|e| e.to_string())?;
    runtime
        .start_monitoring(1.0)
        .await
        .map_err(|e| e.to_string())?;

    if let Some(events) = app.try_state::<Arc<EventStore>>() {
        events.publish(
            &app,
            EventType::Log,
            "db",
            serde_json::json!({
                "message": "数据库已清空，监听已重启",
            }),
        );
    }
    let _ = app.emit(
        "db-cleared-restart",
        serde_json::json!({
            "ok": true,
            "message": "数据库已清空，监听已重启",
        }),
    );

    Ok(serde_json::json!({ "ok": true, "message": "数据库已清空，监听已重启" }))
}

/// 清空本地消息库并重启监听，用于恢复测试基线或重新初始化运行环境。
#[tauri::command]
pub async fn db_clear_restart(
    app: tauri::AppHandle,
    db: tauri::State<'_, Arc<MessageDb>>,
    manager: tauri::State<'_, TaskManager>,
) -> Result<serde_json::Value, String> {
    clear_restart_command(app, db.inner().clone(), manager.inner().clone()).await
}
