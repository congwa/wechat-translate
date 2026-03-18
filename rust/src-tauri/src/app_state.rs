use crate::config::{load_app_config, AppConfig, ConfigDir};
use crate::task_manager::{TaskManager, TaskState};
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

pub async fn runtime_snapshot(
    manager: &TaskManager,
    close_to_tray: &CloseToTray,
    versions: &SnapshotVersionState,
) -> AppRuntimeState {
    AppRuntimeState {
        version: versions.current_runtime(),
        tasks: manager.get_task_state(),
        translator: manager.get_translator_status().await,
        close_to_tray: close_to_tray.0.load(Ordering::Relaxed),
    }
}

pub async fn snapshot(
    settings: AppConfig,
    manager: &TaskManager,
    close_to_tray: &CloseToTray,
    versions: &SnapshotVersionState,
) -> AppStateSnapshot {
    AppStateSnapshot {
        settings: SettingsStateSnapshot {
            version: versions.current_settings(),
            data: settings,
        },
        runtime: runtime_snapshot(manager, close_to_tray, versions).await,
    }
}

pub async fn load_snapshot(
    config_dir: &ConfigDir,
    manager: &TaskManager,
    close_to_tray: &CloseToTray,
    versions: &SnapshotVersionState,
) -> Result<AppStateSnapshot, String> {
    let settings = load_app_config(&config_dir.0).map_err(|error| error.to_string())?;
    Ok(snapshot(settings, manager, close_to_tray, versions).await)
}

/// 同步版本的 load_snapshot，用于非 async 上下文
/// 注意：translator 状态使用默认值，需要后续异步更新
pub fn load_snapshot_sync(
    config_dir: &ConfigDir,
    manager: &TaskManager,
    close_to_tray: &CloseToTray,
    versions: &SnapshotVersionState,
) -> Result<AppStateSnapshot, String> {
    let settings = load_app_config(&config_dir.0).map_err(|error| error.to_string())?;
    // 同步版本无法获取 async translator status，使用 disabled 状态
    // 实际状态会通过后续的 runtime-updated 事件更新
    Ok(AppStateSnapshot {
        settings: SettingsStateSnapshot {
            version: versions.current_settings(),
            data: settings,
        },
        runtime: AppRuntimeState {
            version: versions.current_runtime(),
            tasks: manager.get_task_state(),
            translator: crate::translator::TranslatorServiceStatus::disabled(),
            close_to_tray: close_to_tray.0.load(Ordering::Relaxed),
        },
    })
}

pub fn emit_settings_updated(app: &AppHandle, settings: &AppConfig) {
    if let Some(menu_state) = app.try_state::<crate::TrayMenuState>() {
        // 同步 macOS 应用菜单栏的翻译开关
        if let Some(toggle) = &menu_state.translate_enabled_check {
            let _ = toggle.set_checked(settings.translate.enabled);
        }
        // 同步系统托盘菜单的翻译开关
        let _ = menu_state.translate_toggle.set_checked(settings.translate.enabled);
    }
    if let Some(versions) = app.try_state::<SnapshotVersionState>() {
        let snapshot = SettingsStateSnapshot {
            version: versions.next_settings(),
            data: settings.clone(),
        };
        let _ = app.emit("settings-updated", snapshot);
    }
}

pub fn emit_runtime_updated_with_status(
    app: &AppHandle,
    tasks: TaskState,
    translator: TranslatorServiceStatus,
) {
    if let (Some(close_to_tray), Some(versions)) = (
        app.try_state::<CloseToTray>(),
        app.try_state::<SnapshotVersionState>(),
    ) {
        let runtime = AppRuntimeState {
            version: versions.next_runtime(),
            tasks,
            translator,
            close_to_tray: close_to_tray.0.load(Ordering::Relaxed),
        };
        let _ = app.emit("runtime-updated", runtime);
    }
}

pub fn emit_runtime_updated(app: &AppHandle, manager: &TaskManager) {
    // 同步版本：使用 spawn 异步发送事件
    let app_clone = app.clone();
    let manager_clone = manager.clone();
    tauri::async_runtime::spawn(async move {
        if let (Some(close_to_tray), Some(versions)) = (
            app_clone.try_state::<CloseToTray>(),
            app_clone.try_state::<SnapshotVersionState>(),
        ) {
            let tasks = manager_clone.get_task_state();
            let translator = manager_clone.get_translator_status().await;
            let runtime = AppRuntimeState {
                version: versions.next_runtime(),
                tasks,
                translator,
                close_to_tray: close_to_tray.0.load(Ordering::Relaxed),
            };
            let _ = app_clone.emit("runtime-updated", runtime);
        }
    });
}
