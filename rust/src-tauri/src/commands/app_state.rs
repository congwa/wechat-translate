use crate::app_state;
use crate::application::runtime::service::RuntimeService;
use crate::application::settings::service::SettingsService;
use crate::config::ConfigDir;
use crate::task_manager::TaskManager;
use crate::CloseToTray;
use tauri::Manager;

pub async fn save_settings(
    app: &tauri::AppHandle,
    config_dir: &ConfigDir,
    manager: &TaskManager,
    close_to_tray: &CloseToTray,
    settings: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let runtime = RuntimeService::new(manager.clone());
    let settings_service = SettingsService::new(config_dir, runtime.clone());
    let result = settings_service.save_raw_config(&settings).await?;
    if !result.errors.is_empty() {
        return Ok(serde_json::json!({ "ok": false, "errors": result.errors }));
    }

    let settings = result
        .settings
        .ok_or_else(|| "配置保存后未生成有效快照".to_string())?;
    app_state::emit_settings_updated(app, &settings);
    app_state::emit_runtime_updated(app, runtime.clone());

    let versions = app.state::<app_state::SnapshotVersionState>();
    let snapshot = app_state::snapshot(settings, &runtime, close_to_tray, &versions).await;
    Ok(serde_json::json!({
        "ok": true,
        "message": "settings saved",
        "path": result.path,
        "data": snapshot,
    }))
}

pub async fn settings_update(
    app: tauri::AppHandle,
    config_dir: tauri::State<'_, ConfigDir>,
    manager: tauri::State<'_, TaskManager>,
    close_to_tray: tauri::State<'_, CloseToTray>,
    settings: serde_json::Value,
) -> Result<serde_json::Value, String> {
    save_settings(&app, &config_dir, &manager, &close_to_tray, settings).await
}
