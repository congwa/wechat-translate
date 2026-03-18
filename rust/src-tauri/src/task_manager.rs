use crate::adapter::MacOSAdapter;
use crate::app_state;
use crate::application::runtime::monitor_loop::{spawn_monitor_loop, MonitorLoopContext};
use crate::application::runtime::translator_runtime::spawn_sidebar_translation_update;
use crate::application::sidebar::projection_service::{emit_sidebar_invalidated, SidebarRuntime};
use crate::config::AppConfig;
use crate::db::MessageDb;
use crate::events::{EventStore, EventType};
use crate::image_cache::WeChatImageCache;
use crate::translator::{
    TranslateConfig, TranslateProviderConfig, TranslationLimiter, TranslationService,
    TranslatorServiceStatus,
};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};
use tokio::sync::{watch, Mutex};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskState {
    pub monitoring: bool,
    pub sidebar: bool,
}

pub(crate) struct SidebarConfig {
    pub(crate) translator: Option<Arc<dyn crate::translator::Translator>>,
    pub(crate) limiter: Option<Arc<TranslationLimiter>>,
    pub(crate) target_set: HashSet<String>,
    pub(crate) image_capture: bool,
}

/// 监听循环首次 poll 完成信号
/// 用于 live_start 等待监听就绪后再打开窗口
pub struct FirstPollSignal {
    tx: watch::Sender<Option<String>>,
    rx: watch::Receiver<Option<String>>,
}

impl FirstPollSignal {
    fn new() -> Self {
        let (tx, rx) = watch::channel(None);
        Self { tx, rx }
    }

    /// 监听循环调用：标记首次 poll 完成，并传递当前聊天名称
    pub(crate) fn signal_ready(&self, chat_name: &str) {
        let _ = self.tx.send(Some(chat_name.to_string()));
    }

    /// 重置信号（监听重启时调用）
    pub(crate) fn reset(&self) {
        let _ = self.tx.send(None);
    }

