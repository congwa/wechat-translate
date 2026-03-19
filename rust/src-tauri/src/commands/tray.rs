use crate::app_state;
use crate::application::runtime::service::RuntimeService;
use crate::CloseToTray;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Manager, State};

/// 返回当前 close-to-tray 偏好，供设置页和托盘 UI 展示当前值。
pub fn get_close_to_tray(state: State<'_, CloseToTray>) -> bool {
    state.0.load(Ordering::Relaxed)
}

/// 更新 close-to-tray 偏好，并同步刷新托盘复选框和 runtime snapshot。
pub fn set_close_to_tray(
    app: AppHandle,
    state: State<'_, CloseToTray>,
    manager: State<'_, crate::task_manager::TaskManager>,
    enabled: bool,
) {
    state.0.store(enabled, Ordering::Relaxed);
    if let Some(tray) = app.try_state::<crate::TrayMenuState>() {
        let _ = tray.close_to_tray_check.set_checked(enabled);
    }
    let runtime = RuntimeService::new(manager.inner().clone());
    app_state::emit_runtime_updated(&app, runtime);
}
