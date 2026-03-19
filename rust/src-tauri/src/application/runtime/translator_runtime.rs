//! 翻译运行时服务：负责翻译配置应用、健康探测和手动翻译触发，
//! 把高层翻译编排从 TaskManager 中抽离出来，低层写回仍复用既有实现。
use crate::app_state;
use crate::application::runtime::read_service as runtime_read;
use crate::application::runtime::service::RuntimeService;
use crate::application::runtime::state::SidebarConfig;
use crate::application::runtime::status_sync::RuntimeStatusContext;
use crate::application::runtime::translation_config::build_translate_config_from_app_config;
use crate::application::sidebar::projection_service::SidebarRuntime;
use crate::config::AppConfig;
use crate::db::MessageDb;
use crate::events::EventStore;
use crate::infrastructure::tauri::tray_adapter::update_tray_menu;
use crate::task_manager::TaskManager;
use crate::translator::TranslationService;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

pub(crate) use crate::runtime_translator_runtime::{
    publish_sidebar_append, spawn_sidebar_translation_update,
};

/// TranslatorRuntimeContext 汇总翻译运行态编排所需依赖，
/// 让应用层翻译服务不直接依赖 TaskManager 的字段布局。
#[derive(Clone)]
pub(crate) struct TranslatorRuntimeContext {
    pub(crate) manager: TaskManager,
    pub(crate) events: Arc<EventStore>,
    pub(crate) db: Arc<MessageDb>,
    pub(crate) translation_service: Arc<TranslationService>,
    pub(crate) sidebar_enabled: Arc<AtomicBool>,
    pub(crate) sidebar_config: Arc<Mutex<SidebarConfig>>,
    pub(crate) sidebar_runtime: Arc<SidebarRuntime>,
    pub(crate) status: RuntimeStatusContext,
}

/// 在配置保存后把翻译设置应用到运行态，并在 sidebar 已开启时同步刷新其译文依赖。
pub(crate) async fn apply_runtime_config(ctx: &TranslatorRuntimeContext, config: &AppConfig) {
    let translator_generation = ctx.manager.next_translator_generation();
    let translate_config = build_translate_config_from_app_config(config);
    let translator_status = ctx
        .translation_service
        .update_config(translate_config)
        .await;

    if ctx.sidebar_enabled.load(Ordering::Relaxed) {
        let (translator, limiter) = ctx.translation_service.get_translator_and_limiter().await;
        let mut sidebar_config = ctx.sidebar_config.lock().await;
        sidebar_config.translator = translator;
        sidebar_config.limiter = limiter;
    }

    let read = ctx.manager.read_context();
    if let Ok(app) = runtime_read::app_handle(&read).await {
        let state = runtime_read::task_state(&read);
        update_tray_menu(&app, &state, &translator_status);
        app_state::emit_runtime_updated(&app, RuntimeService::new(ctx.manager.clone()));
    }

    if translator_status.configured {
        ctx.manager
            .spawn_translator_health_check(translator_generation);
    }
}

/// 手动翻译指定消息，并复用统一的 sidebar 译文写回链路，避免出现第二套翻译落库流程。
pub(crate) async fn translate_message_manually(
    ctx: &TranslatorRuntimeContext,
    app: tauri::AppHandle,
    message_id: u64,
    chat_name: String,
    sender: String,
    content: String,
    detected_at: String,
) -> Result<(), String> {
    log::info!("[TranslatorRuntime] translate_message_manually 开始");
    let config = ctx.translation_service.get_config().await;
    if !config.enabled {
        return Err("翻译服务未启用".to_string());
    }

    let (translator, limiter) = ctx.translation_service.get_translator_and_limiter().await;
    let translator = translator.ok_or_else(|| "翻译服务未配置".to_string())?;
    let limiter = limiter.ok_or_else(|| "翻译限流器未配置".to_string())?;

    spawn_sidebar_translation_update(
        ctx.status.clone(),
        ctx.events.clone(),
        app,
        ctx.db.clone(),
        ctx.sidebar_runtime.clone(),
        translator,
        limiter,
        config.source_lang.clone(),
        config.target_lang.clone(),
        message_id,
        chat_name,
        sender,
        content,
        detected_at,
    );

    log::info!("[TranslatorRuntime] translate_message_manually 已提交");
    Ok(())
}
