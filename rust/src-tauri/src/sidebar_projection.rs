use crate::events::{EventStore, EventType};
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::{AppHandle, Emitter};

/// Sidebar 读模型运行态：维护当前聊天与投影版本号。
pub struct SidebarRuntime {
    current_chat: std::sync::Mutex<String>,
    refresh_version: AtomicU64,
}

impl SidebarRuntime {
    pub fn new() -> Self {
        Self {
            current_chat: std::sync::Mutex::new(String::new()),
            refresh_version: AtomicU64::new(0),
        }
    }

    pub fn get_current_chat(&self) -> String {
        self.current_chat.lock().unwrap().clone()
    }

    pub fn set_current_chat(&self, chat_name: &str) {
        *self.current_chat.lock().unwrap() = chat_name.to_string();
    }

    pub fn get_refresh_version(&self) -> u64 {
        self.refresh_version.load(Ordering::Relaxed)
    }

    pub fn increment_refresh_version(&self) -> u64 {
        self.refresh_version.fetch_add(1, Ordering::Relaxed) + 1
    }

    pub fn update_chat_and_version(&self, chat_name: &str) -> u64 {
        self.set_current_chat(chat_name);
        self.increment_refresh_version()
    }

    pub fn clear(&self) {
        *self.current_chat.lock().unwrap() = String::new();
        self.refresh_version.store(0, Ordering::Relaxed);
    }
}

pub fn emit_sidebar_invalidated(
    app_handle: &AppHandle,
    events: &EventStore,
    chat_name: &str,
    refresh_version: u64,
) {
    events.publish(
        app_handle,
        EventType::Status,
        "sidebar",
        serde_json::json!({
            "type": "sidebar-refresh",
            "chat_name": chat_name,
            "refresh_version": refresh_version,
        }),
    );
    let _ = app_handle.emit(
        "sidebar-invalidated",
        serde_json::json!({
            "version": refresh_version,
            "chat_name": chat_name,
        }),
    );
}
