//! 运行时生命周期服务：负责监听任务的启动、停止、恢复和循环收尾，
//! 让 TaskManager 退回为聚合入口，而不是继续承载完整生命周期编排细节。
use crate::app_state;
use crate::application::runtime::monitor_loop::{spawn_monitor_loop, MonitorLoopContext};
use crate::application::runtime::state::{
    FirstPollSignal, MonitorConfig, SidebarConfig, TaskState,
};
use crate::application::sidebar::projection_service::SidebarRuntime;
use crate::db::MessageDb;
use crate::events::EventStore;
use crate::image_cache::WeChatImageCache;
use crate::infrastructure::tauri::tray_adapter::update_tray_menu;
use crate::task_manager::TaskManager;
use crate::translator::{TranslationService, TranslatorServiceStatus};
use anyhow::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::AppHandle;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

/// RuntimeLifecycleContext 汇总监听生命周期所需依赖，
/// 让生命周期服务只关心编排，不直接知道 TaskManager 的字段布局。
#[derive(Clone)]
pub(crate) struct RuntimeLifecycleContext {
    pub(crate) manager: TaskManager,
    pub(crate) adapter: Arc<crate::adapter::MacOSAdapter>,
    pub(crate) events: Arc<EventStore>,
    pub(crate) db: Arc<MessageDb>,
    pub(crate) image_cache: Arc<std::sync::Mutex<WeChatImageCache>>,
    pub(crate) translation_service: Arc<TranslationService>,
    pub(crate) monitor_token: Arc<Mutex<Option<CancellationToken>>>,
    pub(crate) monitoring_active: Arc<AtomicBool>,
    pub(crate) monitor_config: Arc<Mutex<MonitorConfig>>,
    pub(crate) first_poll_signal: Arc<FirstPollSignal>,
    pub(crate) sidebar_enabled: Arc<AtomicBool>,
    pub(crate) sidebar_config: Arc<Mutex<SidebarConfig>>,
    pub(crate) sidebar_runtime: Arc<SidebarRuntime>,
}

