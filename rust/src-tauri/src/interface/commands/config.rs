//! 配置写命令入口：负责把设置保存、原始配置读写和默认配置模板通过 Tauri 暴露给前端，
//! 让“配置真相”这条写链路逐步从旧 `commands/*` 目录迁到 `interface/commands`。
use crate::app_state;
use crate::application::runtime::service::RuntimeService;
use crate::application::settings::service::SettingsService;
use crate::config::{self as app_config, ConfigDir};
use crate::task_manager::TaskManager;
use crate::CloseToTray;
use tauri::{Emitter, Manager};

/// 保存一份结构化 settings，并在成功后同步广播 settings/runtime 两份 snapshot。
/// 这个 helper 同时服务于 Tauri 命令入口和托盘内部调用，避免保存逻辑回流到旧命令层。
pub(crate) async fn save_settings_command(
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

/// 保存结构化 settings，并同步触发 settings/runtime 快照更新。
#[tauri::command]
pub async fn settings_update(
    app: tauri::AppHandle,
    config_dir: tauri::State<'_, ConfigDir>,
    manager: tauri::State<'_, TaskManager>,
    close_to_tray: tauri::State<'_, CloseToTray>,
    settings: serde_json::Value,
) -> Result<serde_json::Value, String> {
    save_settings_command(&app, &config_dir, &manager, &close_to_tray, settings).await
}

/// 读取当前配置原始 JSON，供设置页高级编辑器和兼容调试入口使用。
#[tauri::command]
pub async fn config_get(
    config_dir: tauri::State<'_, ConfigDir>,
) -> Result<serde_json::Value, String> {
    let config = app_config::read_config(&config_dir.0).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "ok": true, "data": config }))
}

/// 保存完整配置 JSON，并触发后端运行态按新配置重建或刷新。
#[tauri::command]
pub async fn config_put(
    app: tauri::AppHandle,
    config_dir: tauri::State<'_, ConfigDir>,
    manager: tauri::State<'_, TaskManager>,
    config: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let runtime = RuntimeService::new(manager.inner().clone());
    let settings_service = SettingsService::new(&config_dir, runtime.clone());
    let result = settings_service.save_raw_config(&config).await?;

    if !result.errors.is_empty() {
        return Ok(serde_json::json!({ "ok": false, "errors": result.errors }));
    }

    let app_config = result
        .settings
        .ok_or_else(|| "配置保存后未生成有效快照".to_string())?;
    app_state::emit_settings_updated(&app, &app_config);
    app_state::emit_runtime_updated(&app, runtime.clone());
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

/// 返回默认配置模板，供高级配置编辑器一键恢复默认值。
#[tauri::command]
pub async fn config_default() -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({ "ok": true, "data": app_config::default_config_value() }))
}
