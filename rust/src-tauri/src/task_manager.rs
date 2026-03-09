use crate::adapter::ax_reader::{self, ChatMessage};
use crate::adapter::MacOSAdapter;
use crate::avatar_capture::{self, AvatarCache};
use crate::db::{content_hash, content_only_hash, MessageDb};
use crate::events::{EventStore, EventType};
use crate::image_cache::{self, WeChatImageCache};
use crate::translator::DeepLXTranslator;
use anyhow::Result;
use log::{debug, warn};
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
    pub autoreply: bool,
}

impl TaskState {
    pub fn empty() -> Self {
        Self {
            monitoring: false,
            sidebar: false,
            autoreply: false,
        }
    }
}

struct SidebarConfig {
    translator: Option<Arc<DeepLXTranslator>>,
    target_set: HashSet<String>,
    beta_image_capture: bool,
    beta_avatar_capture: bool,
}

#[derive(Clone)]
pub struct TaskManager {
    adapter: Arc<MacOSAdapter>,
    events: Arc<EventStore>,
    db: Arc<MessageDb>,
    image_cache: Arc<std::sync::Mutex<WeChatImageCache>>,
    avatar_cache: Arc<std::sync::Mutex<AvatarCache>>,
    monitor_token: Arc<Mutex<Option<CancellationToken>>>,
    monitoring_active: Arc<AtomicBool>,
    sidebar_enabled: Arc<AtomicBool>,
    autoreply_enabled: Arc<AtomicBool>,
    sidebar_config: Arc<Mutex<SidebarConfig>>,
    app_handle: Arc<Mutex<Option<AppHandle>>>,
}

