use crate::adapter::ax_reader::{self, ChatMessage};
use crate::adapter::MacOSAdapter;
use crate::db::MessageDb;
use crate::events::{EventStore, EventType};
use crate::image_cache::{self, WeChatImageCache};
use crate::translator::DeepLXTranslator;
use anyhow::Result;
use log::debug;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskState {
    pub monitoring: bool,
    pub sidebar: bool,
}

impl TaskState {
    pub fn empty() -> Self {
        Self {
            monitoring: false,
            sidebar: false,
        }
    }
}

struct SidebarConfig {
    translator: Option<Arc<DeepLXTranslator>>,
    target_set: HashSet<String>,
    image_capture: bool,
}

#[derive(Clone)]
pub struct TaskManager {
    adapter: Arc<MacOSAdapter>,
    events: Arc<EventStore>,
    db: Arc<MessageDb>,
    image_cache: Arc<std::sync::Mutex<WeChatImageCache>>,
    monitor_token: Arc<Mutex<Option<CancellationToken>>>,
    monitoring_active: Arc<AtomicBool>,
    sidebar_enabled: Arc<AtomicBool>,
    sidebar_config: Arc<Mutex<SidebarConfig>>,
    app_handle: Arc<Mutex<Option<AppHandle>>>,
}

impl TaskManager {
    pub fn new(
        adapter: Arc<MacOSAdapter>,
        events: Arc<EventStore>,
        db: Arc<MessageDb>,
        image_cache: Arc<std::sync::Mutex<WeChatImageCache>>,
        ) -> Self {
        Self {
            adapter,
            events,
            db,
            image_cache,
            monitor_token: Arc::new(Mutex::new(None)),
            monitoring_active: Arc::new(AtomicBool::new(false)),
            sidebar_enabled: Arc::new(AtomicBool::new(false)),
            sidebar_config: Arc::new(Mutex::new(SidebarConfig {
                translator: None,
                target_set: HashSet::new(),
                image_capture: false,
            })),
            app_handle: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn set_app_handle(&self, handle: AppHandle) {
        *self.app_handle.lock().await = Some(handle);
    }

    async fn get_app_handle(&self) -> Result<AppHandle> {
        self.app_handle
            .lock()
            .await
            .clone()
            .ok_or_else(|| anyhow::anyhow!("AppHandle not set"))
    }

    pub fn get_task_state(&self) -> TaskState {
        TaskState {
            monitoring: self.monitoring_active.load(Ordering::Relaxed),
            sidebar: self.sidebar_enabled.load(Ordering::Relaxed),
        }
    }

    pub fn service_status(&self) -> serde_json::Value {
        let state = self.get_task_state();
        serde_json::json!({
            "adapter": {
                "platform": self.adapter.is_supported().then_some("macos").unwrap_or("unsupported"),
                "supported": self.adapter.is_supported(),
                "reason": self.adapter.support_reason(),
            },
            "tasks": state,
        })
    }

    pub async fn start_monitoring(&self, interval_seconds: f64) -> Result<()> {
        {
            let existing = self.monitor_token.lock().await;
            if existing.is_some() {
                anyhow::bail!("监听已在运行中");
            }
        }

        let token = CancellationToken::new();
        *self.monitor_token.lock().await = Some(token.clone());
        self.monitoring_active.store(true, Ordering::Relaxed);

        let app = self.get_app_handle().await?;
        let state = self.get_task_state();
        self.events.publish(
            &app,
            EventType::TaskState,
            "task_manager",
            serde_json::json!({
                "task": "monitoring",
                "running": true,
                "state": &state,
            }),
        );
        update_tray_menu(&app, &state);

        let adapter = self.adapter.clone();
        let events = self.events.clone();
        let db = self.db.clone();
        let image_cache = self.image_cache.clone();
        let monitor_token_ref = self.monitor_token.clone();
        let monitoring_active = self.monitoring_active.clone();
        let sidebar_enabled = self.sidebar_enabled.clone();
        let sidebar_config = self.sidebar_config.clone();
        let app_handle = app.clone();
        let poll_interval = interval_seconds.max(0.4);

        tokio::spawn(async move {
            let mut session_state: HashMap<String, SessionListenState> = HashMap::new();
            let mut chat_baselines: HashMap<String, Vec<ChatMessage>> = HashMap::new();
            let mut chat_kinds: HashMap<String, ChatKind> = HashMap::new();
            let mut preview_sender_hints: HashMap<String, PreviewSenderHint> = HashMap::new();
            let mut active_chat_name = String::new();
            let mut pending_chat_name: Option<String> = None;
            let mut pending_count: u32 = 0;
            const DEBOUNCE_THRESHOLD: u32 = 2;
            const SESSION_CORRECTION_WINDOW_SECONDS: i64 = 15;

            loop {
                if token.is_cancelled() {
                    break;
                }

                if adapter.is_ui_paused() {
                    tokio::select! {
                        _ = token.cancelled() => break,
                        _ = tokio::time::sleep(std::time::Duration::from_millis(200)) => {}
                    }
                    continue;
                }

                let poll_result = tokio::task::spawn_blocking({
                    let adapter = adapter.clone();
                    move || -> Result<(String, Vec<ax_reader::SessionItemSnapshot>, Vec<ChatMessage>, Option<u32>)> {
                        if adapter.is_ui_paused() || adapter.has_popup_or_menu() {
                            anyhow::bail!("ui paused or popup detected, skip");
                        }
                        let chat_name = adapter.read_active_chat_name()?;
                        let snapshots = adapter.read_session_snapshots()?;
                        let messages = adapter.read_chat_messages_rich().unwrap_or_default();
                        let member_count = adapter.read_active_chat_member_count().unwrap_or(None);
                        Ok((chat_name, snapshots, messages, member_count))
                    }
                })
                .await;

                if let Ok(Ok((chat_name, snapshots, mut messages, member_count))) = poll_result {
                    let now_instant = Instant::now();
                    cleanup_preview_sender_hints(&mut preview_sender_hints, now_instant);
                    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
                    let prev_unread_counts: HashMap<String, u32> = session_state
                        .iter()
                        .map(|(name, state)| (name.clone(), state.last_unread))
                        .collect();
                    let snapshot_map: HashMap<String, ax_reader::SessionItemSnapshot> = snapshots
                        .iter()
                        .cloned()
                        .map(|item| (item.chat_name.clone(), item))
                        .collect();

                    for snapshot in &snapshots {
                        let state = session_state.entry(snapshot.chat_name.clone()).or_default();
                        if should_emit_session_snapshot(snapshot, state) {
                            let mut chat_kind = chat_kinds
                                .get(&snapshot.chat_name)
                                .copied()
                                .unwrap_or(ChatKind::Unknown);

                            // Use member_count for the active chat (highest priority signal)
                            if snapshot.chat_name == chat_name && member_count.is_some() {
                                chat_kind = ChatKind::Group;
                            } else if snapshot.is_group {
                                chat_kind = ChatKind::Group;
                            } else if chat_kind == ChatKind::Unknown {
                                chat_kind = ChatKind::Private;
                            }
                            chat_kinds.insert(snapshot.chat_name.clone(), chat_kind);
                            remember_preview_sender_hint(
                                &mut preview_sender_hints,
                                snapshot,
                                now_instant,
                            );

                            let mut sender = snapshot.sender_hint.clone().unwrap_or_default();
                            let prev_unread = prev_unread_counts
                                .get(&snapshot.chat_name)
                                .copied()
                                .unwrap_or(0);
                            let unread_increased = snapshot.unread_count > prev_unread;
                            let is_self = if snapshot.has_sender_prefix {
                                false // group chat with sender prefix → other person
                            } else if unread_increased {
                                false // unread count grew → must be from other person
                            } else if matches!(chat_kind, ChatKind::Group) {
                                sender.clear();
                                true // group, no prefix, unread didn't grow → self
                            } else {
                                // private chat, no prefix, unread didn't grow → likely self
                                true
                            };
                            if !is_self && sender.is_empty() {
                                sender = snapshot.chat_name.clone();
                            }

                            events.publish(
                                &app_handle,
                                EventType::Message,
                                "monitor",
                                serde_json::json!({
                                    "chat_name": snapshot.chat_name,
                                    "chat_type": chat_kind.as_str(),
                                    "self_source": if snapshot.has_sender_prefix { "prefix" } else if unread_increased { "unread" } else if matches!(chat_kind, ChatKind::Group) { "group_no_prefix" } else { "unread_stable" },
                                    "source": "session_preview",
                                    "quality": "low",
                                    "sender": sender,
                                    "text": snapshot.preview_body,
                                    "is_self": is_self,
                                }),
                            );

                            // Skip DB insert for the active chat — the higher-quality
                            // chat_diff path handles it with better sender/content data.
                            if snapshot.chat_name != chat_name {
                                let _ = db.insert_message_with_meta(
                                    &snapshot.chat_name,
                                    &sender,
                                    &snapshot.preview_body,
                                    "",
                                    is_self,
                                    &now,
                                    None,
                                    "session_preview",
                                    "low",
                                );
                            }
                        }

                        if !snapshot.preview_body.is_empty() {
                            state.last_preview_body = snapshot.preview_body.clone();
                        }
                        state.last_unread = snapshot.unread_count;
                    }

                    if chat_name != active_chat_name {
                        match &pending_chat_name {
                            Some(pending) if *pending == chat_name => {
                                pending_count += 1;
                                if pending_count >= DEBOUNCE_THRESHOLD {
                                    active_chat_name = chat_name.clone();
                                    pending_chat_name = None;
                                    pending_count = 0;
                                    events.publish(
                                        &app_handle,
                                        EventType::Status,
                                        "monitor",
                                        serde_json::json!({
                                            "type": "chat_switched",
                                            "chat_name": chat_name,
                                        }),
                                    );
                                }
                            }
                            _ => {
                                pending_chat_name = Some(chat_name.clone());
                                pending_count = 1;
                            }
                        }
                    } else {
                        pending_chat_name = None;
                        pending_count = 0;
                    }

                    if !messages.is_empty() {
                        let mut chat_kind = chat_kinds
                            .get(&chat_name)
                            .copied()
                            .unwrap_or(ChatKind::Unknown);

                        // Highest priority: member_count from current_chat_count_label
                        // Group chats have this element, private chats don't
                        if member_count.is_some() {
                            chat_kind = ChatKind::Group;
                        }

                        if let Some(baseline) = chat_baselines.get(&chat_name) {
                            inherit_sender_from_reference(&mut messages, baseline);
                        }

                        if let Some(snapshot) = snapshot_map.get(&chat_name) {
                            let prev_unread = prev_unread_counts.get(&chat_name).copied().unwrap_or(0);
                            let unread_increased = snapshot.unread_count > prev_unread;
                            apply_session_preview_sender_hint(
                                &mut messages,
                                &snapshot.raw_preview,
                                &mut chat_kind,
                                unread_increased,
                            );
                        } else if chat_kind == ChatKind::Unknown {
                            chat_kind = ChatKind::Private;
                        }
                        apply_sender_defaults(&mut messages, &chat_name, chat_kind);
                        chat_kinds.insert(chat_name.clone(), chat_kind);
                        let mut chat_type_label = chat_kind.as_str().to_string();

                        if let Some(last) = messages.last() {
                            debug!(
                                "infer_state chat='{}' kind={} total={} latest='{}' is_self={} sender='{}' source={}",
                                chat_name,
                                chat_kind.as_str(),
                                messages.len(),
                                trim_for_log(&last.content, 24),
                                last.is_self,
                                last.sender,
                                self_source_label(last),
                            );
                        }

                        let baseline = chat_baselines.entry(chat_name.clone()).or_default();
                        if baseline.is_empty() {
                            *baseline = messages;
                        } else {
                            let diff_result = diff_messages(baseline, &messages);
                            if diff_result.anchor_failed && !diff_result.new_messages.is_empty() {
                                events.publish(
                                    &app_handle,
                                    EventType::Log,
                                    "monitor",
                                    serde_json::json!({
                                        "message": format!(
                                            "锚点匹配失败，bag diff 兜底检测到 {} 条新消息",
                                            diff_result.new_messages.len()
                                        ),
                                    }),
                                );
                            }
                            let mut new_msgs = diff_result.new_messages;
                            if apply_cached_preview_sender_hint(
                                &chat_name,
                                &mut new_msgs,
                                &mut chat_kind,
                                &mut preview_sender_hints,
                                now_instant,
                            ) {
                                chat_type_label = chat_kind.as_str().to_string();
                                chat_kinds.insert(chat_name.clone(), chat_kind);
                            }
                            for msg in &new_msgs {
                                let mut content_en = String::new();
                                let mut found_image_path: Option<String> = None;
                                events.publish(
                                    &app_handle,
                                    EventType::Log,
                                    "monitor_infer",
                                    serde_json::json!({
                                        "type": "message_inferred",
                                        "chat_name": chat_name,
                                        "chat_type": chat_type_label.clone(),
                                        "self_source": self_source_label(msg),
                                        "is_self": msg.is_self,
                                        "sender": msg.sender,
                                        "text_preview": trim_for_log(&msg.content, 24),
                                    }),
                                );

                                events.publish(
                                    &app_handle,
                                    EventType::Message,
                                    "monitor",
                                    serde_json::json!({
                                        "chat_name": chat_name,
                                        "chat_type": chat_type_label.clone(),
                                        "self_source": self_source_label(msg),
                                        "source": "chat",
                                        "quality": "high",
                                        "sender": msg.sender,
                                        "text": msg.content,
                                        "is_self": msg.is_self,
                                    }),
                                );

                                if sidebar_enabled.load(Ordering::Relaxed) {
                                    let (translator, target_set, image_capture) = {
                                        let config = sidebar_config.lock().await;
                                        (
                                            config.translator.clone(),
                                            config.target_set.clone(),
                                            config.image_capture,
                                        )
                                    };

                                    if image_capture && image_cache::is_image_placeholder(&msg.content) {
                                        let now_ts = std::time::SystemTime::now()
                                            .duration_since(std::time::UNIX_EPOCH)
                                            .unwrap()
                                            .as_secs()
                                            as i64;
                                        let cn = chat_name.clone();
                                        if let Ok(mut cache) = image_cache.lock() {
                                            if let Some(path) =
                                                cache.find_image_for_message(&cn, now_ts)
                                            {
                                                found_image_path =
                                                    Some(path.to_string_lossy().to_string());
                                            }
                                        }
                                    }

                                    if target_set.is_empty() || target_set.contains(&chat_name) {
                                        let mut text_en = msg.content.clone();
                                        let mut translate_error = String::new();

                                        if let Some(ref translator) = translator {
                                            let t = translator.clone();
                                            let text = msg.content.clone();
                                            match tokio::runtime::Handle::try_current() {
                                                Ok(handle) => {
                                                    let result = std::thread::spawn(move || {
                                                        handle.block_on(t.translate(&text))
                                                    })
                                                    .join();
                                                    match result {
                                                        Ok(Ok(translated)) => text_en = translated,
                                                        Ok(Err(e)) => {
                                                            translate_error = e.to_string()
                                                        }
                                                        Err(_) => {
                                                            translate_error =
                                                                "translate thread panicked"
                                                                    .to_string()
                                                        }
                                                    }
                                                }
                                                Err(_) => {
                                                    translate_error =
                                                        "no tokio runtime available".to_string();
                                                }
                                            }
                                        }

                                        content_en = text_en.clone();

                                        let mut sidebar_payload = serde_json::json!({
                                            "chat_name": chat_name,
                                            "chat_type": chat_type_label.clone(),
                                            "self_source": self_source_label(msg),
                                            "source": "chat",
                                            "quality": "high",
                                            "sender": msg.sender,
                                            "text_cn": msg.content,
                                            "text_en": text_en,
                                            "translate_error": translate_error,
                                            "is_self": msg.is_self,
                                        });
                                        if let Some(ref ip) = found_image_path {
                                            sidebar_payload["image_path"] =
                                                serde_json::Value::String(ip.clone());
                                        }

                                        events.publish(
                                            &app_handle,
                                            EventType::Message,
                                            "sidebar",
                                            sidebar_payload,
                                        );
                                    }
                                }

                                let corrected = db
                                    .try_correct_preview_row(
                                        &chat_name,
                                        &msg.content,
                                        &msg.sender,
                                        &content_en,
                                        msg.is_self,
                                        found_image_path.as_deref(),
                                        &now,
                                        SESSION_CORRECTION_WINDOW_SECONDS,
                                    )
                                    .unwrap_or(false);

                                if !corrected {
                                    let _ = db.insert_message_with_meta(
                                        &chat_name,
                                        &msg.sender,
                                        &msg.content,
                                        &content_en,
                                        msg.is_self,
                                        &now,
                                        found_image_path.as_deref(),
                                        "chat",
                                        "high",
                                    );
                                } else {
                                    events.publish(
                                        &app_handle,
                                        EventType::Log,
                                        "monitor_infer",
                                        serde_json::json!({
                                            "type": "preview_corrected",
                                            "chat_name": chat_name,
                                            "text_preview": trim_for_log(&msg.content, 24),
                                        }),
                                    );
                                }
                            }

                            *baseline = messages;
                        }
                    }
                }

                tokio::select! {
                    _ = token.cancelled() => break,
                    _ = tokio::time::sleep(std::time::Duration::from_secs_f64(poll_interval)) => {}
                }
            }

            *monitor_token_ref.lock().await = None;
            monitoring_active.store(false, Ordering::Relaxed);
            sidebar_enabled.store(false, Ordering::Relaxed);

            let stopped_state = TaskState::empty();
            events.publish(
                &app_handle,
                EventType::TaskState,
                "task_manager",
                serde_json::json!({
                    "task": "monitoring",
                    "running": false,
                    "state": &stopped_state,
                }),
            );
            update_tray_menu(&app_handle, &stopped_state);
        });

        Ok(())
    }

    pub async fn stop_monitoring(&self) -> Result<()> {
        let token = self.monitor_token.lock().await.clone();
        if let Some(token) = token {
            token.cancel();
        }
        Ok(())
    }

    pub async fn enable_sidebar(
        &self,
        targets: Vec<String>,
        translate_enabled: bool,
        deeplx_url: String,
        source_lang: String,
        target_lang: String,
        timeout_seconds: f64,
        image_capture: bool,
    ) -> Result<()> {
        let translator = if translate_enabled && !deeplx_url.is_empty() {
            Some(Arc::new(DeepLXTranslator::new(
                &deeplx_url,
                &source_lang,
                &target_lang,
                timeout_seconds,
            )))
        } else {
            None
        };

        let target_set: HashSet<String> = targets.into_iter().filter(|t| !t.is_empty()).collect();

        {
            let mut config = self.sidebar_config.lock().await;
            config.translator = translator;
            config.target_set = target_set;
            config.image_capture = image_capture;
        }

        self.sidebar_enabled.store(true, Ordering::Relaxed);

        let app = self.get_app_handle().await?;
        let state = self.get_task_state();
        self.events.publish(
            &app,
            EventType::TaskState,
            "task_manager",
            serde_json::json!({
                "task": "sidebar",
                "running": true,
                "state": &state,
            }),
        );
        update_tray_menu(&app, &state);

        Ok(())
    }

    pub async fn disable_sidebar(&self) -> Result<()> {
        self.sidebar_enabled.store(false, Ordering::Relaxed);

        {
            let mut config = self.sidebar_config.lock().await;
            config.translator = None;
            config.target_set.clear();
        }

        let app = self.get_app_handle().await?;
        let state = self.get_task_state();
        self.events.publish(
            &app,
            EventType::TaskState,
            "task_manager",
            serde_json::json!({
                "task": "sidebar",
                "running": false,
                "state": &state,
            }),
        );
        update_tray_menu(&app, &state);

        Ok(())
    }

    pub async fn stop_all(&self) {
        let _ = self.stop_monitoring().await;
    }
}

struct DiffResult {
    new_messages: Vec<ChatMessage>,
    anchor_failed: bool,
}

#[derive(Debug, Clone)]
struct PreviewSenderHint {
    sender: String,
    preview_body: String,
    preview_body_key: String,
    unread_count: u32,
    updated_at: Instant,
    consumed: bool,
}

#[derive(Debug, Clone, Default)]
struct SessionListenState {
    last_preview_body: String,
    last_unread: u32,
}

const PREVIEW_SENDER_HINT_TTL: Duration = Duration::from_secs(12);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChatKind {
    Group,
    Private,
    Unknown,
}

impl ChatKind {
    fn as_str(self) -> &'static str {
        match self {
            ChatKind::Group => "group",
            ChatKind::Private => "private",
            ChatKind::Unknown => "unknown",
        }
    }
}

fn self_source_label(msg: &ChatMessage) -> &'static str {
    if !msg.sender.is_empty() {
        "prefix"
    } else {
        "fallback"
    }
}

