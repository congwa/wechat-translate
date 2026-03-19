//! Context Tool: 返回当前监听上下文摘要（活跃群名、消息统计、时间范围等）。
use rig::{completion::ToolDefinition, tool::Tool};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::application::chat_agent::tools::{ToolCallEvent, ToolEventBuffer};
use crate::db::MessageDb;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ContextInput {}

#[derive(Debug, Serialize, Deserialize)]
pub struct ContextOutput {
    pub total_messages: i64,
    pub chat_count: i64,
    pub earliest_date: Option<String>,
    pub latest_date: Option<String>,
    pub top_chats: Vec<ChatSummary>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChatSummary {
    pub chat_name: String,
    pub message_count: i64,
}

pub struct ContextTool {
    pub db: Arc<MessageDb>,
    pub event_buffer: Arc<ToolEventBuffer>,
}

impl Tool for ContextTool {
    const NAME: &'static str = "get_context";

    type Error = crate::application::chat_agent::tools::ToolError;
    type Args = ContextInput;
    type Output = ContextOutput;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "返回当前消息数据库的总体统计摘要：总消息数、会话数、时间范围、最活跃的群聊列表。用于了解数据库的整体规模和数据范围。".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        }
    }

    async fn call(&self, _input: Self::Args) -> Result<Self::Output, Self::Error> {
        let result = self.db.get_context_summary().map_err(|e| e.to_string());

        match result {
            Ok((total_messages, chat_count, earliest_date, latest_date, raw_top)) => {
                let top_chats = raw_top
                    .into_iter()
                    .map(|(chat_name, message_count)| ChatSummary { chat_name, message_count })
                    .collect();

                let output = ContextOutput {
                    total_messages,
                    chat_count,
                    earliest_date,
                    latest_date,
                    top_chats,
                };

                self.event_buffer.push(ToolCallEvent {
                    tool_name: Self::NAME.to_string(),
                    input: serde_json::json!({}),
                    output: serde_json::to_string(&output).unwrap_or_default(),
                    is_error: false,
                });

                Ok(output)
            }
            Err(e) => {
                self.event_buffer.push(ToolCallEvent {
                    tool_name: Self::NAME.to_string(),
                    input: serde_json::json!({}),
                    output: e.clone(),
                    is_error: true,
                });
                Err(e.into())
            }
        }
    }
}
