use crate::config::{load_app_config, AppConfig, ConfigDir};
use crate::task_manager::{TaskManager, TaskState, TranslatorServiceStatus};
use crate::CloseToTray;
use serde::{Deserialize, Serialize};
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppRuntimeState {
    pub tasks: TaskState,
    pub translator: TranslatorServiceStatus,
    pub close_to_tray: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppStateSnapshot {
    pub settings: AppConfig,
    pub runtime: AppRuntimeState,
}

pub fn runtime_snapshot(manager: &TaskManager, close_to_tray: &CloseToTray) -> AppRuntimeState {
    AppRuntimeState {
        tasks: manager.get_task_state(),
        translator: manager.get_translator_status(),
        close_to_tray: close_to_tray.0.load(Ordering::Relaxed),
    }
}

pub fn snapshot(
    settings: AppConfig,
    manager: &TaskManager,
    close_to_tray: &CloseToTray,
) -> AppStateSnapshot {
    AppStateSnapshot {
        settings,
        runtime: runtime_snapshot(manager, close_to_tray),
    }
}

pub fn load_snapshot(
    config_dir: &ConfigDir,
    manager: &TaskManager,
    close_to_tray: &CloseToTray,
) -> Result<AppStateSnapshot, String> {
    let settings = load_app_config(&config_dir.0).map_err(|error| error.to_string())?;
    Ok(snapshot(settings, manager, close_to_tray))
}

pub fn emit_settings_updated(app: &AppHandle, settings: &AppConfig) {
    if let Some(menu_state) = app.try_state::<crate::TrayMenuState>() {
        if let Some(toggle) = &menu_state.translate_enabled_check {
            let _ = toggle.set_checked(settings.translate.enabled);
        }
    }
    let _ = app.emit("settings-updated", settings);
}

pub fn emit_runtime_updated(app: &AppHandle, manager: &TaskManager) {
    if let Some(close_to_tray) = app.try_state::<CloseToTray>() {
        let _ = app.emit("runtime-updated", runtime_snapshot(manager, &close_to_tray));
    }
}
