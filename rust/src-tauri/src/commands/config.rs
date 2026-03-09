use crate::config::{self as app_config, ConfigDir};

#[tauri::command]
pub async fn config_get(
    config_dir: tauri::State<'_, ConfigDir>,
) -> Result<serde_json::Value, String> {
    let config = app_config::read_config(&config_dir.0).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "ok": true, "data": config }))
}

#[tauri::command]
pub async fn config_put(
    config_dir: tauri::State<'_, ConfigDir>,
    config: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let (errors, path) =
        app_config::validate_and_write_config(&config_dir.0, &config).map_err(|e| e.to_string())?;

    if !errors.is_empty() {
        return Ok(serde_json::json!({ "ok": false, "errors": errors }));
    }

    Ok(serde_json::json!({ "ok": true, "message": "config saved", "path": path }))
}

#[tauri::command]
pub async fn config_default() -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({ "ok": true, "data": app_config::default_config_value() }))
}