fn should_emit_session_snapshot(
    snapshot: &ax_reader::SessionItemSnapshot,
    state: &SessionListenState,
) -> bool {
    if snapshot.preview_body.is_empty() {
        return false;
    }
    // Only emit when preview text actually changes.
    // Previously also checked `unread_count > last_unread`, but for the active chat
    // the unread count oscillates (0→1→0→1…) as messages arrive and are "read",
    // causing the same preview to be emitted and inserted into DB repeatedly.
    snapshot.preview_body != state.last_preview_body
}

fn apply_sender_defaults(messages: &mut [ChatMessage], chat_name: &str, chat_kind: ChatKind) {
    if matches!(chat_kind, ChatKind::Group) {
        return;
    }
    for msg in messages {
        if !msg.is_self && msg.sender.is_empty() {
            msg.sender = chat_name.to_string();
        }
    }
}

fn trim_for_log(text: &str, max_chars: usize) -> String {
    let normalized = ax_reader::normalize_for_match(text);
    let mut out = normalized.chars().take(max_chars).collect::<String>();
    if normalized.chars().count() > max_chars {
        out.push('…');
    }
    out
}

fn preview_body_matches_message(
    preview_body: &str,
    message_content: &str,
    preview_body_key: Option<&str>,
) -> bool {
    let normalized_preview = ax_reader::normalize_for_match(preview_body);
    let normalized_message = ax_reader::normalize_for_match(message_content);
    if normalized_preview.is_empty() || normalized_message.is_empty() {
        return false;
    }

    if let Some(key) = preview_body_key {
        if !key.is_empty() && key == ax_reader::prefix8_key(&normalized_message) {
            return true;
        }
    } else if ax_reader::is_same_message_prefix8(&normalized_preview, &normalized_message) {
        return true;
    }

    let preview_len = normalized_preview.chars().count();
    let message_len = normalized_message.chars().count();
    (preview_len < 8 && normalized_message.starts_with(&normalized_preview))
        || (message_len < 8 && normalized_preview.starts_with(&normalized_message))
}

