//! 运行态状态同步服务：负责任务状态事件、翻译代际号和运行态快照回流，
//! 让 TaskManager 不再直接承载这类“状态广播/同步”逻辑。
use crate::app_state;
use crate::application::runtime::service::RuntimeService;
use crate::application::runtime::state::TaskState;
use crate::events::{EventStore, EventType};
use crate::infrastructure::tauri::tray_adapter::update_tray_menu;
use crate::task_manager::TaskManager;
use crate::translator::{TranslationService, TranslatorServiceStatus};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// RuntimeStatusContext 汇总状态同步所需依赖，
/// 让状态广播逻辑不必直接知道 TaskManager 的完整字段布局。
#[derive(Clone)]
pub(crate) struct RuntimeStatusContext {
    pub(crate) manager: TaskManager,
    pub(crate) adapter: Arc<crate::adapter::MacOSAdapter>,
    pub(crate) events: Arc<EventStore>,
    pub(crate) translation_service: Arc<TranslationService>,
    pub(crate) translator_generation: Arc<AtomicU64>,
}

/// 生成统一的运行时健康快照，供 listen health 和诊断面板查询当前运行状态。
pub(crate) async fn service_status(ctx: &RuntimeStatusContext) -> serde_json::Value {
    let state = ctx.manager.get_task_state();
    let translator_status = ctx.manager.get_translator_status().await;
    serde_json::json!({
        "adapter": {
            "platform": ctx.adapter.is_supported().then_some("macos").unwrap_or("unsupported"),
            "supported": ctx.adapter.is_supported(),
            "reason": ctx.adapter.support_reason(),
        },
        "tasks": state,
        "translator": translator_status.as_json(),
    })
}

/// 发布统一的任务状态事件，供日志页、调试页和未来的运行态观测链路使用。
pub(crate) async fn publish_task_state_event(
    ctx: &RuntimeStatusContext,
    task: &str,
    running: bool,
    state: &TaskState,
    translator_status: &TranslatorServiceStatus,
) {
    if let Ok(app) = ctx.manager.get_app_handle().await {
        ctx.events.publish(
            &app,
            EventType::TaskState,
            "task_manager",
            serde_json::json!({
                "task": task,
                "running": running,
                "state": state,
                "translator": translator_status.as_json(),
            }),
        );
    }
}

/// 推进翻译代际号，避免旧健康检查或旧译文写回覆盖新一轮配置状态。
pub(crate) fn next_translator_generation(ctx: &RuntimeStatusContext) -> u64 {
    ctx.translator_generation.fetch_add(1, Ordering::Relaxed) + 1
}

/// 返回当前翻译代际号，供异步写回在提交结果前判断自己是否仍然有效。
pub(crate) fn current_translator_generation(ctx: &RuntimeStatusContext) -> u64 {
    ctx.translator_generation.load(Ordering::Relaxed)
}

/// 触发后台翻译健康检查，并在结果回流时按代际号保护当前运行态。
pub(crate) fn spawn_translator_health_check(ctx: &RuntimeStatusContext, generation: u64) {
    let ctx = ctx.clone();
    tokio::spawn(async move {
        let config = ctx.translation_service.get_config().await;
        let provider = config.provider_name().to_string();
        let next_status = match ctx.translation_service.check_health().await {
            Ok(_) => TranslatorServiceStatus::healthy(&provider),
            Err(error) => TranslatorServiceStatus::error(&provider, error.to_string()),
        };
        set_translator_status_if_current(&ctx, generation, next_status).await;
    });
}

/// 仅当翻译代际号仍匹配时更新翻译状态，并同步任务事件、托盘和 runtime snapshot。
pub(crate) async fn set_translator_status_if_current(
    ctx: &RuntimeStatusContext,
    generation: u64,
    status: TranslatorServiceStatus,
) {
    if current_translator_generation(ctx) != generation {
        return;
    }

    ctx.translation_service.set_status(status.clone()).await;

    if current_translator_generation(ctx) != generation {
        return;
    }

    let task_state = ctx.manager.get_task_state();
    publish_task_state_event(ctx, "translator", status.enabled, &task_state, &status).await;

    if let Ok(app) = ctx.manager.get_app_handle().await {
        if current_translator_generation(ctx) != generation {
            return;
        }
        update_tray_menu(&app, &task_state, &status);
        app_state::emit_runtime_updated(&app, RuntimeService::new(ctx.manager.clone()));
    }
}
