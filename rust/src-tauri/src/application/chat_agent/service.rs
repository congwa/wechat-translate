//! Chat Agent 应用服务：根据 AI 配置动态构建 Rig Agent，执行多轮 Text2SQL 对话。
use crate::application::chat_agent::session::SessionStore;
use crate::application::chat_agent::tools::context_tool::ContextTool;
use crate::application::chat_agent::tools::schema_tool::SchemaTool;
use crate::application::chat_agent::tools::sql_tool::SqlTool;
use crate::application::chat_agent::tools::{ToolCallEvent, ToolEventBuffer};
use crate::config::TranslateConfig;
use crate::db::MessageDb;
use rig::client::{CompletionClient, ProviderClient};
use rig::completion::Chat;
use rig::providers::openai::CompletionsClient;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

/// 单次 agent chat 的完整响应，包含工具调用记录和最终回复
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentChatResponse {
    pub session_id: String,
    pub response: String,
    pub tool_calls: Vec<ToolCallEvent>,
    pub is_error: bool,
    pub error_message: Option<String>,
}

pub struct ChatAgentService {
    pub db: Arc<MessageDb>,
    pub sessions: Arc<SessionStore>,
}

impl ChatAgentService {
    pub fn new(db: Arc<MessageDb>) -> Self {
        Self {
            db,
            sessions: Arc::new(SessionStore::new()),
        }
    }

    pub fn new_session(&self) -> String {
        self.sessions.create()
    }

    pub fn clear_session(&self, session_id: &str) {
        self.sessions.clear(session_id);
    }

    pub async fn run_chat(
        &self,
        session_id: &str,
        user_message: &str,
        ai_config: &TranslateConfig,
        app_handle: &AppHandle,
    ) -> Result<AgentChatResponse, String> {
        if ai_config.ai_api_key.trim().is_empty() {
            return Err("AI API Key 未配置，请在设置中填写".to_string());
        }

        let history = self.sessions.get_rig_history(session_id);
        let event_buffer = Arc::new(ToolEventBuffer::new());

        let schema_tool = SchemaTool { db: self.db.clone() };
        let sql_tool = SqlTool {
            db: self.db.clone(),
            event_buffer: event_buffer.clone(),
        };
        let context_tool = ContextTool {
            db: self.db.clone(),
            event_buffer: event_buffer.clone(),
        };

        let api_key = ai_config.ai_api_key.trim();
        let model_id = if ai_config.ai_model_id.trim().is_empty() {
            "gpt-4o".to_string()
        } else {
            ai_config.ai_model_id.trim().to_string()
        };
        let preamble = build_preamble();

        let mut builder =
            CompletionsClient::<rig::http_client::ReqwestClient>::builder().api_key(api_key);
        if !ai_config.ai_base_url.trim().is_empty() {
            builder = builder.base_url(ai_config.ai_base_url.trim());
        }
        let client: CompletionsClient<rig::http_client::ReqwestClient> =
            builder.build().map_err(|e| e.to_string())?;

        let agent = client
            .agent(&model_id)
            .preamble(&preamble)
            .tool(schema_tool)
            .tool(sql_tool)
            .tool(context_tool)
            .build();

        let result: Result<String, String> = agent
            .chat(user_message, history)
            .await
            .map_err(|e: rig::completion::PromptError| e.to_string());

        let tool_calls = event_buffer.take();

        match result {
            Ok(response) => {
                self.sessions.push_user(session_id, user_message.to_string());
                self.sessions.push_assistant(session_id, response.clone());

                let chat_response = AgentChatResponse {
                    session_id: session_id.to_string(),
                    response,
                    tool_calls,
                    is_error: false,
                    error_message: None,
                };

                let _ = app_handle.emit("agent-chat-response", &chat_response);
                Ok(chat_response)
            }
            Err(e) => {
                let chat_response = AgentChatResponse {
                    session_id: session_id.to_string(),
                    response: String::new(),
                    tool_calls,
                    is_error: true,
                    error_message: Some(e.clone()),
                };
                let _ = app_handle.emit("agent-chat-response", &chat_response);
                Err(e)
            }
        }
    }
}

fn build_preamble() -> String {
    r#"你是微信消息数据分析助手，帮助用户用自然语言查询他们本地存储的微信消息记录。

数据库 Schema（SQLite）：
  messages(id, chat_name, sender, content, content_en, detected_at,
           is_self, image_path, source, quality, chat_type)
  message_translations(id, content_hash, source_lang, target_lang,
                       translated_text, updated_at)

说明：
  - detected_at 是 ISO8601 字符串，如 '2024-01-15T14:30:00'，可直接用字符串比较
  - is_self=1 表示自己发送的消息，0 表示他人发送
  - chat_name 是群聊或私聊的名称
  - content 是原文，content_en 是英文翻译（可能为空）
  - source 字段值为 'chat'（正常消息）或 'summary'（系统摘要）

使用步骤：
1. 先用 get_context 了解数据库整体规模和活跃群聊
2. 用 get_schema 确认字段细节
3. 用 execute_sql 执行 SELECT 查询
4. 基于结果用中文给出简洁清晰的分析

约束：只能执行只读查询，execute_sql 禁止写操作。"#
        .to_string()
}
