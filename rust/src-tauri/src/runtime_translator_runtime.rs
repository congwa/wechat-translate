use crate::db::MessageDb;
use crate::events::{EventStore, EventType, ServiceEvent};
use crate::runtime_monitor_ingest::short_error_text;
use crate::sidebar_projection::emit_sidebar_invalidated;
use crate::task_manager::TaskManager;
use crate::translator::{TranslationLimiter, TranslatorServiceStatus};
use std::sync::Arc;
use tauri::AppHandle;

pub(crate) fn publish_sidebar_append(
    events: &EventStore,
    app_handle: &AppHandle,
    chat_name: &str,
    chat_type: &str,
    self_source: &str,
    source: &str,
    quality: &str,
    sender: &str,
    text_cn: &str,
    is_self: bool,
    image_path: Option<&str>,
) -> ServiceEvent {
    let mut payload = serde_json::json!({
        "kind": "append",
        "chat_name": chat_name,
        "chat_type": chat_type,
        "self_source": self_source,
        "source": source,
        "quality": quality,
        "sender": sender,
        "text_cn": text_cn,
        "text_en": "",
        "translate_error": "",
        "is_self": is_self,
    });
    if let Some(path) = image_path {
        payload["image_path"] = serde_json::Value::String(path.to_string());
    }

    events.publish(app_handle, EventType::Message, "sidebar", payload)
}

pub(crate) fn spawn_sidebar_translation_update(
    manager: TaskManager,
    events: Arc<EventStore>,
    app_handle: AppHandle,
    db: Arc<MessageDb>,
    translator: Arc<dyn crate::translator::Translator>,
    limiter: Arc<TranslationLimiter>,
    source_lang: String,
    target_lang: String,
    message_id: u64,
    chat_name: String,
    sender: String,
    content: String,
    detected_at: String,
) {
    tokio::spawn(async move {
        log::info!(
            "[spawn_sidebar_translation_update] 翻译任务启动: message_id={}",
            message_id
        );
        let translator_generation = manager.current_translator_generation();

        if let Ok(Some(cached)) = db.get_cached_translation(&content, &source_lang, &target_lang) {
            log::info!(
                "[spawn_sidebar_translation_update] 使用缓存翻译: {}",
                &cached.translated_text[..cached.translated_text.len().min(50)]
            );
            let _ = db.update_message_translation(
                &chat_name,
                &sender,
                &content,
                &detected_at,
                &cached.translated_text,
            );

            let refresh_version = manager.get_sidebar_runtime().increment_refresh_version();
            emit_sidebar_invalidated(&app_handle, &events, &chat_name, refresh_version);

            events.publish(
                &app_handle,
                EventType::Message,
                "sidebar",
                serde_json::json!({
                    "kind": "update",
                    "message_id": message_id,
                    "chat_name": chat_name,
                    "text_en": cached.translated_text,
                    "translate_error": "",
                }),
            );
            return;
        }

        log::info!("[spawn_sidebar_translation_update] 开始实际翻译");
        let _permit = limiter.acquire().await;
        log::info!("[spawn_sidebar_translation_update] 获得限流许可");
        match translator.translate(&content, &source_lang, &target_lang).await {
            Ok(translated) => {
                log::info!(
                    "[spawn_sidebar_translation_update] 翻译成功: {}",
                    &translated[..translated.len().min(50)]
                );
                let _ =
                    db.upsert_cached_translation(&content, &source_lang, &target_lang, &translated);
                let _ = db.update_message_translation(
                    &chat_name,
                    &sender,
                    &content,
                    &detected_at,
                    &translated,
                );
                let provider = translator.provider_id().to_string();
                manager
                    .set_translator_status_if_current(
                        translator_generation,
                        TranslatorServiceStatus::healthy(&provider),
                    )
                    .await;

                let refresh_version = manager.get_sidebar_runtime().increment_refresh_version();
                emit_sidebar_invalidated(&app_handle, &events, &chat_name, refresh_version);

                events.publish(
                    &app_handle,
                    EventType::Message,
                    "sidebar",
                    serde_json::json!({
                        "kind": "update",
                        "message_id": message_id,
                        "chat_name": chat_name,
                        "text_en": translated,
                        "translate_error": "",
                    }),
                );
            }
            Err(error) => {
                let translate_error = error.to_string();
                let provider = translator.provider_id().to_string();
                manager
                    .set_translator_status_if_current(
                        translator_generation,
                        TranslatorServiceStatus::error(&provider, &translate_error),
                    )
                    .await;
                events.publish(
                    &app_handle,
                    EventType::Message,
                    "sidebar",
                    serde_json::json!({
                        "kind": "update",
                        "message_id": message_id,
                        "chat_name": chat_name,
                        "text_en": "",
                        "translate_error": short_error_text(&translate_error),
                    }),
                );
            }
        }
    });
}
