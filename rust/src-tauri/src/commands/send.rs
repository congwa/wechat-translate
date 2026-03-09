use crate::adapter::MacOSAdapter;
use std::sync::Arc;

#[tauri::command]
pub async fn send_text(
    adapter: tauri::State<'_, Arc<MacOSAdapter>>,
    who: String,
    text: String,
) -> Result<serde_json::Value, String> {
    let adapter = adapter.inner().clone();
    adapter.pause_ui();
    let send_adapter = adapter.clone();
    let result = tokio::task::spawn_blocking(move || send_adapter.send_text(&who, &text)).await;
    adapter.resume_ui();
    result
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;

    Ok(serde_json::json!({ "ok": true, "message": "text sent" }))
}

#[tauri::command]
pub async fn send_files(
    adapter: tauri::State<'_, Arc<MacOSAdapter>>,
    who: String,
    file_paths: Vec<String>,
) -> Result<serde_json::Value, String> {
    let adapter = adapter.inner().clone();
    adapter.pause_ui();
    let send_adapter = adapter.clone();
    let result =
        tokio::task::spawn_blocking(move || send_adapter.send_files(&who, &file_paths)).await;
    adapter.resume_ui();
    result
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;

    Ok(serde_json::json!({ "ok": true, "message": "files sent" }))
}
