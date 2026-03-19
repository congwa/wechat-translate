//! 数据库管理命令入口：负责把清库重启这类运维动作通过 Tauri 暴露给前端，
//! 避免运维写操作继续混在历史查询模块中。
use crate::commands;
use crate::db::MessageDb;
use crate::task_manager::TaskManager;
use std::sync::Arc;

/// 清空本地消息库并重启监听，用于恢复测试基线或重新初始化运行环境。
#[tauri::command]
pub async fn db_clear_restart(
    app: tauri::AppHandle,
    db: tauri::State<'_, Arc<MessageDb>>,
    manager: tauri::State<'_, TaskManager>,
) -> Result<serde_json::Value, String> {
    commands::db::db_clear_restart(app, db, manager).await
}
