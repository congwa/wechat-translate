pub(crate) mod context_tool;
pub(crate) mod schema_tool;
pub(crate) mod sql_tool;

use serde::{Deserialize, Serialize};
use std::sync::Mutex;

/// Tool Error 类型，满足 rig Tool::Error 约束 (std::error::Error + Send + Sync)
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct ToolError(pub String);

impl From<String> for ToolError {
    fn from(s: String) -> Self {
        ToolError(s)
    }
}

impl From<&str> for ToolError {
    fn from(s: &str) -> Self {
        ToolError(s.to_string())
    }
}

/// 单次工具调用的记录，用于前端展示 Agent 推理链
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallEvent {
    pub tool_name: String,
    pub input: serde_json::Value,
    pub output: String,
    pub is_error: bool,
}

/// 线程安全的工具调用事件缓冲区，由各 Tool 共享写入，Service 在 chat 完成后统一读出
pub struct ToolEventBuffer {
    events: Mutex<Vec<ToolCallEvent>>,
}

impl ToolEventBuffer {
    pub fn new() -> Self {
        Self {
            events: Mutex::new(Vec::new()),
        }
    }

    pub fn push(&self, event: ToolCallEvent) {
        if let Ok(mut v) = self.events.lock() {
            v.push(event);
        }
    }

    pub fn take(&self) -> Vec<ToolCallEvent> {
        self.events
            .lock()
            .map(|mut v| std::mem::take(&mut *v))
            .unwrap_or_default()
    }
}