fn apply_session_preview_sender_hint(
    messages: &mut [ChatMessage],
    preview_text: &str,
    chat_kind: &mut ChatKind,
    unread_increased: bool,
) {
    if messages.is_empty() {
        return;
    }
    let (sender, preview_body) = ax_reader::parse_session_preview_line(preview_text);
    let Some(preview_body) = preview_body else {
        return;
    };
    if let Some(last) = messages.last_mut() {
        let text_matched = preview_body_matches_message(&preview_body, &last.content, None);
        let image_equivalent =
            is_image_placeholder_like(&preview_body) && is_image_placeholder_like(&last.content);
        let matched = text_matched || image_equivalent;

        match sender {
            Some(sender) if !sender.is_empty() => {
                *chat_kind = ChatKind::Group;
                if matched {
                    last.sender = sender;
                    last.is_self = false;
                    debug!(
                        "preview_hint matched(group) body='{}' latest='{}' -> sender='{}'",
                        trim_for_log(&preview_body, 24),
                        trim_for_log(&last.content, 24),
                        last.sender,
                    );
                }
            }
            _ => {
                if *chat_kind != ChatKind::Group {
                    *chat_kind = ChatKind::Private;
                }
                if matched {
                    // For private chats (no sender prefix), use unread signal:
                    // unread grew → other person; unread stable → likely self
                    let is_self_hint = if matches!(*chat_kind, ChatKind::Group) {
                        true // group without prefix → self (unchanged)
                    } else if unread_increased {
                        false // private, unread grew → other person
                    } else {
                        true // private, unread stable → likely self
                    };
                    last.sender.clear();
                    last.is_self = is_self_hint;
                    debug!(
                        "preview_hint no_sender kind={} matched={} unread_inc={} body='{}' latest='{}' -> is_self={}",
                        chat_kind.as_str(),
                        matched,
                        unread_increased,
                        trim_for_log(&preview_body, 24),
                        trim_for_log(&last.content, 24),
                        last.is_self,
                    );
                }
            }
        }
    }
}

