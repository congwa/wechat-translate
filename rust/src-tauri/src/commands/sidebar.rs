use crate::config::{load_app_config, ConfigDir};
use crate::db::MessageDb;
use crate::sidebar_window::{SidebarWindowState, WindowMode};
use crate::task_manager::TaskManager;
use crate::translator::DeepLXTranslator;
use std::sync::Arc;

#[tauri::command]
pub async fn sidebar_start(
    config_dir: tauri::State<'_, ConfigDir>,
    manager: tauri::State<'_, TaskManager>,
    targets: Option<Vec<String>>,
    translate_enabled: Option<bool>,
    deeplx_url: Option<String>,
    source_lang: Option<String>,
    target_lang: Option<String>,
    timeout_seconds: Option<f64>,
    max_concurrency: Option<usize>,
    max_requests_per_second: Option<usize>,
    image_capture: Option<bool>,
) -> Result<serde_json::Value, String> {
    let config = load_app_config(&config_dir.0).map_err(|e| e.to_string())?;
    manager
        .set_use_right_panel_details(config.listen.use_right_panel_details)
        .await;

    let translate_enabled = translate_enabled.unwrap_or(config.translate.enabled);
    let deeplx_url = deeplx_url.unwrap_or_else(|| config.translate.deeplx_url.clone());

    manager
        .enable_sidebar(
            targets.unwrap_or_default(),
            translate_enabled,
            deeplx_url,
            source_lang.unwrap_or_else(|| config.translate.source_lang.clone()),
            target_lang.unwrap_or_else(|| config.translate.target_lang.clone()),
            timeout_seconds.unwrap_or(config.translate.timeout_seconds),
            max_concurrency.unwrap_or(config.translate.max_concurrency),
            max_requests_per_second.unwrap_or(config.translate.max_requests_per_second),
            image_capture.unwrap_or(false),
        )
        .await
        .map_err(|e| e.to_string())?;

    Ok(serde_json::json!({ "ok": true, "message": "sidebar enabled" }))
}

#[tauri::command]
pub async fn sidebar_stop(
    app: tauri::AppHandle,
    manager: tauri::State<'_, TaskManager>,
    sidebar_state: tauri::State<'_, Arc<SidebarWindowState>>,
) -> Result<serde_json::Value, String> {
    manager.disable_sidebar().await.map_err(|e| e.to_string())?;
    let _ = sidebar_state.close(&app).await;
    Ok(serde_json::json!({ "ok": true, "message": "sidebar disabled" }))
}

#[tauri::command]
pub async fn live_start(
    app: tauri::AppHandle,
    config_dir: tauri::State<'_, ConfigDir>,
    manager: tauri::State<'_, TaskManager>,
    sidebar_state: tauri::State<'_, Arc<SidebarWindowState>>,
    translate_enabled: Option<bool>,
    deeplx_url: Option<String>,
    source_lang: Option<String>,
    target_lang: Option<String>,
    interval_seconds: Option<f64>,
    timeout_seconds: Option<f64>,
    max_concurrency: Option<usize>,
    max_requests_per_second: Option<usize>,
    image_capture: Option<bool>,
    window_mode: Option<String>,
) -> Result<serde_json::Value, String> {
    let mode = WindowMode::from_str_opt(window_mode.as_deref());
    let config = load_app_config(&config_dir.0).map_err(|e| e.to_string())?;
    manager
        .set_use_right_panel_details(config.listen.use_right_panel_details)
        .await;

    let translate_enabled = translate_enabled.unwrap_or(config.translate.enabled);
    let deeplx_url = deeplx_url.unwrap_or_else(|| config.translate.deeplx_url.clone());

    let state = manager.get_task_state();
    if !state.monitoring {
        let interval = interval_seconds.unwrap_or(1.0);
        manager
            .start_monitoring(interval)
            .await
            .map_err(|e| e.to_string())?;
    }

    manager
        .enable_sidebar(
            vec![],
            translate_enabled,
            deeplx_url,
            source_lang.unwrap_or_else(|| config.translate.source_lang.clone()),
            target_lang.unwrap_or_else(|| config.translate.target_lang.clone()),
            timeout_seconds.unwrap_or(config.translate.timeout_seconds),
            max_concurrency.unwrap_or(config.translate.max_concurrency),
            max_requests_per_second.unwrap_or(config.translate.max_requests_per_second),
            image_capture.unwrap_or(false),
        )
        .await
        .map_err(|e| e.to_string())?;

    let _ = sidebar_state
        .open(&app, Some(config.display.width as f64), mode)
        .await;

    Ok(serde_json::json!({ "ok": true, "message": "live started" }))
}

#[tauri::command]
pub async fn sidebar_window_open(
    app: tauri::AppHandle,
    config_dir: tauri::State<'_, ConfigDir>,
    state: tauri::State<'_, Arc<SidebarWindowState>>,
    width: Option<f64>,
    window_mode: Option<String>,
) -> Result<serde_json::Value, String> {
    let mode = WindowMode::from_str_opt(window_mode.as_deref());
    let config = load_app_config(&config_dir.0).map_err(|e| e.to_string())?;
    state
        .open(&app, width.or(Some(config.display.width as f64)), mode)
        .await
        .map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "ok": true, "message": "sidebar window opened" }))
}

#[tauri::command]
pub async fn sidebar_window_close(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<SidebarWindowState>>,
) -> Result<serde_json::Value, String> {
    state.close(&app).await.map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "ok": true, "message": "sidebar window closed" }))
}

#[tauri::command]
pub async fn sidebar_snapshot_get(
    db: tauri::State<'_, Arc<MessageDb>>,
    manager: tauri::State<'_, TaskManager>,
    chat_name: Option<String>,
    limit: Option<i64>,
) -> Result<serde_json::Value, String> {
    let selected_chat = match chat_name.as_deref().map(str::trim).filter(|name| !name.is_empty()) {
        Some(name) => Some(name.to_string()),
        None => db.latest_chat_name().map_err(|e| e.to_string())?,
    };

    let messages = if let Some(chat) = selected_chat.as_deref() {
        db.query_messages(Some(chat), None, None, limit.unwrap_or(50), 0)
            .map_err(|e| e.to_string())?
    } else {
        Vec::new()
    };

    Ok(serde_json::json!({
        "ok": true,
        "data": {
            "current_chat": selected_chat,
            "messages": messages,
            "translator": manager.get_translator_status(),
        }
    }))
}

#[tauri::command]
pub async fn translate_test(
    deeplx_url: String,
    source_lang: Option<String>,
    target_lang: Option<String>,
    timeout_seconds: Option<f64>,
) -> Result<serde_json::Value, String> {
    if deeplx_url.is_empty() {
        return Err("DeepLX 地址不能为空".to_string());
    }

    let translator = DeepLXTranslator::new(
        &deeplx_url,
        &source_lang.unwrap_or_else(|| "auto".to_string()),
        &target_lang.unwrap_or_else(|| "EN".to_string()),
        timeout_seconds.unwrap_or(8.0),
    );

    match translator.translate("你好，世界").await {
        Ok(result) => Ok(serde_json::json!({ "ok": true, "data": result })),
        Err(e) => Err(format!("{}", e)),
    }
}
