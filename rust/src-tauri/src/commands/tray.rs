use crate::CloseToTray;
use std::sync::atomic::Ordering;
use tauri::State;

#[tauri::command]
pub fn get_close_to_tray(state: State<'_, CloseToTray>) -> bool {
    state.0.load(Ordering::Relaxed)
}

#[tauri::command]
pub fn set_close_to_tray(state: State<'_, CloseToTray>, enabled: bool) {
    state.0.store(enabled, Ordering::Relaxed);
}
