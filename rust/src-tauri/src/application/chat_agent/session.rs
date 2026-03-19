//! Chat Agent 会话管理：维护每个对话的多轮历史，按 session_id 索引。
use std::collections::HashMap;
use std::sync::Mutex;

const MAX_HISTORY_TURNS: usize = 40;

/// 简单的消息记录，role 为 "user" 或 "assistant"
#[derive(Clone, Debug)]
pub struct HistoryMessage {
    pub role: String,
    pub content: String,
}

impl HistoryMessage {
    pub fn user(content: String) -> Self {
        Self { role: "user".to_string(), content }
    }

    pub fn assistant(content: String) -> Self {
        Self { role: "assistant".to_string(), content }
    }

    /// 转换为 Rig completion Message
    pub fn to_rig_message(&self) -> rig::completion::Message {
        if self.role == "assistant" {
            rig::completion::Message::Assistant {
                id: None,
                content: rig::OneOrMany::one(rig::completion::message::AssistantContent::Text(
                    rig::completion::message::Text {
                        text: self.content.clone(),
                    },
                )),
            }
        } else {
            rig::completion::Message::User {
                content: rig::OneOrMany::one(rig::completion::message::UserContent::Text(
                    rig::completion::message::Text {
                        text: self.content.clone(),
                    },
                )),
            }
        }
    }
}

pub struct ChatSession {
    pub messages: Vec<HistoryMessage>,
}

impl ChatSession {
    pub fn new() -> Self {
        Self { messages: Vec::new() }
    }

    pub fn push_user(&mut self, content: String) {
        self.messages.push(HistoryMessage::user(content));
        self.trim();
    }

    pub fn push_assistant(&mut self, content: String) {
        self.messages.push(HistoryMessage::assistant(content));
        self.trim();
    }

    fn trim(&mut self) {
        if self.messages.len() > MAX_HISTORY_TURNS {
            let drain = self.messages.len() - MAX_HISTORY_TURNS;
            self.messages.drain(0..drain);
        }
    }

    pub fn to_rig_history(&self) -> Vec<rig::completion::Message> {
        self.messages.iter().map(|m| m.to_rig_message()).collect()
    }
}

pub struct SessionStore {
    sessions: Mutex<HashMap<String, ChatSession>>,
}

impl SessionStore {
    pub fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
        }
    }

    pub fn create(&self) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        self.sessions
            .lock()
            .unwrap()
            .insert(id.clone(), ChatSession::new());
        id
    }

    pub fn get_rig_history(&self, id: &str) -> Vec<rig::completion::Message> {
        self.sessions
            .lock()
            .unwrap()
            .get(id)
            .map(|s| s.to_rig_history())
            .unwrap_or_default()
    }

    pub fn push_user(&self, id: &str, content: String) {
        if let Some(s) = self.sessions.lock().unwrap().get_mut(id) {
            s.push_user(content);
        }
    }

    pub fn push_assistant(&self, id: &str, content: String) {
        if let Some(s) = self.sessions.lock().unwrap().get_mut(id) {
            s.push_assistant(content);
        }
    }

    pub fn clear(&self, id: &str) {
        if let Some(s) = self.sessions.lock().unwrap().get_mut(id) {
            s.messages.clear();
        }
    }
}
