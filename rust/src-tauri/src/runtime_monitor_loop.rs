use crate::adapter::ax_reader::{self, ChatMessage};
use crate::adapter::MacOSAdapter;
use crate::application::runtime::lifecycle::{finalize_monitor_loop, RuntimeLifecycleContext};
use crate::application::runtime::monitor_ingest::{
    apply_cached_preview_sender_hint, apply_sender_defaults, apply_session_preview_sender_hint,
    cleanup_preview_sender_hints, diff_messages, inherit_sender_from_reference,
    remember_preview_sender_hint, self_source_label, should_emit_session_snapshot,
    should_forward_session_preview, should_forward_sidebar_chat, trim_for_log, ChatKind,
    PreviewSenderHint, SessionListenState,
};
use crate::application::runtime::read_service::{self as runtime_read, RuntimeReadContext};
use crate::application::runtime::state::{FirstPollSignal, MonitorConfig, SidebarConfig};
use crate::application::runtime::status_sync::RuntimeStatusContext;
use crate::application::runtime::translator_runtime::{
    publish_sidebar_append, spawn_sidebar_translation_update,
};
use crate::application::sidebar::projection_service::{emit_sidebar_invalidated, SidebarRuntime};
use crate::db::MessageDb;
use crate::events::{EventStore, EventType};
use crate::image_cache::{self, WeChatImageCache};
use anyhow::Result;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tauri::AppHandle;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

pub(crate) struct MonitorLoopContext {
    pub token: CancellationToken,
    pub adapter: Arc<MacOSAdapter>,
    pub events: Arc<EventStore>,
    pub db: Arc<MessageDb>,
    pub image_cache: Arc<std::sync::Mutex<WeChatImageCache>>,
    pub monitor_token_ref: Arc<Mutex<Option<CancellationToken>>>,
    pub monitoring_active: Arc<AtomicBool>,
    pub monitor_config: Arc<Mutex<MonitorConfig>>,
    pub first_poll_signal: Arc<FirstPollSignal>,
    pub sidebar_enabled: Arc<AtomicBool>,
    pub sidebar_config: Arc<Mutex<SidebarConfig>>,
    pub sidebar_runtime: Arc<SidebarRuntime>,
    pub app_handle: AppHandle,
    pub read: RuntimeReadContext,
    pub status: RuntimeStatusContext,
    pub lifecycle: RuntimeLifecycleContext,
    pub poll_interval: f64,
}

