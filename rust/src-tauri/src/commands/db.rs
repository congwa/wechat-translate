//! 数据库命令入口：承接“清库重启”这类维护动作。
//! 历史消息、聊天列表、统计等只读查询已经迁移到 `interface/queries/history.rs`。
use crate::application::runtime::service::RuntimeService;
use crate::db::MessageDb;
use crate::events::{EventStore, EventType};
use crate::task_manager::TaskManager;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};

pub async fn clear_restart(
    app: AppHandle,
    db: Arc<MessageDb>,
    manager: TaskManager,
) -> Result<serde_json::Value, String> {
    // 清库重启必须先停掉运行态再清表，避免后台监听继续往刚清空的数据库写脏数据。
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

pub async fn db_clear_restart(
    app: tauri::AppHandle,
    db: tauri::State<'_, Arc<MessageDb>>,
    manager: tauri::State<'_, TaskManager>,
) -> Result<serde_json::Value, String> {
    // 对外暴露清库重启动作，供设置页和调试入口以单一命令恢复本地基线。
    clear_restart(app, db.inner().clone(), manager.inner().clone()).await
}
