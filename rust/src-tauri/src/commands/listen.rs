use crate::task_manager::TaskManager;

#[tauri::command]
pub async fn listen_start(
    manager: tauri::State<'_, TaskManager>,
    interval_seconds: Option<f64>,
) -> Result<serde_json::Value, String> {
    let interval = interval_seconds.unwrap_or(1.0);
    manager
        .start_monitoring(interval)
        .await
        .map_err(|e| e.to_string())?;

    Ok(serde_json::json!({ "ok": true, "message": "monitoring started" }))
}

#[tauri::command]
pub async fn listen_stop(
    manager: tauri::State<'_, TaskManager>,
) -> Result<serde_json::Value, String> {
    manager.stop_monitoring().await.map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "ok": true, "message": "monitoring stopped" }))
}

#[tauri::command]
pub async fn autoreply_start(
    manager: tauri::State<'_, TaskManager>,
) -> Result<serde_json::Value, String> {
    manager
        .enable_autoreply()
        .await
        .map_err(|e| e.to_string())?;

    Ok(serde_json::json!({ "ok": true, "message": "autoreply enabled" }))
}

#[tauri::command]
pub async fn autoreply_stop(
    manager: tauri::State<'_, TaskManager>,
) -> Result<serde_json::Value, String> {
    manager
        .disable_autoreply()
        .await
        .map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "ok": true, "message": "autoreply disabled" }))
}

#[tauri::command]
pub async fn get_task_status(
    manager: tauri::State<'_, TaskManager>,
) -> Result<serde_json::Value, String> {
    let status = manager.service_status();
    Ok(serde_json::json!({ "ok": true, "data": status }))
}

#[tauri::command]
pub async fn health_check(
    manager: tauri::State<'_, TaskManager>,
) -> Result<serde_json::Value, String> {
    let status = manager.service_status();
    Ok(serde_json::json!({ "status": "ok", "service_status": status }))
}
