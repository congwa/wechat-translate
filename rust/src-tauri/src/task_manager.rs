use crate::adapter::MacOSAdapter;
use crate::app_state;
use crate::application::runtime::lifecycle::{self as runtime_lifecycle, RuntimeLifecycleContext};
use crate::application::runtime::state::{FirstPollSignal, MonitorConfig, SidebarConfig};
use crate::application::runtime::translation_config::{
    build_translate_config_from_app_config, build_translate_config_from_sidebar_params,
    SidebarTranslationRuntimeParams,
};
use crate::application::runtime::translator_runtime::spawn_sidebar_translation_update;
use crate::application::sidebar::projection_service::SidebarRuntime;
use crate::config::AppConfig;
use crate::db::MessageDb;
use crate::events::{EventStore, EventType};
use crate::image_cache::WeChatImageCache;
use crate::infrastructure::tauri::tray_adapter::update_tray_menu;
use crate::translator::{TranslationService, TranslatorServiceStatus};
use anyhow::Result;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::AppHandle;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

pub use crate::application::runtime::state::TaskState;

#[derive(Clone)]
pub struct TaskManager {
    adapter: Arc<MacOSAdapter>,
    events: Arc<EventStore>,
    db: Arc<MessageDb>,
    image_cache: Arc<std::sync::Mutex<WeChatImageCache>>,
    translation_service: Arc<TranslationService>,
    monitor_token: Arc<Mutex<Option<CancellationToken>>>,
    monitoring_active: Arc<AtomicBool>,
    monitor_config: Arc<Mutex<MonitorConfig>>,
    first_poll_signal: Arc<FirstPollSignal>,
    sidebar_enabled: Arc<AtomicBool>,
    sidebar_config: Arc<Mutex<SidebarConfig>>,
    sidebar_runtime: Arc<SidebarRuntime>,
    translator_generation: Arc<AtomicU64>,
    app_handle: Arc<Mutex<Option<AppHandle>>>,
}

impl TaskManager {
    pub fn new(
        adapter: Arc<MacOSAdapter>,
        events: Arc<EventStore>,
        db: Arc<MessageDb>,
        image_cache: Arc<std::sync::Mutex<WeChatImageCache>>,
        translation_service: Arc<TranslationService>,
    ) -> Self {
        Self {
            adapter,
            events,
            db,
            image_cache,
            translation_service,
            monitor_token: Arc::new(Mutex::new(None)),
            monitoring_active: Arc::new(AtomicBool::new(false)),
            monitor_config: Arc::new(Mutex::new(MonitorConfig {
                use_right_panel_details: false,
            })),
            first_poll_signal: Arc::new(FirstPollSignal::new()),
            sidebar_enabled: Arc::new(AtomicBool::new(false)),
            sidebar_config: Arc::new(Mutex::new(SidebarConfig {
                translator: None,
                limiter: None,
                target_set: HashSet::new(),
                image_capture: false,
            })),
            sidebar_runtime: Arc::new(SidebarRuntime::new()),
            translator_generation: Arc::new(AtomicU64::new(0)),
            app_handle: Arc::new(Mutex::new(None)),
        }
    }

    /// 记录主应用句柄，供运行时服务在监听循环、托盘和事件链路里统一发消息。
    pub async fn set_app_handle(&self, handle: AppHandle) {
        *self.app_handle.lock().await = Some(handle);
    }

    /// 更新监听是否读取右侧详情面板的策略，让运行中监听可以响应最新配置。
    pub async fn set_use_right_panel_details(&self, enabled: bool) {
        self.monitor_config.lock().await.use_right_panel_details = enabled;
    }

