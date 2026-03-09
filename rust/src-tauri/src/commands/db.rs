use crate::db::MessageDb;
use crate::task_manager::TaskManager;
use std::sync::Arc;

#[tauri::command]
pub async fn db_clear_restart(
    db: tauri::State<'_, Arc<MessageDb>>,
    manager: tauri::State<'_, TaskManager>,
) -> Result<serde_json::Value, String> {
    manager.stop_all().await;
    db.clear_all().map_err(|e| e.to_string())?;
    let _ = manager.start_monitoring(1.0).await;
    Ok(serde_json::json!({ "ok": true, "message": "数据库已清空，监听已重启" }))
}

#[tauri::command]
pub async fn db_query_messages(
    db: tauri::State<'_, Arc<MessageDb>>,
    chat_name: Option<String>,
    sender: Option<String>,
    keyword: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<serde_json::Value, String> {
    let messages = db
        .query_messages(
            chat_name.as_deref(),
            sender.as_deref(),
            keyword.as_deref(),
            limit.unwrap_or(50),
            offset.unwrap_or(0),
        )
        .map_err(|e| e.to_string())?;

    Ok(serde_json::json!({
        "ok": true,
        "data": messages,
    }))
}

#[tauri::command]
pub async fn db_get_chats(
    db: tauri::State<'_, Arc<MessageDb>>,
) -> Result<serde_json::Value, String> {
    let chats = db.get_chat_list().map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "ok": true,
        "data": chats,
    }))
}

#[tauri::command]
pub async fn db_get_stats(
    db: tauri::State<'_, Arc<MessageDb>>,
) -> Result<serde_json::Value, String> {
    let stats = db.get_stats().map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "ok": true,
        "data": stats,
    }))
}