    /// 等待首次 poll 完成，返回当前聊天名称
    /// 带超时保护，避免无限等待
    pub async fn wait_ready(&self, timeout: Duration) -> Option<String> {
        let mut rx = self.rx.clone();
        tokio::select! {
            result = async {
                loop {
                    if let Some(chat_name) = rx.borrow().clone() {
                        return Some(chat_name);
                    }
                    if rx.changed().await.is_err() {
                        return None;
                    }
                }
            } => result,
            _ = tokio::time::sleep(timeout) => None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct MonitorConfig {
    pub(crate) use_right_panel_details: bool,
}

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

    pub async fn set_app_handle(&self, handle: AppHandle) {
        *self.app_handle.lock().await = Some(handle);
    }

    pub async fn set_use_right_panel_details(&self, enabled: bool) {
        self.monitor_config.lock().await.use_right_panel_details = enabled;
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

    async fn wait_for_monitor_stop(&self, timeout: Duration) -> Result<()> {
        let started_at = Instant::now();
        loop {
            if self.monitor_token.lock().await.is_none() {
                return Ok(());
            }

            if started_at.elapsed() >= timeout {
                anyhow::bail!("等待监听任务停止超时");
            }

            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    pub async fn stop_monitoring_and_wait(&self, timeout: Duration) -> Result<()> {
        self.stop_monitoring().await?;
        self.wait_for_monitor_stop(timeout).await
    }

    async fn restore_sidebar_after_monitor_restart(&self, chat_name: &str) {
        if chat_name.trim().is_empty() || !self.sidebar_enabled.load(Ordering::Relaxed) {
            return;
        }

        let refresh_version = self.sidebar_runtime.update_chat_and_version(chat_name);
        if let Ok(app) = self.get_app_handle().await {
            emit_sidebar_invalidated(&app, &self.events, chat_name, refresh_version);
        }
    }

    pub async fn restart_monitoring(
        &self,
        interval_seconds: f64,
        ready_timeout: Duration,
        recover_sidebar_runtime: bool,
    ) -> Result<Option<String>> {
        let was_running = self.monitor_token.lock().await.is_some();
        if was_running {
            self.stop_monitoring_and_wait(Duration::from_secs(3))
                .await?;
        }

        if recover_sidebar_runtime && self.sidebar_enabled.load(Ordering::Relaxed) {
            self.sidebar_runtime.clear();
        }

        self.start_monitoring(interval_seconds).await?;

        let first_chat = match self.wait_first_poll(ready_timeout).await {
            Some(chat_name) => Some(chat_name),
            None => {
                let _ = self.stop_monitoring_and_wait(Duration::from_secs(3)).await;
                anyhow::bail!("监听恢复失败：首次轮询超时");
            }
        };

        if recover_sidebar_runtime {
            if let Some(chat_name) = first_chat.as_deref() {
                self.restore_sidebar_after_monitor_restart(chat_name).await;
            }
        }

        Ok(first_chat)
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

    async fn publish_task_state_event(
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

    fn next_translator_generation(&self) -> u64 {
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

        // 根据 provider 类型构建对应的配置
        let provider_config = if config.translate.provider == "ai" {
            TranslateProviderConfig::Ai {
                provider_id: config.translate.ai_provider_id.clone(),
                model_id: config.translate.ai_model_id.clone(),
                api_key: config.translate.ai_api_key.clone(),
                base_url: if config.translate.ai_base_url.is_empty() {
                    None
                } else {
                    Some(config.translate.ai_base_url.clone())
                },
            }
        } else {
            TranslateProviderConfig::Deeplx {
                url: config.translate.deeplx_url.clone(),
            }
        };

        // 更新全局翻译服务配置
        let translate_config = TranslateConfig {
            enabled: config.translate.enabled,
            provider_config,
            source_lang: config.translate.source_lang.clone(),
            target_lang: config.translate.target_lang.clone(),
            timeout_seconds: config.translate.timeout_seconds,
            max_concurrency: config.translate.max_concurrency,
            max_requests_per_second: config.translate.max_requests_per_second,
        };

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

    pub(crate) async fn finalize_monitor_loop(&self, app_handle: &AppHandle) {
        let next_state = TaskState {
            monitoring: false,
            sidebar: self.sidebar_enabled.load(Ordering::Relaxed),
        };

        if !next_state.sidebar {
            self.next_translator_generation();
            self.translation_service
                .set_status(TranslatorServiceStatus::disabled())
                .await;
        }

        let translator_status = self.get_translator_status().await;
        self.publish_task_state_event("monitoring", false, &next_state, &translator_status)
            .await;
        update_tray_menu(app_handle, &next_state, &translator_status);
        app_state::emit_runtime_updated(app_handle, self);
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
        self.first_poll_signal.reset();

        let app = self.get_app_handle().await?;
        let state = self.get_task_state();
        let translator_status = self.get_translator_status().await;
        self.publish_task_state_event("monitoring", true, &state, &translator_status)
            .await;
        update_tray_menu(&app, &state, &translator_status);
        app_state::emit_runtime_updated(&app, self);

        let adapter = self.adapter.clone();
        let events = self.events.clone();
        let db = self.db.clone();
        let image_cache = self.image_cache.clone();
        let monitor_token_ref = self.monitor_token.clone();
        let monitoring_active = self.monitoring_active.clone();
        let monitor_config = self.monitor_config.clone();
        let first_poll_signal = self.first_poll_signal.clone();
        let sidebar_enabled = self.sidebar_enabled.clone();
        let sidebar_config = self.sidebar_config.clone();
        let sidebar_runtime = self.sidebar_runtime.clone();
        let app_handle = app.clone();
        let manager = self.clone();
        let poll_interval = interval_seconds.max(0.4);

        spawn_monitor_loop(MonitorLoopContext {
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
            manager,
            poll_interval,
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

        // 根据 provider 创建对应的配置
        let provider_config = match provider.as_str() {
            "ai" => TranslateProviderConfig::Ai {
                provider_id: ai_provider_id,
                model_id: ai_model_id,
                api_key: ai_api_key,
                base_url: if ai_base_url.is_empty() {
                    None
                } else {
                    Some(ai_base_url)
                },
            },
            _ => TranslateProviderConfig::Deeplx { url: deeplx_url },
        };

        let translate_config = TranslateConfig {
            enabled: translate_enabled,
            provider_config,
            source_lang,
            target_lang,
            timeout_seconds,
            max_concurrency,
            max_requests_per_second,
        };

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

    pub async fn stop_all_and_wait(&self, timeout: Duration) -> Result<()> {
        let _ = self.stop_monitoring().await;
        self.wait_for_monitor_stop(timeout).await?;
        self.disable_sidebar().await?;
        Ok(())
    }

    pub async fn stop_all(&self) {
        let _ = self.stop_all_and_wait(Duration::from_secs(3)).await;
    }
}

fn update_tray_menu(
    app: &AppHandle,
    state: &TaskState,
    translator_status: &TranslatorServiceStatus,
) {
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
            "● 监听运行中"
        } else {
            "○ 监听未运行"
        });
        let _ = tray.listen_toggle.set_text(if state.monitoring {
            "暂停监听"
        } else {
            "开启监听"
        });
        let _ = tray
            .translate_status
            .set_text(translator_status.menu_text());
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