fn cleanup_preview_sender_hints(cache: &mut HashMap<String, PreviewSenderHint>, now: Instant) {
    cache.retain(|_, hint| {
        !hint.consumed && now.duration_since(hint.updated_at) <= PREVIEW_SENDER_HINT_TTL
    });
}

fn remember_preview_sender_hint(
    cache: &mut HashMap<String, PreviewSenderHint>,
    snapshot: &ax_reader::SessionItemSnapshot,
    now: Instant,
) {
    if !snapshot.has_sender_prefix {
        return;
    }

    let Some(sender) = snapshot
        .sender_hint
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    else {
        return;
    };

    let preview_body = snapshot.preview_body.trim();
    if preview_body.is_empty() {
        return;
    }

    cache.insert(
        snapshot.chat_name.clone(),
        PreviewSenderHint {
            sender: sender.to_string(),
            preview_body: preview_body.to_string(),
            preview_body_key: ax_reader::prefix8_key(preview_body),
            unread_count: snapshot.unread_count,
            updated_at: now,
            consumed: false,
        },
    );
}

fn apply_cached_preview_sender_hint(
    chat_name: &str,
    new_messages: &mut [ChatMessage],
    chat_kind: &mut ChatKind,
    cache: &mut HashMap<String, PreviewSenderHint>,
    now: Instant,
) -> bool {
    let Some(hint) = cache.get_mut(chat_name) else {
        return false;
    };

    if hint.consumed || now.duration_since(hint.updated_at) > PREVIEW_SENDER_HINT_TTL {
        return false;
    }

    for msg in new_messages.iter_mut().rev() {
        let text_matched = preview_body_matches_message(
            &hint.preview_body,
            &msg.content,
            Some(&hint.preview_body_key),
        );
        let image_equivalent = is_image_placeholder_like(&hint.preview_body)
            && is_image_placeholder_like(&msg.content);
        if !text_matched && !image_equivalent {
            continue;
        }

        *chat_kind = ChatKind::Group;
        msg.sender = hint.sender.clone();
        msg.is_self = false;
        hint.consumed = true;
        debug!(
            "preview_hint cache matched chat='{}' unread={} body='{}' latest='{}' -> sender='{}'",
            chat_name,
            hint.unread_count,
            trim_for_log(&hint.preview_body, 24),
            trim_for_log(&msg.content, 24),
            msg.sender,
        );
        return true;
    }

    false
}

