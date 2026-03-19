//! 历史总结服务：负责校验总结请求、读取源消息并调用 AI 总结引擎，
//! 让 command 层只负责参数进出，不再承载总结业务规则。
use crate::application::history::query_service;
use crate::config::{load_app_config, ConfigDir};
use crate::db::MessageDb;
use crate::history_summary::{
    GlobalSummaryResult, HistorySummaryParticipantRef, HistorySummaryResult,
    HistorySummaryService, SummaryRange, SummaryScope,
};
use std::sync::Arc;

/// 生成一份历史总结结果。
/// 业务约束：会话必须已选中；成员总结必须选定成员；空消息范围直接返回空总结而不是调用 AI。
pub(crate) async fn generate_history_summary(
    config_dir: &ConfigDir,
    db: &Arc<MessageDb>,
    chat_name: String,
    scope: String,
    participant_id: Option<String>,
    start_date: String,
    end_date: String,
) -> Result<HistorySummaryResult, String> {
    if chat_name.trim().is_empty() {
        return Err("请先选择一个会话".to_string());
    }

    let scope = SummaryScope::parse(scope.trim()).map_err(|error| error.to_string())?;
    let range = SummaryRange::parse(&start_date, &end_date).map_err(|error| error.to_string())?;
    let participants = query_service::list_summary_participants(db, &chat_name, &range)?;

    let selected_participant = resolve_selected_participant(&scope, participant_id, &participants)?;

    let messages = db
        .query_summary_messages(
            &chat_name,
            &range.start_date,
            &range.end_date,
            selected_participant.as_ref().map(|item| item.id.as_str()),
        )
        .map_err(|error| error.to_string())?;

    if messages.is_empty() {
        return Ok(HistorySummaryResult {
            scope: scope.as_str().to_string(),
            chat_name,
            participant: selected_participant
                .as_ref()
                .map(|item| HistorySummaryParticipantRef {
                    id: item.id.clone(),
                    label: item.label.clone(),
                }),
            start_date: range.start_date,
            end_date: range.end_date,
            message_count: 0,
            participant_count: participants.len(),
            overall_summary: String::new(),
            daily_items: Vec::new(),
        });
    }

    let config = load_app_config(&config_dir.0).map_err(|error| error.to_string())?;
    let service = HistorySummaryService::from_translate_config(&config.translate)
        .map_err(|error| error.to_string())?;

    service
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
        .map_err(|error| error.to_string())
}

/// 解析总结请求中的成员选择。
/// 业务约束：只有成员总结模式才要求 participant_id，且成员必须在当前时间范围内有可用消息。
fn resolve_selected_participant(
    scope: &SummaryScope,
    participant_id: Option<String>,
    participants: &[crate::db::HistorySummaryParticipant],
) -> Result<Option<crate::db::HistorySummaryParticipant>, String> {
    if *scope != SummaryScope::Participant {
        return Ok(None);
    }

    let participant_id = participant_id
        .as_deref()
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .ok_or_else(|| "成员总结需要先选择一个成员".to_string())?;

    participants
        .iter()
        .find(|item| item.id == participant_id)
        .cloned()
        .map(Some)
        .ok_or_else(|| "所选成员在当前时间范围内没有可用消息".to_string())
}

/// 生成跨所有群聊的全局整体总结。
pub(crate) async fn generate_global_summary(
    config_dir: &ConfigDir,
    db: &Arc<MessageDb>,
    start_date: String,
    end_date: String,
) -> Result<GlobalSummaryResult, String> {
    let range = SummaryRange::parse(&start_date, &end_date).map_err(|error| error.to_string())?;

    let chats = db
        .list_global_summary_chats(&range.start_date, &range.end_date)
        .map_err(|error| error.to_string())?;

    let messages = db
        .query_global_summary_messages(&range.start_date, &range.end_date)
        .map_err(|error| error.to_string())?;

    if messages.is_empty() {
        return Ok(GlobalSummaryResult {
            scope: SummaryScope::Global.as_str().to_string(),
            start_date: range.start_date,
            end_date: range.end_date,
            message_count: 0,
            chat_count: chats.len(),
            overall_summary: String::new(),
            daily_items: Vec::new(),
        });
    }

    let config = load_app_config(&config_dir.0).map_err(|error| error.to_string())?;
    let service = HistorySummaryService::from_translate_config(&config.translate)
        .map_err(|error| error.to_string())?;

    service
        .summarize_global(
            chats.len(),
            &range.start_date,
            &range.end_date,
            &messages,
        )
        .await
        .map_err(|error| error.to_string())
}
