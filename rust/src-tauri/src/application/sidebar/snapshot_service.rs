//! Sidebar 快照服务：统一负责根据当前 sidebar 投影和消息库生成前端读模型，
//! 让 command 层只负责 Tauri 参数拆装，不再自己拼装 sidebar snapshot。
use crate::application::runtime::service::RuntimeService;
use crate::db::{MessageDb, StoredMessage};
use crate::translator::TranslatorServiceStatus;
use serde::Serialize;
use std::sync::Arc;

/// SidebarSnapshotData 是前端浮窗页面消费的完整读模型，
/// 它把当前聊天、消息列表、翻译健康和刷新版本放在同一份 snapshot 中返回。
#[derive(Debug, Clone, Serialize)]
pub(crate) struct SidebarSnapshotData {
    pub version: u64,
    pub current_chat: Option<String>,
    pub messages: Vec<StoredMessage>,
    pub translator: TranslatorServiceStatus,
    pub refresh_version: u64,
}

/// 读取当前 sidebar 快照，优先以后端 sidebar projection 中的 current chat 作为真相，
/// 前端传入的 chat_name 只作为 projection 为空时的兜底选择。
pub(crate) async fn load_sidebar_snapshot(
    db: &Arc<MessageDb>,
    runtime: &RuntimeService,
    chat_name: Option<String>,
    limit: Option<i64>,
) -> Result<SidebarSnapshotData, String> {
    let sidebar_runtime = runtime.sidebar_runtime();
    let runtime_chat = sidebar_runtime.get_current_chat();
    let refresh_version = sidebar_runtime.get_refresh_version();
    let selected_chat = resolve_selected_chat(db, runtime_chat, chat_name)?;
    let messages = query_sidebar_messages(db, selected_chat.as_deref(), limit)?;
    let translator = runtime.translator_status().await;

    Ok(SidebarSnapshotData {
        version: refresh_version,
        current_chat: selected_chat,
        messages,
        translator,
        refresh_version,
    })
}

/// 决定 sidebar 应该展示哪个聊天：
/// 优先使用后端投影里的 current chat，其次使用前端传入的候选 chat，最后回退到数据库最近一条消息的聊天。
fn resolve_selected_chat(
    db: &Arc<MessageDb>,
    runtime_chat: String,
    chat_name: Option<String>,
) -> Result<Option<String>, String> {
    if !runtime_chat.is_empty() {
        return Ok(Some(runtime_chat));
    }

    match chat_name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        Some(name) => Ok(Some(name.to_string())),
        None => db.latest_chat_name().map_err(|error| error.to_string()),
    }
}

/// 读取 sidebar 当前聊天最近一批消息，保持与现有浮窗页面一致的消息排序和数量限制。
fn query_sidebar_messages(
    db: &Arc<MessageDb>,
    chat_name: Option<&str>,
    limit: Option<i64>,
) -> Result<Vec<StoredMessage>, String> {
    match chat_name {
        Some(chat) => db
            .query_messages(Some(chat), None, None, limit.unwrap_or(50), 0)
            .map_err(|error| error.to_string()),
        None => Ok(Vec::new()),
    }
}