fn is_image_placeholder_like(text: &str) -> bool {
    let normalized = ax_reader::normalize_for_match(text);
    if normalized.is_empty() {
        return false;
    }
    let stripped = normalized
        .trim_matches(|c| matches!(c, '[' | ']' | '【' | '】' | '(' | ')'))
        .to_lowercase();
    matches!(
        stripped.as_str(),
        "图片" | "image" | "images" | "photo" | "photos" | "照片"
    )
}

/// Reuse known sender labels from a previous visible slice by matching content.
/// We consume from the tail first so the newest repeated texts are aligned first.
fn inherit_sender_from_reference(current: &mut [ChatMessage], reference: &[ChatMessage]) {
    if current.is_empty() || reference.is_empty() {
        return;
    }

    let mut sender_bag: HashMap<String, Vec<String>> = HashMap::new();
    for msg in reference.iter().rev() {
        if !msg.sender.is_empty() {
            sender_bag
                .entry(msg.content.clone())
                .or_default()
                .push(msg.sender.clone());
        }
    }

    for msg in current.iter_mut().rev() {
        if !msg.sender.is_empty() {
            continue;
        }
        if let Some(candidates) = sender_bag.get_mut(&msg.content) {
            if let Some(sender) = candidates.pop() {
                msg.sender = sender;
            }
        }
    }
}

