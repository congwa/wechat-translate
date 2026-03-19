//! Sidebar 查询入口：负责把后端 sidebar 投影和消息库组装成浮窗读模型，
//! 让前端只通过 snapshot 重建浮窗，而不是自己拼接 runtime 和消息数据。
use crate::application::runtime::service::RuntimeService;
use crate::application::sidebar::snapshot_service::load_sidebar_snapshot;
use crate::db::MessageDb;
use crate::task_manager::TaskManager;
use std::sync::Arc;

/// 返回当前 sidebar 快照，供侧边栏窗口在收到 invalidation 后整包替换本地状态。
#[tauri::command]
pub async fn sidebar_snapshot_get(
    db: tauri::State<'_, Arc<MessageDb>>,
    manager: tauri::State<'_, TaskManager>,
    chat_name: Option<String>,
    limit: Option<i64>,
) -> Result<serde_json::Value, String> {
    let runtime = RuntimeService::new(manager.inner().clone());
    let snapshot = load_sidebar_snapshot(db.inner(), &runtime, chat_name, limit).await?;
    Ok(serde_json::json!({
        "ok": true,
        "data": snapshot,
    }))
}
