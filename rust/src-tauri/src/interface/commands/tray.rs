//! 托盘命令入口：负责把 close-to-tray 这类原生窗口行为偏好通过 Tauri 暴露给前端，
//! 让 tray 相关命令逐步脱离旧 `commands/tray.rs` 的直接注册方式。
use crate::commands;
use crate::task_manager::TaskManager;
use crate::CloseToTray;
use tauri::{AppHandle, State};

/// 返回当前 close-to-tray 偏好，供设置页和托盘 UI 展示当前值。
#[tauri::command]
pub fn get_close_to_tray(state: State<'_, CloseToTray>) -> bool {
    commands::tray::get_close_to_tray(state)
}

/// 更新 close-to-tray 偏好，并同步刷新托盘复选框和 runtime snapshot。
#[tauri::command]
pub fn set_close_to_tray(
    app: AppHandle,
    state: State<'_, CloseToTray>,
    manager: State<'_, TaskManager>,
    enabled: bool,
) {
    commands::tray::set_close_to_tray(app, state, manager, enabled)
}
