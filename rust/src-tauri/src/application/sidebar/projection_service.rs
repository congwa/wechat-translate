//! Sidebar 投影服务：负责维护当前聊天与刷新版本号，并把 sidebar 读模型失效事件统一广播给前端，
//! 让 sidebar 当前聊天与 refresh version 这份真相源稳定留在 application/sidebar 子域。
use crate::events::{EventStore, EventType};
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::{AppHandle, Emitter};

/// SidebarRuntime 是 sidebar 读模型的后端真相源。
/// 它只维护“当前聊天”和“刷新版本号”，不直接存消息正文。
pub struct SidebarRuntime {
    current_chat: std::sync::Mutex<String>,
    refresh_version: AtomicU64,
}

impl SidebarRuntime {
    /// 创建一份空的 sidebar 投影状态，供应用启动时初始化为“尚未绑定聊天”。
    pub fn new() -> Self {
        Self {
            current_chat: std::sync::Mutex::new(String::new()),
            refresh_version: AtomicU64::new(0),
        }
    }

    /// 读取当前 sidebar 正在展示的聊天名称，供 snapshot query 和恢复流程判断当前指向。
    pub fn get_current_chat(&self) -> String {
        self.current_chat.lock().unwrap().clone()
    }

    /// 更新当前 sidebar 聊天指向，通常在监听主循环确认活跃聊天后调用。
    pub fn set_current_chat(&self, chat_name: &str) {
        *self.current_chat.lock().unwrap() = chat_name.to_string();
    }

    /// 返回当前 refresh version，供前端抵抗乱序 snapshot 和重复 invalidation。
    pub fn get_refresh_version(&self) -> u64 {
        self.refresh_version.load(Ordering::Relaxed)
    }

    /// 推进 refresh version，表示 sidebar 读模型内容已经发生变化。
    pub fn increment_refresh_version(&self) -> u64 {
        self.refresh_version.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// 同时更新当前聊天和 refresh version，适用于聊天切换后的单次原子投影更新。
    pub fn update_chat_and_version(&self, chat_name: &str) -> u64 {
        self.set_current_chat(chat_name);
        self.increment_refresh_version()
    }

    /// 清空 sidebar 投影状态，通常在 sidebar 关闭或监听恢复前调用，避免残留旧聊天指向。
    pub fn clear(&self) {
        *self.current_chat.lock().unwrap() = String::new();
        self.refresh_version.store(0, Ordering::Relaxed);
    }
}

/// 广播 sidebar 读模型已失效，要求前端重新查询 sidebar snapshot，而不是做局部消息 patch。
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
