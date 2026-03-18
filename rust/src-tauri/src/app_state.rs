use crate::application::runtime::service::RuntimeService;
use crate::application::runtime::snapshot_service;
use crate::application::runtime::state::TaskState;
use crate::config::{AppConfig, ConfigDir};
use crate::translator::TranslatorServiceStatus;
use crate::CloseToTray;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::{AppHandle, Emitter, Manager};

pub struct SnapshotVersionState {
    settings_version: AtomicU64,
    runtime_version: AtomicU64,
}

impl Default for SnapshotVersionState {
    fn default() -> Self {
        Self {
            settings_version: AtomicU64::new(1),
            runtime_version: AtomicU64::new(1),
        }
    }
}

impl SnapshotVersionState {
    pub fn current_settings(&self) -> u64 {
        self.settings_version.load(Ordering::Relaxed)
    }

    pub fn current_runtime(&self) -> u64 {
        self.runtime_version.load(Ordering::Relaxed)
    }

    pub fn next_settings(&self) -> u64 {
        self.settings_version.fetch_add(1, Ordering::Relaxed) + 1
    }

    pub fn next_runtime(&self) -> u64 {
        self.runtime_version.fetch_add(1, Ordering::Relaxed) + 1
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingsStateSnapshot {
    pub version: u64,
    pub data: AppConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppRuntimeState {
    pub version: u64,
    pub tasks: TaskState,
    pub translator: TranslatorServiceStatus,
    pub close_to_tray: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppStateSnapshot {
    pub settings: SettingsStateSnapshot,
    pub runtime: AppRuntimeState,
}

/// 合成应用整包快照，供前端启动或重新加载时以 whole snapshot 替换当前状态。
pub async fn snapshot(
    settings: AppConfig,
    runtime: &RuntimeService,
    close_to_tray: &CloseToTray,
    versions: &SnapshotVersionState,
) -> AppStateSnapshot {
    snapshot_service::snapshot(settings, runtime, close_to_tray, versions).await
}

/// 从配置仓与运行态服务读取当前整包快照，作为 app_state_get 的底层实现。
pub async fn load_snapshot(
    config_dir: &ConfigDir,
    runtime: &RuntimeService,
    close_to_tray: &CloseToTray,
    versions: &SnapshotVersionState,
) -> Result<AppStateSnapshot, String> {
    snapshot_service::load_snapshot(config_dir, runtime, close_to_tray, versions).await
}

/// 同步版本的 load_snapshot，用于非 async 上下文
/// 注意：translator 状态使用默认值，需要后续异步更新
pub fn load_snapshot_sync(
    config_dir: &ConfigDir,
    runtime: &RuntimeService,
    close_to_tray: &CloseToTray,
    versions: &SnapshotVersionState,
) -> Result<AppStateSnapshot, String> {
    snapshot_service::load_snapshot_sync(config_dir, runtime, close_to_tray, versions)
}

/// 广播 settings snapshot 更新，并同步菜单栏/托盘里的翻译开关状态。
pub fn emit_settings_updated(app: &AppHandle, settings: &AppConfig) {
    if let Some(menu_state) = app.try_state::<crate::TrayMenuState>() {
        // 同步 macOS 应用菜单栏的翻译开关
        if let Some(toggle) = &menu_state.translate_enabled_check {
            let _ = toggle.set_checked(settings.translate.enabled);
        }
        // 同步系统托盘菜单的翻译开关
        let _ = menu_state
            .translate_toggle
            .set_checked(settings.translate.enabled);
    }
    if let Some(versions) = app.try_state::<SnapshotVersionState>() {
        let snapshot = SettingsStateSnapshot {
            version: versions.next_settings(),
            data: settings.clone(),
        };
        let _ = app.emit("settings-updated", snapshot);
    }
}

/// 广播 runtime snapshot 更新，让前端统一通过 whole snapshot 替换同步运行态。
pub fn emit_runtime_updated(app: &AppHandle, runtime: RuntimeService) {
    snapshot_service::emit_runtime_updated(app, runtime);
}