/// Find new messages by comparing old and new message lists.
/// Strategy: tail-append → progressive anchor (3→2→1) → bag diff fallback.
fn diff_messages(old: &[ChatMessage], new: &[ChatMessage]) -> DiffResult {
    if old.is_empty() || new.is_empty() {
        return DiffResult {
            new_messages: vec![],
            anchor_failed: false,
        };
    }

    if new.len() == old.len() && contents_match(old, new) {
        return DiffResult {
            new_messages: vec![],
            anchor_failed: false,
        };
    }

    if new.len() > old.len() {
        let old_len = old.len();
        if contents_match(&new[..old_len], old) {
            return DiffResult {
                new_messages: new[old_len..].to_vec(),
                anchor_failed: false,
            };
        }
    }

    if let Some(result) = anchor_diff_progressive(old, new) {
        return DiffResult {
            new_messages: result,
            anchor_failed: false,
        };
    }

    DiffResult {
        new_messages: bag_diff(old, new),
        anchor_failed: true,
    }
}

fn msg_identity_content(m: &ChatMessage) -> &str {
    &m.content
}

fn contents_match(a: &[ChatMessage], b: &[ChatMessage]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter()
        .zip(b.iter())
        .all(|(x, y)| msg_identity_content(x) == msg_identity_content(y))
}

