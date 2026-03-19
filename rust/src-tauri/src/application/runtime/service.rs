//! 运行时应用服务：为 command/query 层提供稳定的运行态访问入口，
//! 避免上层继续直接依赖 TaskManager 的内部组织方式。
use crate::application::runtime::lifecycle as runtime_lifecycle;
use crate::application::runtime::read_service as runtime_read;
use crate::application::runtime::sidebar_runtime as app_sidebar_runtime;
use crate::application::runtime::state::TaskState;
use crate::application::runtime::status_sync as runtime_status_sync;
use crate::application::runtime::translator_runtime as app_translator_runtime;
use crate::config::AppConfig;
use crate::task_manager::TaskManager;
use crate::translator::{TranslationService, TranslatorServiceStatus};
use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;

/// RuntimeService 是当前迁移阶段的应用层 facade。
/// 它暂时委托给 TaskManager 执行业务，但把上层依赖固定到 application 路径。
#[derive(Clone)]
pub(crate) struct RuntimeService {
    manager: TaskManager,
}

impl RuntimeService {
    /// 基于当前运行时管理器构建应用服务，供 command/query 层按需创建短生命周期 facade。
    pub(crate) fn new(manager: TaskManager) -> Self {
        Self { manager }
    }

    /// 同步监听配置到运行时，确保保存配置后后台行为与配置文件保持一致。
    pub(crate) async fn apply_runtime_config(&self, config: &AppConfig) {
        self.manager
            .set_use_right_panel_details(config.listen.use_right_panel_details)
            .await;
        app_translator_runtime::apply_runtime_config(
            &self.manager.translator_runtime_context(),
            config,
        )
        .await;
    }

    /// 更新监听采集策略，保证监听循环按最新 UI 细节模式读取微信内容。
    pub(crate) async fn set_use_right_panel_details(&self, enabled: bool) {
        self.manager.set_use_right_panel_details(enabled).await;
    }

    /// 返回当前运行时任务快照，供上层判断监听与 sidebar 是否处于运行中。
    pub(crate) fn task_state(&self) -> TaskState {
        runtime_read::task_state(&self.manager.read_context())
    }

    /// 暴露 sidebar 投影运行态，供查询层基于当前聊天构建 sidebar 快照。
    pub(crate) fn sidebar_runtime(
        &self,
    ) -> Arc<crate::application::sidebar::projection_service::SidebarRuntime> {
        runtime_read::sidebar_runtime(&self.manager.read_context())
    }

    /// 启动监听主循环。
    pub(crate) async fn start_monitoring(&self, interval_seconds: f64) -> Result<()> {
        runtime_lifecycle::start_monitoring(&self.manager.lifecycle_context(), interval_seconds)
            .await
    }

    /// 停止监听主循环。
    pub(crate) async fn stop_monitoring(&self) -> Result<()> {
        runtime_lifecycle::stop_monitoring(&self.manager.lifecycle_context()).await
    }

    /// 重建监听运行态，并在需要时恢复 sidebar 当前聊天投影。
    pub(crate) async fn restart_monitoring(
        &self,
        interval_seconds: f64,
        ready_timeout: Duration,
        recover_sidebar_runtime: bool,
    ) -> Result<Option<String>> {
        runtime_lifecycle::restart_monitoring(
            &self.manager.lifecycle_context(),
            interval_seconds,
            ready_timeout,
            recover_sidebar_runtime,
        )
        .await
    }

    /// 等待监听首次成功轮询，供 live start 与授权恢复链路判断监听是否真正可用。
    pub(crate) async fn wait_first_poll(&self, timeout: Duration) -> Option<String> {
        runtime_read::wait_first_poll(&self.manager.read_context(), timeout).await
    }

    /// 启用 sidebar 及其译文能力。
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn enable_sidebar(
        &self,
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
        app_sidebar_runtime::enable_sidebar(
            &self.manager.sidebar_runtime_context(),
            targets,
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
            image_capture,
        )
        .await
    }

    /// 关闭 sidebar 相关运行态。
    pub(crate) async fn disable_sidebar(&self) -> Result<()> {
        app_sidebar_runtime::disable_sidebar(&self.manager.sidebar_runtime_context()).await
    }

    /// 返回运行时健康快照，供 listen health 接口与设置页诊断区展示。
    pub(crate) async fn service_status(&self) -> serde_json::Value {
        runtime_status_sync::service_status(&self.manager.status_context()).await
    }

    /// 返回翻译服务健康状态，供 runtime 快照与诊断展示使用。
    pub(crate) async fn translator_status(&self) -> TranslatorServiceStatus {
        runtime_read::translator_status(&self.manager.read_context()).await
    }

    /// 返回翻译服务引用，供字典等只读能力复用统一的翻译配置。
    pub(crate) fn translation_service(&self) -> Arc<TranslationService> {
        runtime_read::translation_service(&self.manager.read_context())
    }

    /// 供 sidebar 手动翻译入口使用：提交一次消息翻译并触发后续刷新。
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn translate_message_manually(
        &self,
        app: tauri::AppHandle,
        message_id: u64,
        chat_name: String,
        sender: String,
        content: String,
        detected_at: String,
    ) -> Result<(), String> {
        app_translator_runtime::translate_message_manually(
            &self.manager.translator_runtime_context(),
            app,
            message_id,
            chat_name,
            sender,
            content,
            detected_at,
        )
        .await
    }

    /// 清空数据后重启监听时，统一通过等待式关闭保证运行态先回到干净基线。
    pub(crate) async fn stop_all_and_wait(&self, timeout: Duration) -> Result<()> {
        runtime_lifecycle::stop_all_and_wait(&self.manager.lifecycle_context(), timeout).await
    }
}
