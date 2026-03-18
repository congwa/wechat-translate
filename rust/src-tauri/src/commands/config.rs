use crate::app_state;
use crate::application::runtime::service::RuntimeService;
use crate::application::settings::service::SettingsService;
use crate::config::{self as app_config, ConfigDir};
use crate::task_manager::TaskManager;
use tauri::{Emitter, Manager};

#[tauri::command]
pub async fn config_get(
    config_dir: tauri::State<'_, ConfigDir>,
) -> Result<serde_json::Value, String> {
    let config = app_config::read_config(&config_dir.0).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "ok": true, "data": config }))
}

#[tauri::command]
pub async fn config_put(
    app: tauri::AppHandle,
    config_dir: tauri::State<'_, ConfigDir>,
    manager: tauri::State<'_, TaskManager>,
    config: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let runtime = RuntimeService::new(manager.inner().clone());
    let settings_service = SettingsService::new(&config_dir, runtime);
    let result = settings_service.save_raw_config(&config).await?;

    if !result.errors.is_empty() {
        return Ok(serde_json::json!({ "ok": false, "errors": result.errors }));
    }

    let app_config = result
        .settings
        .ok_or_else(|| "配置保存后未生成有效快照".to_string())?;
    app_state::emit_settings_updated(&app, &app_config);
    app_state::emit_runtime_updated(&app, &manager);
    let _ = app.emit(
        "config-updated",
        serde_json::to_value(app_state::SettingsStateSnapshot {
            version: app
                .state::<app_state::SnapshotVersionState>()
                .current_settings(),
            data: app_config,
        })
        .unwrap_or_default(),
    );

    Ok(serde_json::json!({ "ok": true, "message": "config saved", "path": result.path }))
}

#[tauri::command]
pub async fn config_default() -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({ "ok": true, "data": app_config::default_config_value() }))
}
