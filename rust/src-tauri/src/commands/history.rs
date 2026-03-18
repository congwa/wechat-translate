use crate::config::{load_app_config, ConfigDir};
use crate::db::MessageDb;
use crate::history_summary::{
    HistorySummaryParticipantRef, HistorySummaryResult, HistorySummaryService, SummaryRange,
    SummaryScope,
};
use std::sync::Arc;

#[tauri::command]
pub async fn history_summary_participants_get(
    db: tauri::State<'_, Arc<MessageDb>>,
    chat_name: String,
    start_date: String,
    end_date: String,
) -> Result<serde_json::Value, String> {
    if chat_name.trim().is_empty() {
        return Err("请先选择一个会话".to_string());
    }
    let range = SummaryRange::parse(&start_date, &end_date).map_err(|e| e.to_string())?;
    let participants = db
        .list_summary_participants(&chat_name, &range.start_date, &range.end_date)
        .map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "ok": true,
        "data": participants,
    }))
}

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
    if chat_name.trim().is_empty() {
        return Err("请先选择一个会话".to_string());
    }

    let scope = SummaryScope::parse(scope.trim()).map_err(|e| e.to_string())?;
    let range = SummaryRange::parse(&start_date, &end_date).map_err(|e| e.to_string())?;
    let participants = db
        .list_summary_participants(&chat_name, &range.start_date, &range.end_date)
        .map_err(|e| e.to_string())?;

    let selected_participant = if scope == SummaryScope::Participant {
        let participant_id = participant_id
            .as_deref()
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .ok_or_else(|| "成员总结需要先选择一个成员".to_string())?;
        Some(
            participants
                .iter()
                .find(|item| item.id == participant_id)
                .cloned()
                .ok_or_else(|| "所选成员在当前时间范围内没有可用消息".to_string())?,
        )
    } else {
        None
    };

    let messages = db
        .query_summary_messages(
            &chat_name,
            &range.start_date,
            &range.end_date,
            selected_participant.as_ref().map(|item| item.id.as_str()),
        )
        .map_err(|e| e.to_string())?;

    if messages.is_empty() {
        return Ok(serde_json::json!({
            "ok": true,
            "data": HistorySummaryResult {
                scope: scope.as_str().to_string(),
                chat_name,
                participant: selected_participant.as_ref().map(|item| HistorySummaryParticipantRef {
                    id: item.id.clone(),
                    label: item.label.clone(),
                }),
                start_date: range.start_date,
                end_date: range.end_date,
                message_count: 0,
                participant_count: participants.len(),
                overall_summary: String::new(),
                daily_items: Vec::new(),
            }
        }));
    }

    let config = load_app_config(&config_dir.0).map_err(|e| e.to_string())?;
    let service = HistorySummaryService::from_translate_config(&config.translate)
        .map_err(|e| e.to_string())?;

    let result = service
        .summarize(
            scope,
            &chat_name,
            selected_participant.as_ref(),
            participants.len(),
            &range.start_date,
            &range.end_date,
            &messages,
        )
        .await
        .map_err(|e| e.to_string())?;

    Ok(serde_json::json!({
        "ok": true,
        "data": result,
    }))
}
