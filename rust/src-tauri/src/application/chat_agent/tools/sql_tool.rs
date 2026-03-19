//! SQL Tool: 让 Agent 执行只读 SQL 查询，并以 JSON 格式返回结果。
use rig::{completion::ToolDefinition, tool::Tool};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::application::chat_agent::tools::{ToolCallEvent, ToolEventBuffer};
use crate::db::MessageDb;

const MAX_ROWS: usize = 200;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SqlInput {
    /// 要执行的 SELECT SQL 语句
    pub sql: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SqlOutput {
    pub rows: Vec<serde_json::Value>,
    pub row_count: usize,
    pub truncated: bool,
}

pub struct SqlTool {
    pub db: Arc<MessageDb>,
    pub event_buffer: Arc<ToolEventBuffer>,
}

fn is_safe_sql(sql: &str) -> bool {
    let upper = sql.trim().to_uppercase();
    let starts_ok = upper.starts_with("SELECT") || upper.starts_with("WITH");
    let no_write = !upper.contains("INSERT")
        && !upper.contains("UPDATE")
        && !upper.contains("DELETE")
        && !upper.contains("DROP")
        && !upper.contains("ALTER")
        && !upper.contains("CREATE")
        && !upper.contains("TRUNCATE")
        && !upper.contains("REPLACE")
        && !upper.contains("ATTACH");
    starts_ok && no_write
}

impl Tool for SqlTool {
    const NAME: &'static str = "execute_sql";

    type Error = crate::application::chat_agent::tools::ToolError;
    type Args = SqlInput;
    type Output = SqlOutput;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "对本地微信消息数据库执行 SELECT 查询，返回 JSON 格式的行数组。只允许 SELECT/WITH 开头的只读查询，最多返回 200 行。".to_string(),
            parameters: serde_json::to_value(schemars::schema_for!(SqlInput)).unwrap_or_default(),
        }
    }

    async fn call(&self, input: Self::Args) -> Result<Self::Output, Self::Error> {
        let sql = input.sql.trim().to_string();

        if !is_safe_sql(&sql) {
            let err = "安全限制：只允许执行 SELECT 或 WITH 开头的只读查询，禁止任何写操作。";
            self.event_buffer.push(ToolCallEvent {
                tool_name: Self::NAME.to_string(),
                input: serde_json::json!({ "sql": sql }),
                output: err.to_string(),
                is_error: true,
            });
            return Err(err.into());
        }

        let rows = self
            .db
            .execute_read_query(&sql, MAX_ROWS)
            .map_err(|e| format!("SQL 执行失败: {e}"));

        match rows {
            Ok(rows) => {
                let truncated = rows.len() >= MAX_ROWS;
                let row_count = rows.len();
                let output = SqlOutput { rows, row_count, truncated };
                self.event_buffer.push(ToolCallEvent {
                    tool_name: Self::NAME.to_string(),
                    input: serde_json::json!({ "sql": sql }),
                    output: serde_json::to_string(&output).unwrap_or_default(),
                    is_error: false,
                });
                Ok(output)
            }
            Err(e) => {
                self.event_buffer.push(ToolCallEvent {
                    tool_name: Self::NAME.to_string(),
                    input: serde_json::json!({ "sql": sql }),
                    output: e.clone(),
                    is_error: true,
                });
                Err(e.into())
            }
        }
    }
}
