//! 历史查询服务：负责把消息库中的历史消息、会话列表和成员列表组织成前端可消费的读模型，
//! 避免 command 层直接调用底层数据库方法拼装响应。
use crate::db::{ChatSummary, DbStats, HistorySummaryParticipant, MessageDb, StoredMessage};
use crate::history_summary::SummaryRange;
use std::sync::Arc;

/// 查询历史消息列表，供历史页分页浏览与筛选使用。
/// 业务约束：limit/offset 由上层传入，service 只负责把可选筛选项映射到数据库查询。
pub(crate) fn query_messages(
    db: &Arc<MessageDb>,
    chat_name: Option<String>,
    sender: Option<String>,
    keyword: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Vec<StoredMessage>, String> {
    db.query_messages(
        chat_name.as_deref(),
        sender.as_deref(),
        keyword.as_deref(),
        limit.unwrap_or(50),
        offset.unwrap_or(0),
    )
    .map_err(|error| error.to_string())
}

/// 查询历史会话列表，供历史页左侧会话筛选器展示最近活跃的聊天摘要。
pub(crate) fn get_chats(db: &Arc<MessageDb>) -> Result<Vec<ChatSummary>, String> {
    db.get_chat_list().map_err(|error| error.to_string())
}

/// 查询历史库统计，供调试页或设置页展示当前消息库规模。
pub(crate) fn get_stats(db: &Arc<MessageDb>) -> Result<DbStats, String> {
    db.get_stats().map_err(|error| error.to_string())
}

/// 查询某个会话在指定时间范围内可参与总结的成员列表。
/// 业务约束：会话名不能为空，日期范围必须先通过 SummaryRange 校验。
pub(crate) fn list_summary_participants(
    db: &Arc<MessageDb>,
    chat_name: &str,
    range: &SummaryRange,
) -> Result<Vec<HistorySummaryParticipant>, String> {
    if chat_name.trim().is_empty() {
        return Err("请先选择一个会话".to_string());
    }

    db.list_summary_participants(chat_name, &range.start_date, &range.end_date)
        .map_err(|error| error.to_string())
}