    /// 读取当前应用句柄；若应用尚未完成 bootstrap，则返回错误避免生命周期误触发。
    pub(crate) async fn get_app_handle(&self) -> Result<AppHandle> {
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

    pub async fn get_translator_status(&self) -> TranslatorServiceStatus {
        self.translation_service.get_status().await
    }

    pub fn get_sidebar_runtime(&self) -> &Arc<SidebarRuntime> {
        &self.sidebar_runtime
    }

    /// 等待监听循环首次 poll 完成
    /// 返回当前聊天名称，超时返回 None
    pub async fn wait_first_poll(&self, timeout: Duration) -> Option<String> {
        self.first_poll_signal.wait_ready(timeout).await
    }

    /// 汇总监听生命周期依赖，供独立 runtime 生命周期模块执行启停/恢复编排。
    pub(crate) fn lifecycle_context(&self) -> RuntimeLifecycleContext {
        RuntimeLifecycleContext {
            manager: self.clone(),
            adapter: self.adapter.clone(),
            events: self.events.clone(),
            db: self.db.clone(),
            image_cache: self.image_cache.clone(),
            translation_service: self.translation_service.clone(),
            monitor_token: self.monitor_token.clone(),
            monitoring_active: self.monitoring_active.clone(),
            monitor_config: self.monitor_config.clone(),
            first_poll_signal: self.first_poll_signal.clone(),
            sidebar_enabled: self.sidebar_enabled.clone(),
            sidebar_config: self.sidebar_config.clone(),
            sidebar_runtime: self.sidebar_runtime.clone(),
        }
    }

    /// 供恢复链路使用：先停止旧监听，再等待后台任务真正退出。
    pub async fn stop_monitoring_and_wait(&self, timeout: Duration) -> Result<()> {
        runtime_lifecycle::stop_monitoring_and_wait(&self.lifecycle_context(), timeout).await
    }

    /// 在辅助功能恢复后，按等待式生命周期重建监听和 sidebar 基线。
    pub async fn restart_monitoring(
        &self,
        interval_seconds: f64,
        ready_timeout: Duration,
        recover_sidebar_runtime: bool,
    ) -> Result<Option<String>> {
        runtime_lifecycle::restart_monitoring(
            &self.lifecycle_context(),
            interval_seconds,
            ready_timeout,
            recover_sidebar_runtime,
        )
        .await
    }

    /// 获取翻译服务是否可用
    pub async fn is_translator_available(&self) -> bool {
        self.translation_service.is_available().await
    }

    /// 获取翻译服务
    pub fn get_translation_service(&self) -> Arc<TranslationService> {
        self.translation_service.clone()
    }

    /// 手动翻译消息（用于点击翻译按钮）
    pub async fn translate_message_manually(
        &self,
        app: tauri::AppHandle,
        message_id: u64,
        chat_name: String,
        sender: String,
        content: String,
        detected_at: String,
    ) -> Result<(), String> {
        log::info!("[TaskManager] translate_message_manually 开始");
        let config = self.translation_service.get_config().await;
        log::info!("[TaskManager] 翻译配置: enabled={}", config.enabled);
        if !config.enabled {
            return Err("翻译服务未启用".to_string());
        }

        let (translator, limiter) = self.translation_service.get_translator_and_limiter().await;
        log::info!(
            "[TaskManager] translator={}, limiter={}",
            translator.is_some(),
            limiter.is_some()
        );
        let translator = translator.ok_or_else(|| "翻译服务未配置".to_string())?;
        let limiter = limiter.ok_or_else(|| "翻译限流器未配置".to_string())?;

        let source_lang = config.source_lang.clone();
        let target_lang = config.target_lang.clone();

        log::info!(
            "[TaskManager] 准备启动翻译任务: {}→{}",
            source_lang,
            target_lang
        );
        // 复用已有的翻译更新逻辑
        spawn_sidebar_translation_update(
            self.clone(),
            self.events.clone(),
            app,
            self.db.clone(),
            translator,
            limiter,
            source_lang,
            target_lang,
            message_id,
            chat_name,
            sender,
            content,
            detected_at,
        );

        log::info!("[TaskManager] translate_message_manually 完成");
        Ok(())
    }

    pub async fn service_status(&self) -> serde_json::Value {
        let state = self.get_task_state();
        let translator_status = self.get_translator_status().await;
        serde_json::json!({
            "adapter": {
                "platform": self.adapter.is_supported().then_some("macos").unwrap_or("unsupported"),
                "supported": self.adapter.is_supported(),
                "reason": self.adapter.support_reason(),
            },
            "tasks": state,
            "translator": translator_status.as_json(),
        })
    }

    /// 发布统一的任务状态事件，供日志页和调试面板观察生命周期变化。
    pub(crate) async fn publish_task_state_event(
        &self,
        task: &str,
        running: bool,
        state: &TaskState,
        translator_status: &TranslatorServiceStatus,
    ) {
        if let Ok(app) = self.get_app_handle().await {
            self.events.publish(
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

    /// 推进翻译代际号，避免旧一轮翻译健康检查或写回覆盖新配置状态。
    pub(crate) fn next_translator_generation(&self) -> u64 {
        self.translator_generation.fetch_add(1, Ordering::Relaxed) + 1
    }

    pub(crate) fn current_translator_generation(&self) -> u64 {
        self.translator_generation.load(Ordering::Relaxed)
    }

    fn spawn_translator_health_check(&self, translator_generation: u64) {
        let manager = self.clone();
        tokio::spawn(async move {
            let config = manager.translation_service.get_config().await;
            let provider = config.provider_name().to_string();
            let next_status = match manager.translation_service.check_health().await {
                Ok(_) => TranslatorServiceStatus::healthy(&provider),
                Err(error) => TranslatorServiceStatus::error(&provider, error.to_string()),
            };
            manager
                .set_translator_status_if_current(translator_generation, next_status)
                .await;
        });
    }

    pub async fn apply_runtime_config(&self, config: &AppConfig) {
        self.set_use_right_panel_details(config.listen.use_right_panel_details)
            .await;

        let translator_generation = self.next_translator_generation();

        let translate_config = build_translate_config_from_app_config(config);

        let translator_status = self
            .translation_service
            .update_config(translate_config)
            .await;

        // 更新侧边栏配置（使用全局翻译服务的 translator 和 limiter）
        if self.sidebar_enabled.load(Ordering::Relaxed) {
            let (translator, limiter) = self.translation_service.get_translator_and_limiter().await;
            let mut sidebar_config = self.sidebar_config.lock().await;
            sidebar_config.translator = translator;
            sidebar_config.limiter = limiter;
        }

        if let Ok(app) = self.get_app_handle().await {
            let state = self.get_task_state();
            update_tray_menu(&app, &state, &translator_status);
            app_state::emit_runtime_updated(&app, self);
        }

        // 如果翻译服务已配置，启动健康检查
        if translator_status.configured {
            self.spawn_translator_health_check(translator_generation);
        }
    }

    pub(crate) async fn set_translator_status_if_current(
        &self,
        generation: u64,
        status: TranslatorServiceStatus,
    ) {
        if self.translator_generation.load(Ordering::Relaxed) != generation {
            return;
        }

        self.translation_service.set_status(status.clone()).await;

        if self.translator_generation.load(Ordering::Relaxed) != generation {
            return;
        }

        let task_state = self.get_task_state();
        self.publish_task_state_event("translator", status.enabled, &task_state, &status)
            .await;

        if let Ok(app) = self.get_app_handle().await {
            if self.translator_generation.load(Ordering::Relaxed) != generation {
                return;
            }
            update_tray_menu(&app, &task_state, &status);
            app_state::emit_runtime_updated(&app, self);
        }
    }

    /// 在监听循环退出后统一回收运行态并刷新 snapshot，避免 UI 长时间停留在“监听中”。
    pub(crate) async fn finalize_monitor_loop(&self, app_handle: &AppHandle) {
        runtime_lifecycle::finalize_monitor_loop(&self.lifecycle_context(), app_handle).await;
    }

    /// 启动监听主循环，并通过生命周期服务同步任务状态、托盘和 runtime snapshot。
    pub async fn start_monitoring(&self, interval_seconds: f64) -> Result<()> {
        runtime_lifecycle::start_monitoring(&self.lifecycle_context(), interval_seconds).await
    }

    /// 发出监听取消信号；真正的退出等待由等待式 API 负责收尾。
    pub async fn stop_monitoring(&self) -> Result<()> {
        runtime_lifecycle::stop_monitoring(&self.lifecycle_context()).await
    }

    pub async fn enable_sidebar(
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
        let translator_generation = self.next_translator_generation();

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

        let translator_status = self
            .translation_service
            .update_config(translate_config)
            .await;

        let target_set: HashSet<String> = targets.into_iter().filter(|t| !t.is_empty()).collect();

        // 从 TranslationService 获取内部状态用于 sidebar
        let (translator, limiter) = self.translation_service.get_translator_and_limiter().await;

        {
            let mut config = self.sidebar_config.lock().await;
            config.translator = translator;
            config.limiter = limiter;
            config.target_set = target_set;
            config.image_capture = image_capture;
        }

        self.sidebar_enabled.store(true, Ordering::Relaxed);

        let app = self.get_app_handle().await?;
        let state = self.get_task_state();
        self.publish_task_state_event("sidebar", true, &state, &translator_status)
            .await;
        update_tray_menu(&app, &state, &translator_status);
        app_state::emit_runtime_updated(&app, self);

        // 如果翻译服务已配置，启动健康检查
        if translator_status.configured {
            self.spawn_translator_health_check(translator_generation);
        }

        Ok(())
    }

    pub async fn disable_sidebar(&self) -> Result<()> {
        self.next_translator_generation();
        self.sidebar_enabled.store(false, Ordering::Relaxed);

        {
            let mut config = self.sidebar_config.lock().await;
            config.translator = None;
            config.limiter = None;
            config.target_set.clear();
            config.image_capture = false;
        }

        self.sidebar_runtime.clear();

        self.translation_service
            .set_status(TranslatorServiceStatus::disabled())
            .await;

        let app = self.get_app_handle().await?;
        let state = self.get_task_state();
        let translator_status = self.get_translator_status().await;
        self.publish_task_state_event("sidebar", false, &state, &translator_status)
            .await;
        update_tray_menu(&app, &state, &translator_status);
        app_state::emit_runtime_updated(&app, self);

        Ok(())
    }

    /// 关闭所有运行态前先等待监听退出，确保清库或恢复场景不会残留后台任务。
    pub async fn stop_all_and_wait(&self, timeout: Duration) -> Result<()> {
        runtime_lifecycle::stop_all_and_wait(&self.lifecycle_context(), timeout).await
    }

    pub async fn stop_all(&self) {
        let _ = self.stop_all_and_wait(Duration::from_secs(3)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::{MacOSAdapter, TaskManager, TranslatorServiceStatus};
    use crate::runtime_monitor_ingest::{
        short_error_text, should_forward_session_preview, should_forward_sidebar_chat,
    };
    use crate::{db::MessageDb, events::EventStore, translator::TranslationService};
    use std::{path::PathBuf, sync::Arc, time::Duration};
    use tokio_util::sync::CancellationToken;

    #[test]
    fn translator_status_menu_text_matches_state() {
        assert_eq!(
            TranslatorServiceStatus::disabled().menu_text(),
            "○ 翻译未启用"
        );
        assert_eq!(
            TranslatorServiceStatus::unconfigured("deeplx").menu_text(),
            "○ 翻译未配置"
        );
        assert_eq!(
            TranslatorServiceStatus::checking("deeplx").menu_text(),
            "◐ 翻译检测中"
        );
        assert_eq!(
            TranslatorServiceStatus::healthy("deeplx").menu_text(),
            "● 翻译服务可用"
        );
        assert_eq!(
            TranslatorServiceStatus::error("deeplx", "boom").menu_text(),
            "⚠ 翻译服务异常"
        );
    }

    #[test]
    fn translator_status_json_contains_expected_fields() {
        let status = TranslatorServiceStatus::error("deeplx", "request failed");
        let json = status.as_json();

        assert_eq!(json["enabled"], true);
        assert_eq!(json["configured"], true);
        assert_eq!(json["checking"], false);
        assert_eq!(json["healthy"], false);
        assert_eq!(json["last_error"], "request failed");
    }

    #[test]
    fn short_error_text_truncates_long_messages() {
        let message = "x".repeat(140);
        let shortened = short_error_text(&message);

        assert_eq!(shortened.len(), 123);
        assert!(shortened.ends_with("..."));
    }

    #[test]
    fn session_preview_forwarding_skips_active_chat_when_right_panel_enabled() {
        assert!(should_forward_session_preview(false, "项目群", "项目群"));
        assert!(should_forward_session_preview(true, "另一个群", "项目群"));
        assert!(!should_forward_session_preview(true, "项目群", "项目群"));
    }

    #[test]
    fn sidebar_forwarding_requires_non_empty_chat_name() {
        assert!(should_forward_sidebar_chat("项目群"));
        assert!(should_forward_sidebar_chat("另一个群"));
        assert!(!should_forward_sidebar_chat(""));
    }

    fn temp_db_path(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        std::env::temp_dir().join(format!("wechat_pc_auto_task_manager_{tag}_{nanos}.sqlite3"))
    }

    fn build_test_manager() -> TaskManager {
        let db_path = temp_db_path("wait_for_stop");
        let db = Arc::new(MessageDb::new(&db_path).expect("create db"));
        TaskManager::new(
            Arc::new(MacOSAdapter::new()),
            Arc::new(EventStore::new()),
            db,
            Arc::new(std::sync::Mutex::new(
                crate::image_cache::WeChatImageCache::new(),
            )),
            Arc::new(TranslationService::new()),
        )
    }

    #[tokio::test]
    async fn stop_monitoring_and_wait_waits_until_monitor_token_is_cleared() {
        let manager = build_test_manager();
        let token = CancellationToken::new();
        *manager.monitor_token.lock().await = Some(token.clone());

        let monitor_token = manager.monitor_token.clone();
        tokio::spawn(async move {
            token.cancel();
            tokio::time::sleep(Duration::from_millis(80)).await;
            *monitor_token.lock().await = None;
        });

        manager
            .stop_monitoring_and_wait(Duration::from_secs(1))
            .await
            .expect("wait for monitor stop");

        assert!(manager.monitor_token.lock().await.is_none());
    }

    #[tokio::test]
    async fn stop_monitoring_and_wait_times_out_when_monitor_token_stays_alive() {
        let manager = build_test_manager();
        *manager.monitor_token.lock().await = Some(CancellationToken::new());

        let error = manager
            .stop_monitoring_and_wait(Duration::from_millis(120))
            .await
            .expect_err("should time out");

        assert!(error.to_string().contains("等待监听任务停止超时"));
    }
}
