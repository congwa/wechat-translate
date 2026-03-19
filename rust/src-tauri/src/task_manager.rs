use crate::adapter::MacOSAdapter;
use crate::application::runtime::lifecycle::RuntimeLifecycleContext;
use crate::application::runtime::read_service::RuntimeReadContext;
use crate::application::runtime::sidebar_runtime::SidebarRuntimeContext;
use crate::application::runtime::state::{FirstPollSignal, MonitorConfig, SidebarConfig};
use crate::application::runtime::status_sync::{self as runtime_status_sync, RuntimeStatusContext};
use crate::application::runtime::translator_runtime::TranslatorRuntimeContext;
use crate::application::sidebar::projection_service::SidebarRuntime;
use crate::db::MessageDb;
use crate::events::EventStore;
use crate::image_cache::WeChatImageCache;
use crate::translator::{TranslationService, TranslatorServiceStatus};
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::Arc;
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

    /// 汇总运行态只读依赖，供 application service 统一读取任务/翻译/sidebar 当前状态。
    pub(crate) fn read_context(&self) -> RuntimeReadContext {
        RuntimeReadContext {
            monitoring_active: self.monitoring_active.clone(),
            sidebar_enabled: self.sidebar_enabled.clone(),
            translation_service: self.translation_service.clone(),
            first_poll_signal: self.first_poll_signal.clone(),
            sidebar_runtime: self.sidebar_runtime.clone(),
            app_handle: self.app_handle.clone(),
        }
    }

    /// 汇总翻译运行态依赖，供翻译配置应用、健康探测和手动翻译链路复用。
    pub(crate) fn translator_runtime_context(&self) -> TranslatorRuntimeContext {
        TranslatorRuntimeContext {
            manager: self.clone(),
            events: self.events.clone(),
            db: self.db.clone(),
            translation_service: self.translation_service.clone(),
            sidebar_enabled: self.sidebar_enabled.clone(),
            sidebar_config: self.sidebar_config.clone(),
            sidebar_runtime: self.sidebar_runtime.clone(),
            status: self.status_context(),
        }
    }

    /// 汇总 sidebar 生命周期依赖，供 sidebar 启停和投影清理逻辑复用。
    pub(crate) fn sidebar_runtime_context(&self) -> SidebarRuntimeContext {
        SidebarRuntimeContext {
            manager: self.clone(),
            translation_service: self.translation_service.clone(),
            sidebar_enabled: self.sidebar_enabled.clone(),
            sidebar_config: self.sidebar_config.clone(),
            sidebar_runtime: self.sidebar_runtime.clone(),
        }
    }

    /// 汇总运行态状态同步依赖，供任务状态事件、翻译代际号和 health snapshot 复用。
    pub(crate) fn status_context(&self) -> RuntimeStatusContext {
        RuntimeStatusContext {
            manager: self.clone(),
            adapter: self.adapter.clone(),
            events: self.events.clone(),
            translation_service: self.translation_service.clone(),
            translator_generation: self.translator_generation.clone(),
        }
    }

    /// 发布统一的任务状态事件，供日志页和调试面板观察生命周期变化。
    pub(crate) async fn publish_task_state_event(
        &self,
        task: &str,
        running: bool,
        state: &TaskState,
        translator_status: &TranslatorServiceStatus,
    ) {
        runtime_status_sync::publish_task_state_event(
            &self.status_context(),
            task,
            running,
            state,
            translator_status,
        )
        .await;
    }

    /// 推进翻译代际号，避免旧一轮翻译健康检查或写回覆盖新配置状态。
    pub(crate) fn next_translator_generation(&self) -> u64 {
        runtime_status_sync::next_translator_generation(&self.status_context())
    }

    /// 启动一轮后台翻译健康探测，并在结果回流时按代际号保护当前运行态。
    pub(crate) fn spawn_translator_health_check(&self, translator_generation: u64) {
        runtime_status_sync::spawn_translator_health_check(
            &self.status_context(),
            translator_generation,
        );
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

        crate::application::runtime::lifecycle::stop_monitoring_and_wait(
            &manager.lifecycle_context(),
            Duration::from_secs(1),
        )
        .await
        .expect("wait for monitor stop");

        assert!(manager.monitor_token.lock().await.is_none());
    }

    #[tokio::test]
    async fn stop_monitoring_and_wait_times_out_when_monitor_token_stays_alive() {
        let manager = build_test_manager();
        *manager.monitor_token.lock().await = Some(CancellationToken::new());

        let error = crate::application::runtime::lifecycle::stop_monitoring_and_wait(
            &manager.lifecycle_context(),
            Duration::from_millis(120),
        )
        .await
        .expect_err("should time out");

        assert!(error.to_string().contains("等待监听任务停止超时"));
    }
}
