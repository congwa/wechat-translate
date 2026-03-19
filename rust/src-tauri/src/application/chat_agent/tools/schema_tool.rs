//! Schema Tool: 让 Agent 查询数据库表结构，辅助生成准确的 SQL。
use rig::{completion::ToolDefinition, tool::Tool};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::application::chat_agent::tools::ToolError;
use crate::db::MessageDb;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SchemaInput {
    /// 可选，指定表名；不填则返回所有表的 Schema
    pub table_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SchemaOutput {
    pub schema: String,
}

pub struct SchemaTool {
    pub db: Arc<MessageDb>,
}

const MESSAGES_SCHEMA: &str = r#"
Table: messages
  id           INTEGER PRIMARY KEY AUTOINCREMENT
  chat_name    TEXT NOT NULL                     -- 群聊或私聊名称
  sender       TEXT NOT NULL DEFAULT ''          -- 发送者名称
  content      TEXT NOT NULL                     -- 原始消息内容（中文/源语言）
  content_en   TEXT NOT NULL DEFAULT ''          -- 英文翻译内容（可能为空）
  content_hash TEXT NOT NULL                     -- 内容 MD5 哈希，用于去重
  detected_at  TEXT NOT NULL                     -- 检测时间，ISO8601 格式，如 '2024-01-15T14:30:00'
  is_self      INTEGER NOT NULL DEFAULT 0        -- 是否自己发送：1=自己，0=他人
  image_path   TEXT DEFAULT NULL                 -- 图片消息的本地路径（可为空）
  source       TEXT NOT NULL DEFAULT 'chat'      -- 来源：'chat' | 'summary'
  quality      TEXT NOT NULL DEFAULT 'high'      -- 质量：'high' | 'low'
  chat_type    TEXT DEFAULT NULL                 -- 聊天类型（可为空）

Indexes: idx_chat_time(chat_name, detected_at), idx_sender(sender)
"#;

const TRANSLATIONS_SCHEMA: &str = r#"
Table: message_translations
  id              INTEGER PRIMARY KEY AUTOINCREMENT
  content_hash    TEXT NOT NULL                  -- 关联 messages.content_hash
  source_lang     TEXT NOT NULL                  -- 源语言代码，如 'zh', 'ja', 'en'
  target_lang     TEXT NOT NULL                  -- 目标语言代码，如 'en', 'zh'
  translated_text TEXT NOT NULL                  -- 翻译结果
  updated_at      TEXT NOT NULL                  -- 更新时间，ISO8601 格式

Indexes: idx_message_translations_lookup(content_hash, source_lang, target_lang)
"#;

impl Tool for SchemaTool {
    const NAME: &'static str = "get_schema";

    type Error = ToolError;
    type Args = SchemaInput;
    type Output = SchemaOutput;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "返回微信消息数据库的表结构说明，帮助生成准确的 SQL 查询。可指定表名（messages 或 message_translations），不填则返回所有表。".to_string(),
            parameters: serde_json::to_value(schemars::schema_for!(SchemaInput)).unwrap_or_default(),
        }
    }

    async fn call(&self, input: Self::Args) -> Result<Self::Output, Self::Error> {
        let schema = match input.table_name.as_deref() {
            Some("messages") => MESSAGES_SCHEMA.to_string(),
            Some("message_translations") => TRANSLATIONS_SCHEMA.to_string(),
            None | Some(_) => format!("{}{}", MESSAGES_SCHEMA, TRANSLATIONS_SCHEMA),
        };
        Ok(SchemaOutput { schema })
    }
}