/// 等待监听任务真正退出，解决 cancel 发出后 token 仍未清空时的竞态窗口。
pub(crate) async fn wait_for_monitor_stop(
    ctx: &RuntimeLifecycleContext,
    timeout: Duration,
) -> Result<()> {
    let started_at = Instant::now();
    loop {
        if ctx.monitor_token.lock().await.is_none() {
            return Ok(());
        }

        if started_at.elapsed() >= timeout {
            anyhow::bail!("等待监听任务停止超时");
        }

        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// 先发出停止请求，再等待旧监听任务完全退出，保证后续重建基于干净运行态。
pub(crate) async fn stop_monitoring_and_wait(
    ctx: &RuntimeLifecycleContext,
    timeout: Duration,
) -> Result<()> {
    stop_monitoring(ctx).await?;
    wait_for_monitor_stop(ctx, timeout).await
}

/// 在监听重建成功后恢复 sidebar 当前聊天投影，确保浮窗重新指向真实活跃会话。
pub(crate) async fn restore_sidebar_after_monitor_restart(
    ctx: &RuntimeLifecycleContext,
    chat_name: &str,
) {
    if chat_name.trim().is_empty() || !ctx.sidebar_enabled.load(Ordering::Relaxed) {
        return;
    }

    let refresh_version = ctx.sidebar_runtime.update_chat_and_version(chat_name);
    if let Ok(app) = ctx.manager.get_app_handle().await {
        crate::application::sidebar::projection_service::emit_sidebar_invalidated(
            &app,
            &ctx.events,
            chat_name,
            refresh_version,
        );
    }
}

/// 以“停止旧任务 -> 清理必要基线 -> 启动新任务 -> 等待首次 poll”顺序重建监听运行态。
pub(crate) async fn restart_monitoring(
    ctx: &RuntimeLifecycleContext,
    interval_seconds: f64,
    ready_timeout: Duration,
    recover_sidebar_runtime: bool,
) -> Result<Option<String>> {
    let was_running = ctx.monitor_token.lock().await.is_some();
    if was_running {
        stop_monitoring_and_wait(ctx, Duration::from_secs(3)).await?;
    }

    if recover_sidebar_runtime && ctx.sidebar_enabled.load(Ordering::Relaxed) {
        ctx.sidebar_runtime.clear();
    }

    start_monitoring(ctx, interval_seconds).await?;

    let first_chat = match ctx.manager.wait_first_poll(ready_timeout).await {
        Some(chat_name) => Some(chat_name),
        None => {
            let _ = stop_monitoring_and_wait(ctx, Duration::from_secs(3)).await;
            anyhow::bail!("监听恢复失败：首次轮询超时");
        }
    };

    if recover_sidebar_runtime {
        if let Some(chat_name) = first_chat.as_deref() {
            restore_sidebar_after_monitor_restart(ctx, chat_name).await;
        }
    }

    Ok(first_chat)
}

/// 在监听循环结束时收尾运行态、托盘状态和 runtime snapshot，保证前端能感知真实停止结果。
pub(crate) async fn finalize_monitor_loop(ctx: &RuntimeLifecycleContext, app_handle: &AppHandle) {
    let next_state = TaskState {
        monitoring: false,
        sidebar: ctx.sidebar_enabled.load(Ordering::Relaxed),
    };

    if !next_state.sidebar {
        ctx.manager.next_translator_generation();
        ctx.translation_service
            .set_status(TranslatorServiceStatus::disabled())
            .await;
    }

    let translator_status = ctx.manager.get_translator_status().await;
    ctx.manager
        .publish_task_state_event("monitoring", false, &next_state, &translator_status)
        .await;
    update_tray_menu(app_handle, &next_state, &translator_status);
    app_state::emit_runtime_updated(app_handle, &ctx.manager);
}

/// 启动监听主循环并发布运行态变更，确保托盘和前端在同一轮里看到“监听已开始”。
pub(crate) async fn start_monitoring(
    ctx: &RuntimeLifecycleContext,
    interval_seconds: f64,
) -> Result<()> {
    {
        let existing = ctx.monitor_token.lock().await;
        if existing.is_some() {
            anyhow::bail!("监听已在运行中");
        }
    }

    let token = CancellationToken::new();
    *ctx.monitor_token.lock().await = Some(token.clone());
    ctx.monitoring_active.store(true, Ordering::Relaxed);
    ctx.first_poll_signal.reset();

    let app = ctx.manager.get_app_handle().await?;
    let state = ctx.manager.get_task_state();
    let translator_status = ctx.manager.get_translator_status().await;
    ctx.manager
        .publish_task_state_event("monitoring", true, &state, &translator_status)
        .await;
    update_tray_menu(&app, &state, &translator_status);
    app_state::emit_runtime_updated(&app, &ctx.manager);

    let poll_interval = interval_seconds.max(0.4);
    spawn_monitor_loop(MonitorLoopContext {
        token,
        adapter: ctx.adapter.clone(),
        events: ctx.events.clone(),
        db: ctx.db.clone(),
        image_cache: ctx.image_cache.clone(),
        monitor_token_ref: ctx.monitor_token.clone(),
        monitoring_active: ctx.monitoring_active.clone(),
        monitor_config: ctx.monitor_config.clone(),
        first_poll_signal: ctx.first_poll_signal.clone(),
        sidebar_enabled: ctx.sidebar_enabled.clone(),
        sidebar_config: ctx.sidebar_config.clone(),
        sidebar_runtime: ctx.sidebar_runtime.clone(),
        app_handle: app.clone(),
        manager: ctx.manager.clone(),
        poll_interval,
    });

    Ok(())
}

/// 发出监听取消信号；真正的退出确认由 stop_monitoring_and_wait 负责兜底等待。
pub(crate) async fn stop_monitoring(ctx: &RuntimeLifecycleContext) -> Result<()> {
    let token = ctx.monitor_token.lock().await.clone();
    if let Some(token) = token {
        token.cancel();
    }
    Ok(())
}

/// 在清库或退出场景下，先停监听并等待，再关闭 sidebar，保证运行态回到干净基线。
pub(crate) async fn stop_all_and_wait(
    ctx: &RuntimeLifecycleContext,
    timeout: Duration,
) -> Result<()> {
    let _ = stop_monitoring(ctx).await;
    wait_for_monitor_stop(ctx, timeout).await?;
    ctx.manager.disable_sidebar().await?;
    Ok(())
}
