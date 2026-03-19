//! Agent 命令入口：暴露 Text2SQL Chat Agent 的 Tauri IPC 接口。
use crate::application::chat_agent::service::ChatAgentService;
use crate::config::ConfigDir;
use tauri::{AppHandle, State};

/// 创建新的 Agent 会话，返回 session_id
#[tauri::command]
pub async fn agent_session_new(
    agent_service: State<'_, ChatAgentService>,
) -> Result<serde_json::Value, String> {
    let session_id = agent_service.new_session();
    Ok(serde_json::json!({ "ok": true, "session_id": session_id }))
}

/// 发送一条消息给 Agent，Agent 执行 Text2SQL 并通过 "agent-chat-response" 事件返回结果
#[tauri::command]
pub async fn agent_chat(
    session_id: String,
    message: String,
    agent_service: State<'_, ChatAgentService>,
    config_dir: State<'_, ConfigDir>,
    app_handle: AppHandle,
) -> Result<serde_json::Value, String> {
    let config = crate::config::load_app_config(&config_dir.0).map_err(|e| e.to_string())?;

    agent_service
        .run_chat(&session_id, &message, &config.translate, &app_handle)
        .await?;

    Ok(serde_json::json!({ "ok": true }))
}

/// 清空指定会话的历史记录
#[tauri::command]
pub async fn agent_session_clear(
    session_id: String,
    agent_service: State<'_, ChatAgentService>,
) -> Result<serde_json::Value, String> {
    agent_service.clear_session(&session_id);
    Ok(serde_json::json!({ "ok": true }))
}
