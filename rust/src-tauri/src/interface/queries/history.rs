//! 历史查询入口：负责把消息历史、会话列表、统计和总结能力通过 Tauri 暴露给前端，
//! 让 query 边界落在 interface/queries，而不是继续堆在旧 commands 目录里。
use crate::application::history::query_service as history_query_service;
use crate::application::history::summary_service::generate_history_summary;
use crate::config::ConfigDir;
use crate::db::MessageDb;
use crate::history_summary::SummaryRange;
use std::sync::Arc;

/// 查询历史消息列表，供历史页按会话、成员和关键词筛选浏览。
#[tauri::command]
pub async fn db_query_messages(
    db: tauri::State<'_, Arc<MessageDb>>,
    chat_name: Option<String>,
    sender: Option<String>,
    keyword: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<serde_json::Value, String> {
    let messages = history_query_service::query_messages(
        db.inner(),
        chat_name,
        sender,
        keyword,
        limit,
        offset,
    )?;

    Ok(serde_json::json!({
        "ok": true,
        "data": messages,
    }))
}

/// 查询历史会话摘要列表，供历史页左侧会话切换和筛选使用。
#[tauri::command]
pub async fn db_get_chats(
    db: tauri::State<'_, Arc<MessageDb>>,
) -> Result<serde_json::Value, String> {
    let chats = history_query_service::get_chats(db.inner())?;
    Ok(serde_json::json!({
        "ok": true,
        "data": chats,
    }))
}

/// 查询本地消息库统计，供调试页或设置页展示当前数据库规模。
#[tauri::command]
pub async fn db_get_stats(
    db: tauri::State<'_, Arc<MessageDb>>,
) -> Result<serde_json::Value, String> {
    let stats = history_query_service::get_stats(db.inner())?;
    Ok(serde_json::json!({
        "ok": true,
        "data": stats,
    }))
}

/// 查询指定会话在时间范围内可供成员总结使用的成员列表。
#[tauri::command]
pub async fn history_summary_participants_get(
    db: tauri::State<'_, Arc<MessageDb>>,
    chat_name: String,
    start_date: String,
    end_date: String,
) -> Result<serde_json::Value, String> {
    let range = SummaryRange::parse(&start_date, &end_date).map_err(|e| e.to_string())?;
    let participants =
        history_query_service::list_summary_participants(db.inner(), &chat_name, &range)?;
    Ok(serde_json::json!({
        "ok": true,
        "data": participants,
    }))
}

/// 生成群聊或成员总结，内部统一走 application/history summary service 的业务规则。
#[tauri::command]
pub async fn history_summary_generate(
    config_dir: tauri::State<'_, ConfigDir>,
    db: tauri::State<'_, Arc<MessageDb>>,
    chat_name: String,
    scope: String,
    participant_id: Option<String>,
    start_date: String,
    end_date: String,
) -> Result<serde_json::Value, String> {
    let result = generate_history_summary(
        &config_dir,
        db.inner(),
        chat_name,
        scope,
        participant_id,
        start_date,
        end_date,
    )
    .await?;

    Ok(serde_json::json!({
        "ok": true,
        "data": result,
    }))
}