/// Try anchor matching with progressively smaller anchor sizes (3→2→1).
/// Returns Some(new_messages) on success, None if no anchor matched.
fn anchor_diff_progressive(old: &[ChatMessage], new: &[ChatMessage]) -> Option<Vec<ChatMessage>> {
    if old.is_empty() || new.is_empty() {
        return Some(vec![]);
    }

    let max_anchor = 3.min(old.len());
    for anchor_size in (1..=max_anchor).rev() {
        let anchor: Vec<&str> = old[old.len() - anchor_size..]
            .iter()
            .map(msg_identity_content)
            .collect();

        for i in 0..new.len() {
            if i + anchor_size > new.len() {
                break;
            }
            let window: Vec<&str> = new[i..i + anchor_size]
                .iter()
                .map(msg_identity_content)
                .collect();
            if window == anchor {
                let after = i + anchor_size;
                if after < new.len() {
                    return Some(new[after..].to_vec());
                }
                return Some(vec![]);
            }
        }
    }

    None
}

/// Bag (multiset) diff: find messages in `new` not present in `old`.
/// Handles cases where the view scrolled significantly and anchor is lost.
fn bag_diff(old: &[ChatMessage], new: &[ChatMessage]) -> Vec<ChatMessage> {
    let mut bag: HashMap<&str, usize> = HashMap::new();
    for m in old {
        *bag.entry(msg_identity_content(m)).or_default() += 1;
    }
    let mut result = Vec::new();
    for m in new {
        let key = msg_identity_content(m);
        match bag.get_mut(&key) {
            Some(count) if *count > 0 => {
                *count -= 1;
            }
            _ => {
                result.push(m.clone());
            }
        }
    }
    result
}


fn update_tray_menu(app: &AppHandle, state: &TaskState) {
    if let Some(tray) = app.try_state::<crate::TrayMenuState>() {
        let _ = tray.sidebar_status.set_text(if state.sidebar {
            "● 浮窗运行中"
        } else {
            "○ 浮窗未运行"
        });
        let _ = tray.sidebar_toggle.set_text(if state.sidebar {
            "关闭浮窗"
        } else {
            "开启实时浮窗"
        });

        let _ = tray.listen_status.set_text(if state.monitoring {
            "● 监听运行中"
        } else {
            "○ 监听未运行"
        });
        let _ = tray.listen_toggle.set_text(if state.monitoring {
            "暂停监听"
        } else {
            "开启监听"
        });
    }
}