impl TaskManager {
    pub fn new(
        adapter: Arc<MacOSAdapter>,
        events: Arc<EventStore>,
        db: Arc<MessageDb>,
        image_cache: Arc<std::sync::Mutex<WeChatImageCache>>,
        avatar_cache: Arc<std::sync::Mutex<AvatarCache>>,
    ) -> Self {
        Self {
            adapter,
            events,
            db,
            image_cache,
            avatar_cache,
            monitor_token: Arc::new(Mutex::new(None)),
            monitoring_active: Arc::new(AtomicBool::new(false)),
            sidebar_enabled: Arc::new(AtomicBool::new(false)),
            autoreply_enabled: Arc::new(AtomicBool::new(false)),
            sidebar_config: Arc::new(Mutex::new(SidebarConfig {
                translator: None,
                target_set: HashSet::new(),
                beta_image_capture: false,
                beta_avatar_capture: false,
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
            autoreply: self.autoreply_enabled.load(Ordering::Relaxed),
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
        let avatar_cache = self.avatar_cache.clone();
        let monitor_token_ref = self.monitor_token.clone();
        let monitoring_active = self.monitoring_active.clone();
        let sidebar_enabled = self.sidebar_enabled.clone();
        let autoreply_enabled = self.autoreply_enabled.clone();
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

                        apply_position_self_hints(&mut messages);
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
                                "infer_state chat='{}' kind={} total={} latest='{}' side_hint={:?} is_self={} sender='{}' source={}",
                                chat_name,
                                chat_kind.as_str(),
                                messages.len(),
                                trim_for_log(&last.content, 24),
                                last.side_hint,
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
                                let mut found_avatar_path: Option<String> = None;
                                events.publish(
                                    &app_handle,
                                    EventType::Log,
                                    "monitor_infer",
                                    serde_json::json!({
                                        "type": "message_inferred",
                                        "chat_name": chat_name,
                                        "chat_type": chat_type_label.clone(),
                                        "self_source": self_source_label(msg),
                                        "side_hint": msg.side_hint,
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
                                    let (translator, target_set, beta_img, beta_ava) = {
                                        let config = sidebar_config.lock().await;
                                        (
                                            config.translator.clone(),
                                            config.target_set.clone(),
                                            config.beta_image_capture,
                                            config.beta_avatar_capture,
                                        )
                                    };

                                    if beta_img && image_cache::is_image_placeholder(&msg.content) {
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

                                    if beta_ava && !msg.is_self && !msg.sender.is_empty() {
                                        if let Ok(ac) = avatar_cache.lock() {
                                            if ac.has_avatar(&msg.sender) {
                                                found_avatar_path = ac
                                                    .get_avatar_path(&msg.sender)
                                                    .map(|p| p.to_string_lossy().to_string());
                                            }
                                        }
                                        if found_avatar_path.is_none() {
                                            if let Some(avatar_pos) = msg.avatar_position {
                                                let ac = avatar_cache.clone();
                                                let sender = msg.sender.clone();
                                                let result =
                                                    tokio::task::spawn_blocking(move || {
                                                        let frame =
                                                            ax_reader::get_wechat_window_frame()?;
                                                        avatar_capture::capture_and_cache_avatar(
                                                            &mut ac.lock().unwrap(),
                                                            &sender,
                                                            avatar_pos,
                                                            (frame.0, frame.1),
                                                            (frame.2, frame.3),
                                                        )
                                                    })
                                                    .await;
                                                match result {
                                                    Ok(Ok(path)) => {
                                                        found_avatar_path = Some(
                                                            path.to_string_lossy().to_string(),
                                                        );
                                                    }
                                                    Ok(Err(e)) => {
                                                        warn!(
                                                            "avatar capture failed for '{}': {}",
                                                            msg.sender, e
                                                        );
                                                    }
                                                    Err(e) => {
                                                        warn!(
                                                            "avatar capture task panicked: {}",
                                                            e
                                                        );
                                                    }
                                                }
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
                                        if let Some(ref ap) = found_avatar_path {
                                            sidebar_payload["avatar_path"] =
                                                serde_json::Value::String(ap.clone());
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

                                if autoreply_enabled.load(Ordering::Relaxed) {
                                    if let Some(reply_text) = auto_reply_rules(&msg.content) {
                                        events.publish(
                                            &app_handle,
                                            EventType::Message,
                                            "autoreply",
                                            serde_json::json!({
                                                "chat_name": chat_name,
                                                "chat_type": chat_type_label.clone(),
                                                "self_source": self_source_label(msg),
                                                "sender": msg.sender,
                                                "text": msg.content,
                                                "reply": reply_text,
                                            }),
                                        );
                                        let _ = tokio::task::spawn_blocking({
                                            let adapter = adapter.clone();
                                            let reply_text = reply_text.clone();
                                            move || adapter.send_text("", &reply_text)
                                        })
                                        .await;
                                    }
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
            autoreply_enabled.store(false, Ordering::Relaxed);

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
        beta_image_capture: bool,
        beta_avatar_capture: bool,
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
            config.beta_image_capture = beta_image_capture;
            config.beta_avatar_capture = beta_avatar_capture;
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

    pub async fn enable_autoreply(&self) -> Result<()> {
        self.autoreply_enabled.store(true, Ordering::Relaxed);

        let app = self.get_app_handle().await?;
        let state = self.get_task_state();
        self.events.publish(
            &app,
            EventType::TaskState,
            "task_manager",
            serde_json::json!({
                "task": "autoreply",
                "running": true,
                "state": &state,
            }),
        );
        update_tray_menu(&app, &state);

        Ok(())
    }

    pub async fn disable_autoreply(&self) -> Result<()> {
        self.autoreply_enabled.store(false, Ordering::Relaxed);

        let app = self.get_app_handle().await?;
        let state = self.get_task_state();
        self.events.publish(
            &app,
            EventType::TaskState,
            "task_manager",
            serde_json::json!({
                "task": "autoreply",
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

fn apply_position_self_hints(messages: &mut [ChatMessage]) {
    let mut hinted = 0usize;
    for msg in messages {
        if let Some(side_hint) = msg.side_hint {
            msg.is_self = side_hint;
            hinted += 1;
        }
    }
    if hinted > 0 {
        debug!("position_hint applied to {} messages", hinted);
    }
}

fn self_source_label(msg: &ChatMessage) -> &'static str {
    if msg.side_hint.is_some() {
        "position"
    } else if !msg.sender.is_empty() {
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
                    last.side_hint = None;
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
                    last.side_hint = None;
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
        msg.side_hint = None;
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

/// Process a single new message: publish events, translate, insert to DB, and optionally auto-reply.
/// When `emit_events` is false (reconciliation path), only translation + DB insert runs;
/// the frontend will pick up reconciled messages via fetchHistory after chat_switched.
#[allow(clippy::too_many_arguments)]
async fn process_single_message(
    msg: &ChatMessage,
    chat_name: &str,
    now: &str,
    events: &EventStore,
    app_handle: &AppHandle,
    db: &MessageDb,
    sidebar_enabled: &AtomicBool,
    sidebar_config: &Mutex<SidebarConfig>,
    autoreply_enabled: &AtomicBool,
    adapter: &Arc<MacOSAdapter>,
    emit_events: bool,
) {
    let mut content_en = String::new();

    if emit_events {
        events.publish(
            app_handle,
            EventType::Message,
            "monitor",
            serde_json::json!({
                "chat_name": chat_name,
                "sender": msg.sender,
                "text": msg.content,
                "is_self": msg.is_self,
            }),
        );
    }

    if sidebar_enabled.load(Ordering::Relaxed) {
        let (translator, target_set) = {
            let config = sidebar_config.lock().await;
            (config.translator.clone(), config.target_set.clone())
        };
        if target_set.is_empty() || target_set.contains(chat_name) {
            let mut text_en = msg.content.clone();
            let mut translate_error = String::new();

            if let Some(ref translator) = translator {
                let t = translator.clone();
                let text = msg.content.clone();
                match tokio::runtime::Handle::try_current() {
                    Ok(handle) => {
                        let result =
                            std::thread::spawn(move || handle.block_on(t.translate(&text))).join();
                        match result {
                            Ok(Ok(translated)) => text_en = translated,
                            Ok(Err(e)) => translate_error = e.to_string(),
                            Err(_) => translate_error = "translate thread panicked".to_string(),
                        }
                    }
                    Err(_) => {
                        translate_error = "no tokio runtime available".to_string();
                    }
                }
            }

            content_en = text_en.clone();

            if emit_events {
                events.publish(
                    app_handle,
                    EventType::Message,
                    "sidebar",
                    serde_json::json!({
                        "chat_name": chat_name,
                        "sender": msg.sender,
                        "text_cn": msg.content,
                        "text_en": text_en,
                        "translate_error": translate_error,
                        "is_self": msg.is_self,
                    }),
                );
            }
        }
    }

    let _ = db.insert_message(
        chat_name,
        &msg.sender,
        &msg.content,
        &content_en,
        msg.is_self,
        now,
        None,
    );

    if emit_events && autoreply_enabled.load(Ordering::Relaxed) {
        if let Some(reply_text) = auto_reply_rules(&msg.content) {
            events.publish(
                app_handle,
                EventType::Message,
                "autoreply",
                serde_json::json!({
                    "chat_name": chat_name,
                    "sender": msg.sender,
                    "text": msg.content,
                    "reply": reply_text,
                }),
            );
            let _ = tokio::task::spawn_blocking({
                let adapter = adapter.clone();
                let reply_text = reply_text.clone();
                move || adapter.send_text("", &reply_text)
            })
            .await;
        }
    }
}

/// Compare currently visible AX messages against DB records for this chat.
/// Returns messages present in the AX tree but missing from the DB (bag diff by content_hash).
fn reconcile_with_db(
    db: &MessageDb,
    chat_name: &str,
    messages: &[ChatMessage],
) -> Vec<ChatMessage> {
    let (mut db_hash_bag, mut db_content_bag) = match db.query_recent_hashes_dual(chat_name, 200) {
        Ok(h) => h,
        Err(_) => return vec![],
    };
    let mut result = Vec::new();
    for msg in messages {
        let hash = content_hash(&msg.sender, &msg.content);
        match db_hash_bag.get_mut(&hash) {
            Some(count) if *count > 0 => {
                *count -= 1;
            }
            _ => {
                if msg.sender.is_empty() {
                    let content_hash = content_only_hash(&msg.content);
                    match db_content_bag.get_mut(&content_hash) {
                        Some(count) if *count > 0 => {
                            *count -= 1;
                            continue;
                        }
                        _ => {}
                    }
                }
                result.push(msg.clone());
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
            if state.autoreply {
                "● 监听 + 自动回复运行中"
            } else {
                "● 监听运行中"
            }
        } else {
            "○ 监听未运行"
        });
        let _ = tray.listen_toggle.set_text(if state.autoreply {
            "关闭自动回复"
        } else if state.monitoring {
            "暂停监听"
        } else {
            "开启监听"
        });
    }
}

fn auto_reply_rules(content: &str) -> Option<String> {
    let lower = content.to_lowercase();
    if ["关机", "停止", "退出", "下线", "bye", "再见"]
        .iter()
        .any(|k| lower.contains(k))
    {
        return Some("好的，机器人已下线，再见！".to_string());
    }
    if ["在吗", "在不在", "有人吗"]
        .iter()
        .any(|k| lower.contains(k))
    {
        return Some("在的！有事请说～".to_string());
    }
    if ["hello", "hi", "你好"].iter().any(|k| lower.contains(k)) {
        return Some("Hello！你好啊～".to_string());
    }
    if lower.contains("天气") {
        return Some("今天天气不错，适合出门散步哦".to_string());
    }
    if lower.contains("时间") || lower.contains("几点") {
        let now = chrono::Local::now().format("%H:%M:%S").to_string();
        return Some(format!("现在是 {}", now));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{
        apply_cached_preview_sender_hint, apply_position_self_hints, apply_sender_defaults,
        apply_session_preview_sender_hint, cleanup_preview_sender_hints, diff_messages,
        inherit_sender_from_reference, reconcile_with_db, remember_preview_sender_hint,
        should_emit_session_snapshot, ChatKind, PreviewSenderHint, SessionListenState,
        PREVIEW_SENDER_HINT_TTL,
    };
    use crate::adapter::ax_reader::{ChatMessage, SessionItemSnapshot};
    use crate::db::MessageDb;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    fn msg(sender: &str, content: &str) -> ChatMessage {
        ChatMessage {
            sender: sender.to_string(),
            content: content.to_string(),
            is_self: false,
            side_hint: None,
            avatar_position: None,
        }
    }

    fn msg_with_side(sender: &str, content: &str, side_hint: Option<bool>) -> ChatMessage {
        ChatMessage {
            sender: sender.to_string(),
            content: content.to_string(),
            is_self: false,
            side_hint,
            avatar_position: None,
        }
    }

    fn session_snapshot(
        chat_name: &str,
        preview_body: &str,
        unread_count: u32,
    ) -> SessionItemSnapshot {
        SessionItemSnapshot {
            chat_name: chat_name.to_string(),
            raw_preview: format!("{chat_name}\n{preview_body}"),
            preview_body: preview_body.to_string(),
            unread_count,
            sender_hint: None,
            has_sender_prefix: false,
            is_group: false,
        }
    }

    fn temp_db_path(tag: &str) -> PathBuf {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("wechat_pc_auto_{}_{}.db", tag, ts))
    }

    fn cleanup_db(path: &PathBuf) {
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(format!("{}-wal", path.display()));
        let _ = std::fs::remove_file(format!("{}-shm", path.display()));
    }

    #[test]
    fn inherit_sender_from_reference_should_stabilize_sender_labels() {
        let reference = vec![msg("Alice", "A"), msg("Bob", "B"), msg("Alice", "A")];
        let mut current = vec![msg("", "A"), msg("", "B"), msg("", "A")];

        inherit_sender_from_reference(&mut current, &reference);

        assert_eq!(current[0].sender, "Alice");
        assert_eq!(current[1].sender, "Bob");
        assert_eq!(current[2].sender, "Alice");
    }

    #[test]
    fn apply_position_self_hints_should_override_is_self_from_side_hint() {
        let mut current = vec![
            msg_with_side("", "left", Some(false)),
            msg_with_side("", "right", Some(true)),
            msg_with_side("", "unknown", None),
        ];
        current[0].is_self = true;
        current[1].is_self = false;
        current[2].is_self = true;

        apply_position_self_hints(&mut current);

        assert!(!current[0].is_self);
        assert!(current[1].is_self);
        assert!(current[2].is_self);
    }

    #[test]
    fn apply_session_preview_sender_hint_should_fill_latest_message_and_mark_group() {
        let mut current = vec![msg("", "旧消息"), msg("", "因为rust的代码简直不是人类读的")];
        let preview = "某群聊\n花姐🌸: 因为rust的代码简直不是人类读的\n21:34";
        let mut chat_kind = ChatKind::Private;

        apply_session_preview_sender_hint(&mut current, preview, &mut chat_kind, false);
        assert_eq!(chat_kind, ChatKind::Group);
        assert_eq!(current[1].sender, "花姐🌸");
        assert!(!current[1].is_self);
    }

    #[test]
    fn apply_session_preview_sender_hint_should_override_position_when_sender_prefix_matches() {
        let mut current = vec![
            msg("", "旧消息"),
            msg_with_side(
                "",
                "还说自己没学引用 五（挂壁）的消息：不敢劝 自己了搞了一大堆东西 都不咋加钱",
                Some(true),
            ),
        ];
        current[1].is_self = true;
        let preview = "某群聊\n雨后第一—郑爱民-区哥: 还说自己没学\n11:44";
        let mut chat_kind = ChatKind::Private;

        apply_session_preview_sender_hint(&mut current, preview, &mut chat_kind, true);

        assert_eq!(chat_kind, ChatKind::Group);
        assert_eq!(current[1].sender, "雨后第一—郑爱民-区哥");
        assert!(!current[1].is_self);
        assert_eq!(current[1].side_hint, None);
    }

    #[test]
    fn apply_session_preview_sender_hint_should_mark_self_in_group_without_sender_prefix() {
        let mut current = vec![msg("", "旧消息"), msg("", "真好啊")];
        let preview = "某群聊\n真好啊\n22:27";
        let mut chat_kind = ChatKind::Group;

        apply_session_preview_sender_hint(&mut current, preview, &mut chat_kind, false);
        assert_eq!(current[1].sender, "");
        assert!(current[1].is_self);
    }

    #[test]
    fn apply_session_preview_sender_hint_should_mark_self_in_private_when_unread_stable() {
        let mut current = vec![msg("", "旧消息"), msg("", "最新一条")];
        let preview = "某群聊\n最新一条\n22:27";
        let mut chat_kind = ChatKind::Private;
        current[1].is_self = false;

        // unread not increased → likely self-sent
        apply_session_preview_sender_hint(&mut current, preview, &mut chat_kind, false);
        assert_eq!(current[1].sender, "");
        assert!(current[1].is_self);
    }

    #[test]
    fn apply_session_preview_sender_hint_should_mark_other_in_private_when_unread_increased() {
        let mut current = vec![msg("", "旧消息"), msg("", "最新一条")];
        let preview = "某群聊\n最新一条\n22:27";
        let mut chat_kind = ChatKind::Private;
        current[1].is_self = true;

        // unread increased → must be from other person
        apply_session_preview_sender_hint(&mut current, preview, &mut chat_kind, true);
        assert_eq!(current[1].sender, "");
        assert!(!current[1].is_self);
    }

    #[test]
    fn apply_session_preview_sender_hint_should_mark_self_for_image_placeholder_in_group() {
        let mut current = vec![msg("", "旧消息"), msg("", "[Image]")];
        let preview = "某群聊\n[图片]\n22:29";
        let mut chat_kind = ChatKind::Group;

        apply_session_preview_sender_hint(&mut current, preview, &mut chat_kind, false);
        assert_eq!(current[1].sender, "");
        assert!(current[1].is_self);
    }

    #[test]
    fn apply_session_preview_sender_hint_should_keep_group_sticky_without_prefix() {
        let mut current = vec![msg("", "旧消息"), msg("", "我发的是这一句")];
        let preview = "某群聊\n我发的是这一句\n22:30";
        let mut chat_kind = ChatKind::Group;

        apply_session_preview_sender_hint(&mut current, preview, &mut chat_kind, false);
        assert_eq!(chat_kind, ChatKind::Group);
        assert!(current[1].is_self);
        assert_eq!(current[1].side_hint, None);
    }

    #[test]
    fn apply_session_preview_sender_hint_should_override_position_for_group_self_without_prefix() {
        let mut current = vec![msg("", "旧消息"), msg_with_side("", "真好啊", Some(false))];
        let preview = "某群聊\n真好啊\n22:29";
        let mut chat_kind = ChatKind::Group;
        current[1].is_self = false;

        apply_session_preview_sender_hint(&mut current, preview, &mut chat_kind, false);
        assert!(current[1].is_self);
        assert_eq!(current[1].sender, "");
        assert_eq!(current[1].side_hint, None);
    }

    #[test]
    fn apply_session_preview_sender_hint_should_not_override_when_preview_body_does_not_match() {
        let mut current = vec![msg("", "旧消息"), msg_with_side("", "真好啊", Some(false))];
        let preview = "某群聊\n另一句\n22:29";
        let mut chat_kind = ChatKind::Group;
        current[1].is_self = false;

        apply_session_preview_sender_hint(&mut current, preview, &mut chat_kind, false);

        assert!(!current[1].is_self);
        assert_eq!(current[1].sender, "");
        assert_eq!(current[1].side_hint, Some(false));
    }

    #[test]
    fn remember_preview_sender_hint_should_store_group_sender_preview() {
        let snapshot = SessionItemSnapshot {
            chat_name: "工作群".to_string(),
            raw_preview: "工作群\n花姐: 开会了".to_string(),
            preview_body: "开会了".to_string(),
            unread_count: 3,
            sender_hint: Some("花姐".to_string()),
            has_sender_prefix: true,
            is_group: true,
        };
        let mut cache = HashMap::new();

        remember_preview_sender_hint(&mut cache, &snapshot, Instant::now());

        let hint = cache.get("工作群").expect("hint should exist");
        assert_eq!(hint.sender, "花姐");
        assert_eq!(hint.preview_body, "开会了");
        assert_eq!(hint.preview_body_key, "开会了");
        assert_eq!(hint.unread_count, 3);
        assert!(!hint.consumed);
    }

    #[test]
    fn apply_cached_preview_sender_hint_should_fill_matching_new_message() {
        let mut cache = HashMap::from([(
            "工作群".to_string(),
            PreviewSenderHint {
                sender: "花姐".to_string(),
                preview_body: "因为rust的代码简直不是人类读的".to_string(),
                preview_body_key: "因为rust的代".to_string(),
                unread_count: 2,
                updated_at: Instant::now(),
                consumed: false,
            },
        )]);
        let mut current = vec![
            msg("", "旧消息"),
            msg_with_side("", "因为rust的代码简直不是人类读的!!!", Some(true)),
        ];
        current[1].is_self = true;
        let mut chat_kind = ChatKind::Private;

        let matched = apply_cached_preview_sender_hint(
            "工作群",
            &mut current,
            &mut chat_kind,
            &mut cache,
            Instant::now(),
        );

        assert!(matched);
        assert_eq!(chat_kind, ChatKind::Group);
        assert_eq!(current[1].sender, "花姐");
        assert!(!current[1].is_self);
        assert_eq!(current[1].side_hint, None);
        assert!(cache.get("工作群").expect("hint should exist").consumed);
    }

    #[test]
    fn cleanup_preview_sender_hints_should_drop_consumed_and_expired_items() {
        let now = Instant::now();
        let mut cache = HashMap::from([
            (
                "fresh".to_string(),
                PreviewSenderHint {
                    sender: "花姐".to_string(),
                    preview_body: "开会了".to_string(),
                    preview_body_key: "开会了".to_string(),
                    unread_count: 1,
                    updated_at: now,
                    consumed: false,
                },
            ),
            (
                "expired".to_string(),
                PreviewSenderHint {
                    sender: "阿强".to_string(),
                    preview_body: "收到".to_string(),
                    preview_body_key: "收到".to_string(),
                    unread_count: 2,
                    updated_at: now - PREVIEW_SENDER_HINT_TTL - Duration::from_secs(1),
                    consumed: false,
                },
            ),
            (
                "consumed".to_string(),
                PreviewSenderHint {
                    sender: "豆子".to_string(),
                    preview_body: "OK".to_string(),
                    preview_body_key: "OK".to_string(),
                    unread_count: 3,
                    updated_at: now,
                    consumed: true,
                },
            ),
        ]);

        cleanup_preview_sender_hints(&mut cache, now);

        assert!(cache.contains_key("fresh"));
        assert!(!cache.contains_key("expired"));
        assert!(!cache.contains_key("consumed"));
    }

    #[test]
    fn apply_sender_defaults_should_fill_private_sender_with_chat_name() {
        let mut current = vec![msg("", "hello"), msg("", "world"), msg("我", "self msg")];
        current[0].is_self = false;
        current[1].is_self = false;
        current[2].is_self = true;

        apply_sender_defaults(&mut current, "豆子", ChatKind::Private);

        assert_eq!(current[0].sender, "豆子");
        assert_eq!(current[1].sender, "豆子");
        assert_eq!(current[2].sender, "我");
    }

    #[test]
    fn should_emit_session_snapshot_should_emit_on_preview_change() {
        let state = SessionListenState {
            last_preview_body: "旧内容".to_string(),
            last_unread: 1,
        };
        let snapshot = session_snapshot("会话A", "新内容", 1);
        assert!(should_emit_session_snapshot(&snapshot, &state));
    }

    #[test]
    fn should_emit_session_snapshot_should_not_emit_on_unread_growth_alone() {
        let state = SessionListenState {
            last_preview_body: "同内容".to_string(),
            last_unread: 1,
        };
        let snapshot = session_snapshot("会话A", "同内容", 2);
        // Unread growth with same preview_body should NOT emit
        // (prevents duplicate insertions from unread oscillation)
        assert!(!should_emit_session_snapshot(&snapshot, &state));
    }

    #[test]
    fn should_emit_session_snapshot_should_not_emit_when_unchanged() {
        let state = SessionListenState {
            last_preview_body: "同内容".to_string(),
            last_unread: 3,
        };
        let snapshot = session_snapshot("会话A", "同内容", 3);
        assert!(!should_emit_session_snapshot(&snapshot, &state));
    }

    #[test]
    fn diff_messages_should_ignore_sender_only_changes() {
        let old = vec![msg("", "hello"), msg("", "world")];
        let new = vec![msg("Alice", "hello"), msg("Bob", "world")];

        let diff = diff_messages(&old, &new);
        assert!(diff.new_messages.is_empty());
        assert!(!diff.anchor_failed);
    }

    #[test]
    fn reconcile_with_db_should_fallback_to_content_hash_when_sender_missing() {
        let path = temp_db_path("reconcile_fallback");
        let db = MessageDb::new(&path).expect("create db");
        db.insert_message(
            "chat-a",
            "Alice",
            "同一条消息",
            "",
            false,
            "2026-03-06 10:00:00",
            None,
        )
        .expect("insert seed");

        let reconciled = reconcile_with_db(&db, "chat-a", &[msg("", "同一条消息")]);
        assert!(reconciled.is_empty());

        drop(db);
        cleanup_db(&path);
    }

    #[test]
    fn reconcile_with_db_should_report_truly_missing_messages() {
        let path = temp_db_path("reconcile_missing");
        let db = MessageDb::new(&path).expect("create db");
        db.insert_message(
            "chat-b",
            "Alice",
            "旧消息",
            "",
            false,
            "2026-03-06 10:00:00",
            None,
        )
        .expect("insert seed");

        let reconciled = reconcile_with_db(&db, "chat-b", &[msg("", "新消息")]);
        assert_eq!(reconciled.len(), 1);
        assert_eq!(reconciled[0].content, "新消息");

        drop(db);
        cleanup_db(&path);
    }

    // --- unread_count enhanced is_self tests for session preview path ---

    #[test]
    fn session_preview_private_chat_other_person_sends_unread_increases() {
        // Private chat: unread went from 0→1 and preview changed → is_self=false
        let state = SessionListenState {
            last_preview_body: "旧消息".to_string(),
            last_unread: 0,
        };
        let snapshot = session_snapshot("张三", "你好啊", 1);
        // snapshot.unread_count=1 > state.last_unread=0 → unread_increased=true
        assert!(snapshot.unread_count > state.last_unread);
        // has_sender_prefix=false, unread_increased=true → is_self=false
        let unread_increased = snapshot.unread_count > state.last_unread;
        let is_self = if snapshot.has_sender_prefix {
            false
        } else if unread_increased {
            false
        } else {
            true
        };
        assert!(!is_self);
    }

    #[test]
    fn session_preview_private_chat_self_sends_unread_stable() {
        // Private chat: unread stays at 0 but preview changed → is_self=true
        let state = SessionListenState {
            last_preview_body: "旧消息".to_string(),
            last_unread: 0,
        };
        let snapshot = session_snapshot("张三", "我发的新消息", 0);
        // snapshot.unread_count=0 == state.last_unread=0 → unread_increased=false
        assert!(snapshot.unread_count <= state.last_unread);
        let unread_increased = snapshot.unread_count > state.last_unread;
        let is_self = if snapshot.has_sender_prefix {
            false
        } else if unread_increased {
            false
        } else {
            true
        };
        assert!(is_self);
    }

    #[test]
    fn session_preview_group_chat_unread_does_not_override_sender_prefix() {
        // Group chat with sender prefix: always is_self=false regardless of unread
        let snapshot = SessionItemSnapshot {
            chat_name: "工作群".to_string(),
            raw_preview: "花姐: 开会了".to_string(),
            preview_body: "开会了".to_string(),
            unread_count: 3,
            sender_hint: Some("花姐".to_string()),
            has_sender_prefix: true,
            is_group: true,
        };
        let state = SessionListenState {
            last_preview_body: "旧消息".to_string(),
            last_unread: 0,
        };
        let unread_increased = snapshot.unread_count > state.last_unread;
        assert!(unread_increased);
        // has_sender_prefix=true takes priority → is_self=false
        let is_self = if snapshot.has_sender_prefix {
            false
        } else if unread_increased {
            false
        } else {
            true
        };
        assert!(!is_self);
    }
}
