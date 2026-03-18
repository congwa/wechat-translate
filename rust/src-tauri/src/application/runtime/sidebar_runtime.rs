//! Sidebar 运行时服务：负责 sidebar 启停时的翻译依赖装配、状态发布和投影清理，
//! 让 TaskManager 不再直接承载整段 sidebar 生命周期编排。
use crate::app_state;
use crate::application::runtime::service::RuntimeService;
use crate::application::runtime::translation_config::{
    build_translate_config_from_sidebar_params, SidebarTranslationRuntimeParams,
};
use crate::application::sidebar::projection_service::SidebarRuntime;
use crate::infrastructure::tauri::tray_adapter::update_tray_menu;
use crate::task_manager::TaskManager;
use crate::translator::{TranslationService, TranslatorServiceStatus};
use anyhow::Result;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::application::runtime::state::SidebarConfig;

/// SidebarRuntimeContext 汇总 sidebar 生命周期编排所需依赖，
/// 让 sidebar 启停逻辑与 TaskManager 字段布局解耦。
#[derive(Clone)]
pub(crate) struct SidebarRuntimeContext {
    pub(crate) manager: TaskManager,
    pub(crate) translation_service: Arc<TranslationService>,
    pub(crate) sidebar_enabled: Arc<AtomicBool>,
    pub(crate) sidebar_config: Arc<Mutex<SidebarConfig>>,
    pub(crate) sidebar_runtime: Arc<SidebarRuntime>,
}

/// 启用 sidebar 运行态，并把译文器、限流器和目标会话集合装配到统一配置里。
#[allow(clippy::too_many_arguments)]
pub(crate) async fn enable_sidebar(
    ctx: &SidebarRuntimeContext,
    targets: Vec<String>,
    translate_enabled: bool,
    provider: String,
    deeplx_url: String,
    ai_provider_id: String,
    ai_model_id: String,
    ai_api_key: String,
    ai_base_url: String,
    source_lang: String,
    target_lang: String,
    timeout_seconds: f64,
    max_concurrency: usize,
    max_requests_per_second: usize,
    image_capture: bool,
) -> Result<()> {
    let translator_generation = ctx.manager.next_translator_generation();
    let translate_config =
        build_translate_config_from_sidebar_params(SidebarTranslationRuntimeParams {
            translate_enabled,
            provider,
            deeplx_url,
            ai_provider_id,
            ai_model_id,
            ai_api_key,
            ai_base_url,
            source_lang,
            target_lang,
            timeout_seconds,
            max_concurrency,
            max_requests_per_second,
        });

    let translator_status = ctx
        .translation_service
        .update_config(translate_config)
        .await;
    let target_set: HashSet<String> = targets.into_iter().filter(|t| !t.is_empty()).collect();
    let (translator, limiter) = ctx.translation_service.get_translator_and_limiter().await;

    {
        let mut config = ctx.sidebar_config.lock().await;
        config.translator = translator;
        config.limiter = limiter;
        config.target_set = target_set;
        config.image_capture = image_capture;
    }

    ctx.sidebar_enabled.store(true, Ordering::Relaxed);

    let app = ctx.manager.get_app_handle().await?;
    let state = ctx.manager.get_task_state();
    ctx.manager
        .publish_task_state_event("sidebar", true, &state, &translator_status)
        .await;
    update_tray_menu(&app, &state, &translator_status);
    app_state::emit_runtime_updated(&app, RuntimeService::new(ctx.manager.clone()));

    if translator_status.configured {
        ctx.manager
            .spawn_translator_health_check(translator_generation);
    }

    Ok(())
}

/// 关闭 sidebar 运行态，清空投影与译文依赖，避免浮窗停留在旧会话或旧翻译状态上。
pub(crate) async fn disable_sidebar(ctx: &SidebarRuntimeContext) -> Result<()> {
    ctx.manager.next_translator_generation();
    ctx.sidebar_enabled.store(false, Ordering::Relaxed);

    {
        let mut config = ctx.sidebar_config.lock().await;
        config.translator = None;
        config.limiter = None;
        config.target_set.clear();
        config.image_capture = false;
    }

    ctx.sidebar_runtime.clear();
    ctx.translation_service
        .set_status(TranslatorServiceStatus::disabled())
        .await;

    let app = ctx.manager.get_app_handle().await?;
    let state = ctx.manager.get_task_state();
    let translator_status = ctx.manager.get_translator_status().await;
    ctx.manager
        .publish_task_state_event("sidebar", false, &state, &translator_status)
        .await;
    update_tray_menu(&app, &state, &translator_status);
    app_state::emit_runtime_updated(&app, RuntimeService::new(ctx.manager.clone()));

    Ok(())
}
