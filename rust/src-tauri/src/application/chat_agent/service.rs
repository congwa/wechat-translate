//! Chat Agent 应用服务：根据 AI 配置动态构建 Rig Agent，执行多轮 Text2SQL 对话。
use crate::application::chat_agent::session::SessionStore;
use crate::application::chat_agent::tools::context_tool::ContextTool;
use crate::application::chat_agent::tools::schema_tool::SchemaTool;
use crate::application::chat_agent::tools::sql_tool::SqlTool;
use crate::application::chat_agent::tools::{ToolCallEvent, ToolEventBuffer};
use crate::config::TranslateConfig;
use crate::db::MessageDb;
use rig::client::CompletionClient;
// Chat: agent.chat(msg, history) → 经由 Chat trait 高阶封装，history 传值（owned）
// Prompt: agent.prompt(msg) → 返回 PromptRequest builder，可链式调用 .with_history() / .max_turns()
// 方式 B 启用时需将下行取消注释：use rig::completion::Prompt;
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

        // =====================================================================
        // 多步推理（Agentic Loop）配置：两种方式对比
        //
        // 【方式 A：Agent Builder 层设置 default_max_turns（当前使用）】
        //
        //   调用链：
        //     agent.chat(msg, history)
        //     → Chat trait impl (agent/completion.rs:277)
        //       → PromptRequest::from_agent(self, prompt)   ← 读取 agent.default_max_turns
        //         → .with_history(&mut chat_history)
        //         → .await → PromptRequest::send()
        //           → loop { if current_turn > max_turns + 1 { break → MaxTurnsError } }
        //
        //   特点：
        //     - 在 Agent 实例层面统一设置，所有 chat() 调用共享同一上限
        //     - history 以 owned Vec<Message> 传入，内部转为 &mut
        //     - 语法最简洁，无需更改调用方
        //
        // 【方式 B：PromptRequest Builder 层设置 max_turns（注释展示）】
        //
        //   调用链：
        //     agent.prompt(msg)                      ← Prompt trait，返回 PromptRequest builder
        //     → .with_history(&mut history)          ← 直接绑定 &mut Vec<Message>（非 owned）
        //     → .max_turns(5)                        ← 覆写本次请求的 max_turns，不影响 Agent 默认值
        //     → .await → PromptRequest::send()       ← 与方式 A 最终进入同一 send() 逻辑
        //
        //   特点：
        //     - 在单次请求层面按需覆盖，灵活控制每次对话的轮次上限
        //     - history 必须是 &mut Vec<Message>（生命周期绑定到 PromptRequest future）
        //     - 绕过 Chat trait，直接使用 Prompt trait
        //     - 适合同一 Agent 在不同场景需要不同轮次上限时
        //
        // 两者最终都进入 PromptRequest::send() 的 loop 块：
        //   loop 退出条件：current_max_turns > max_turns + 1
        //   即 max_turns=5 时最多执行 7 次循环（允许 5 次工具调用 round-trip）
        // =====================================================================

        // ── 方式 A（使用中）：在 Builder 上设置 default_max_turns ──────────────
        // Text2SQL 典型路径：get_context → get_schema → execute_sql → 文本答案（3 次工具调用）
        // 设为 5 可应对更复杂的多步推理，同时避免无限循环烧光 Token
        let agent = client
            .agent(&model_id)
            .preamble(&preamble)
            .tool(schema_tool)
            .tool(sql_tool)
            .tool(context_tool)
            .default_max_turns(5) // 允许最多 5 次工具调用 round-trip 后再返回文本答案
            .build();

        // ── 方式 B（注释示例）：在单次请求上设置 max_turns ───────────────────
        // 如需按请求覆盖，可将下方注释解开并替换方式 A 的调用：
        //
        // let mut history = history; // 需要 &mut Vec<Message> 而非 owned
        // let result: Result<String, String> = agent
        //     .prompt(user_message)      // Prompt trait，返回 PromptRequest
        //     .with_history(&mut history) // 绑定 &mut，history 生命周期需覆盖整个 await
        //     .max_turns(5)              // 覆写本次请求的轮次上限
        //     .await
        //     .map_err(|e: rig::completion::PromptError| e.to_string());

        // ── 方式 A 实际调用 ──────────────────────────────────────────────────
        // Chat::chat() 内部等价于：
        //   PromptRequest::from_agent(self, prompt)  // 读 agent.default_max_turns = Some(5)
        //     .with_history(&mut chat_history)
        //     .await
        let result: Result<String, String> = agent
            .chat(user_message, history)
            .await
            .map_err(|e: rig::completion::PromptError| {
                let raw = e.to_string();
                // 部分模型/代理不支持 Function Calling，给出明确提示
                if raw.contains("Function call is not supported")
                    || raw.contains("function_call")
                    || raw.contains("tool_use")
                    || raw.contains("tools is not supported")
                {
                    "当前模型不支持 Function Calling（工具调用）。\n\
                     Agent 功能依赖工具调用能力，请在设置中切换到支持该功能的模型，\
                     例如 gpt-4o、gpt-4-turbo、gpt-3.5-turbo 等 OpenAI 兼容模型。\n\
                     原始错误：".to_string() + &raw
                } else {
                    raw
                }
            });

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