/// 启动监听主循环：持续轮询微信 UI、判定消息增量，并把结果投影到数据库和 sidebar。
pub(crate) fn spawn_monitor_loop(ctx: MonitorLoopContext) {
    tokio::spawn(async move {
        let MonitorLoopContext {
            token,
            adapter,
            events,
            db,
            image_cache,
            monitor_token_ref,
            monitoring_active,
            monitor_config,
            first_poll_signal,
            sidebar_enabled,
            sidebar_config,
            sidebar_runtime,
            app_handle,
            read,
            status,
            lifecycle,
            poll_interval,
        } = ctx;

        let mut session_state: HashMap<String, SessionListenState> = HashMap::new();
        let mut chat_baselines: HashMap<String, Vec<ChatMessage>> = HashMap::new();
        let mut chat_kinds: HashMap<String, ChatKind> = HashMap::new();
        let mut preview_sender_hints: HashMap<String, PreviewSenderHint> = HashMap::new();
        let mut active_chat_name = String::new();
        let mut pending_chat_name: Option<String> = None;
        let mut pending_count: u32 = 0;
        let mut last_use_right_panel_details: Option<bool> = None;
        let mut first_poll_signaled = false;
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

            let use_right_panel_details = monitor_config.lock().await.use_right_panel_details;
            if last_use_right_panel_details != Some(use_right_panel_details) {
                chat_baselines.clear();
                last_use_right_panel_details = Some(use_right_panel_details);
            }

            let poll_result = tokio::task::spawn_blocking({
                let adapter = adapter.clone();
                move || -> Result<(
                    String,
                    Vec<ax_reader::SessionItemSnapshot>,
                    Vec<ChatMessage>,
                    Option<u32>,
                )> {
                    if adapter.is_ui_paused() || adapter.has_popup_or_menu() {
                        anyhow::bail!("ui paused or popup detected, skip");
                    }
                    let chat_name = adapter.read_active_chat_name()?;
                    let snapshots = adapter.read_session_snapshots()?;
                    let member_count = adapter.read_active_chat_member_count().unwrap_or(None);
                    let messages = if use_right_panel_details {
                        adapter.read_chat_messages_rich().unwrap_or_default()
                    } else {
                        Vec::new()
                    };
                    Ok((chat_name, snapshots, messages, member_count))
                }
            })
            .await;

            if let Ok(Ok((chat_name, snapshots, mut messages, member_count))) = poll_result {
                if !first_poll_signaled {
                    first_poll_signal.signal_ready(&chat_name);
                    first_poll_signaled = true;
                }

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
                            false
                        } else if unread_increased {
                            false
                        } else if matches!(chat_kind, ChatKind::Group) {
                            sender.clear();
                            true
                        } else {
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

                        if should_forward_session_preview(
                            use_right_panel_details,
                            &snapshot.chat_name,
                            &chat_name,
                        ) {
                            let _ = db.insert_message_with_meta(
                                &snapshot.chat_name,
                                Some(chat_kind.as_str()),
                                &sender,
                                &snapshot.preview_body,
                                "",
                                is_self,
                                &now,
                                None,
                                "session_preview",
                                "low",
                            );

                            if sidebar_enabled.load(Ordering::Relaxed)
                                && should_forward_sidebar_chat(&snapshot.chat_name)
                                && snapshot.chat_name == chat_name
                            {
                                let refresh_version =
                                    sidebar_runtime.update_chat_and_version(&snapshot.chat_name);
                                emit_sidebar_invalidated(
                                    &app_handle,
                                    &events,
                                    &snapshot.chat_name,
                                    refresh_version,
                                );

                                let (translator, limiter) = {
                                    let config = sidebar_config.lock().await;
                                    (config.translator.clone(), config.limiter.clone())
                                };
                                let translate_config =
                                    runtime_read::translation_service(&read).get_config().await;
                                if let (Some(translator), Some(limiter)) = (translator, limiter) {
                                    spawn_sidebar_translation_update(
                                        status.clone(),
                                        events.clone(),
                                        app_handle.clone(),
                                        db.clone(),
                                        sidebar_runtime.clone(),
                                        translator,
                                        limiter,
                                        translate_config.source_lang.clone(),
                                        translate_config.target_lang.clone(),
                                        0,
                                        snapshot.chat_name.clone(),
                                        sender.clone(),
                                        snapshot.preview_body.clone(),
                                        now.clone(),
                                    );
                                }
                            }
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

                                if sidebar_enabled.load(Ordering::Relaxed)
                                    && should_forward_sidebar_chat(&chat_name)
                                {
                                    let refresh_version =
                                        sidebar_runtime.update_chat_and_version(&chat_name);
                                    emit_sidebar_invalidated(
                                        &app_handle,
                                        &events,
                                        &chat_name,
                                        refresh_version,
                                    );
                                }
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

                if use_right_panel_details && !messages.is_empty() {
                    let mut chat_kind = chat_kinds
                        .get(&chat_name)
                        .copied()
                        .unwrap_or(ChatKind::Unknown);

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
                        log::debug!(
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
                            let content_en = String::new();
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
                                let (_translator, _target_set, image_capture) = {
                                    let config = sidebar_config.lock().await;
                                    (
                                        config.translator.clone(),
                                        config.target_set.clone(),
                                        config.image_capture,
                                    )
                                };

                                if image_capture && image_cache::is_image_placeholder(&msg.content)
                                {
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
                            }

                            let corrected = db
                                .try_correct_preview_row(
                                    &chat_name,
                                    Some(&chat_type_label),
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
                                    Some(&chat_type_label),
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

                            if sidebar_enabled.load(Ordering::Relaxed)
                                && should_forward_sidebar_chat(&chat_name)
                            {
                                let refresh_version =
                                    sidebar_runtime.update_chat_and_version(&chat_name);

                                events.publish(
                                    &app_handle,
                                    EventType::Status,
                                    "monitor",
                                    serde_json::json!({
                                        "type": "chat_switched",
                                        "chat_name": chat_name,
                                    }),
                                );

                                emit_sidebar_invalidated(
                                    &app_handle,
                                    &events,
                                    &chat_name,
                                    refresh_version,
                                );

                                let (translator, limiter) = {
                                    let config = sidebar_config.lock().await;
                                    (config.translator.clone(), config.limiter.clone())
                                };
                                let translate_config =
                                    runtime_read::translation_service(&read).get_config().await;

                                let sidebar_event = publish_sidebar_append(
                                    &events,
                                    &app_handle,
                                    &chat_name,
                                    &chat_type_label,
                                    self_source_label(msg),
                                    "chat",
                                    "high",
                                    &msg.sender,
                                    &msg.content,
                                    msg.is_self,
                                    found_image_path.as_deref(),
                                );

                                if let (Some(translator), Some(limiter)) = (translator, limiter) {
                                    spawn_sidebar_translation_update(
                                        status.clone(),
                                        events.clone(),
                                        app_handle.clone(),
                                        db.clone(),
                                        sidebar_runtime.clone(),
                                        translator,
                                        limiter,
                                        translate_config.source_lang.clone(),
                                        translate_config.target_lang.clone(),
                                        sidebar_event.id,
                                        chat_name.clone(),
                                        msg.sender.clone(),
                                        msg.content.clone(),
                                        now.clone(),
                                    );
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
        finalize_monitor_loop(&lifecycle, &app_handle).await;
    });
}
