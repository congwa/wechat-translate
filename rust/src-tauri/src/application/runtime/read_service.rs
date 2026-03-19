//! 运行态读取服务：统一暴露任务状态、翻译状态、首次轮询信号和 sidebar 投影读取，
//! 避免上层继续通过 TaskManager 的零散 getter 直接读取内部字段。
use crate::application::runtime::state::{FirstPollSignal, TaskState};
use crate::application::sidebar::projection_service::SidebarRuntime;
use crate::translator::{TranslationService, TranslatorServiceStatus};
use anyhow::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::AppHandle;
use tokio::sync::Mutex;

/// RuntimeReadContext 汇总运行态“只读访问”所需依赖，
/// 让应用层读取逻辑不再散落到 TaskManager 的多个 getter 中。
#[derive(Clone)]
pub(crate) struct RuntimeReadContext {
    pub(crate) monitoring_active: Arc<AtomicBool>,
    pub(crate) sidebar_enabled: Arc<AtomicBool>,
    pub(crate) translation_service: Arc<TranslationService>,
    pub(crate) first_poll_signal: Arc<FirstPollSignal>,
    pub(crate) sidebar_runtime: Arc<SidebarRuntime>,
    pub(crate) app_handle: Arc<Mutex<Option<AppHandle>>>,
}

/// 返回当前监听/浮窗任务快照，作为运行态查询和事件同步的基础读模型。
pub(crate) fn task_state(ctx: &RuntimeReadContext) -> TaskState {
    TaskState {
        monitoring: ctx.monitoring_active.load(Ordering::Relaxed),
        sidebar: ctx.sidebar_enabled.load(Ordering::Relaxed),
    }
}

/// 返回当前翻译服务状态，供运行态快照、诊断和 sidebar 展示使用。
pub(crate) async fn translator_status(ctx: &RuntimeReadContext) -> TranslatorServiceStatus {
    ctx.translation_service.get_status().await
}

/// 返回全局翻译服务引用，供字典查询和监听循环读取当前语言配置。
pub(crate) fn translation_service(ctx: &RuntimeReadContext) -> Arc<TranslationService> {
    ctx.translation_service.clone()
}

/// 返回 sidebar 当前读模型投影，供浮窗快照和译文写回刷新版本号。
pub(crate) fn sidebar_runtime(ctx: &RuntimeReadContext) -> Arc<SidebarRuntime> {
    ctx.sidebar_runtime.clone()
}

/// 等待监听循环首次成功 poll，供 live start 和授权恢复确认监听是否真正就绪。
pub(crate) async fn wait_first_poll(ctx: &RuntimeReadContext, timeout: Duration) -> Option<String> {
    ctx.first_poll_signal.wait_ready(timeout).await
}

/// 返回当前应用句柄；若应用还未完成 bootstrap，则显式报错避免误发事件。
pub(crate) async fn app_handle(ctx: &RuntimeReadContext) -> Result<AppHandle> {
    ctx.app_handle
        .lock()
        .await
        .clone()
        .ok_or_else(|| anyhow::anyhow!("AppHandle not set"))
}
