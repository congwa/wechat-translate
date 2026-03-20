//! TTS 命令层：暴露朗读控制接口给前端，保持命令层薄而专注，业务逻辑委托给 tts_service。
use crate::config::{load_app_config, ConfigDir};
use crate::task_manager::TaskManager;
use crate::tts_service::TtsState;
use crate::CloseToTray;
use serde::Serialize;
use std::sync::Arc;
use tauri::State;

#[derive(Debug, Serialize)]
pub struct TtsStatus {
    pub enabled: bool,
    pub initialized: bool,
    pub speaking: bool,
}

/// 朗读指定文本。由前端侧边栏在检测到新消息时调用。
/// `message_id` 用于前端追踪动画（对应 StoredMessage.id）。
#[tauri::command]
pub async fn tts_speak(
    tts: State<'_, Arc<TtsState>>,
    message_id: u64,
    text: String,
) -> Result<(), String> {
    tts.speak(message_id, &text);
    Ok(())
}

/// 立即停止当前朗读。
#[tauri::command]
pub async fn tts_stop(tts: State<'_, Arc<TtsState>>) -> Result<(), String> {
    tts.stop();
    Ok(())
}

/// 查询 TTS 当前状态。
#[tauri::command]
pub async fn tts_get_status(tts: State<'_, Arc<TtsState>>) -> Result<TtsStatus, String> {
    Ok(TtsStatus {
        enabled: tts.is_enabled(),
        initialized: tts.is_initialized(),
        speaking: tts.is_speaking(),
    })
}

/// 设置 TTS 启用/禁用，同时持久化到配置文件并广播 settings-updated 事件。
#[tauri::command]
pub async fn tts_set_enabled(
    app: tauri::AppHandle,
    tts: State<'_, Arc<TtsState>>,
    config_dir: State<'_, ConfigDir>,
    manager: State<'_, TaskManager>,
    close_to_tray: State<'_, CloseToTray>,
    enabled: bool,
) -> Result<(), String> {
    tts.set_enabled(enabled);

    let mut config = load_app_config(&config_dir.0).map_err(|e| e.to_string())?;
    config.tts.enabled = enabled;
    let config_value = serde_json::to_value(&config).map_err(|e| e.to_string())?;

    crate::interface::commands::config::save_settings_command(
        &app,
        &config_dir,
        &manager,
        &close_to_tray,
        config_value,
    )
    .await
    .map(|_| ())?;

    sync_tray_tts_toggle(&app, enabled);
    Ok(())
}

/// 同步托盘 TTS 开关状态（由内部调用）
pub(crate) fn sync_tray_tts_toggle(app: &tauri::AppHandle, enabled: bool) {
    use tauri::Manager;
    if let Some(tray) = app.try_state::<crate::TrayMenuState>() {
        let _ = tray.tts_toggle.set_checked(enabled);
    }
}

/// 托盘菜单点击 TTS 开关时的处理函数（复用 save_settings_command 链路）
pub(crate) fn handle_tray_toggle_tts(app: &tauri::AppHandle) {
    use tauri::Manager;
    let desired_enabled = app
        .try_state::<crate::TrayMenuState>()
        .and_then(|tray| tray.tts_toggle.is_checked().ok())
        .unwrap_or(false);

    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let tts = app_handle.state::<Arc<TtsState>>();
        let config_dir = app_handle.state::<ConfigDir>();
        let manager = app_handle.state::<TaskManager>();
        let close_to_tray = app_handle.state::<CloseToTray>();

        let mut config = match load_app_config(&config_dir.0) {
            Ok(c) => c,
            Err(e) => {
                log::warn!("[TTS tray] 读取配置失败: {}", e);
                sync_tray_tts_toggle(&app_handle, !desired_enabled);
                return;
            }
        };
        config.tts.enabled = desired_enabled;

        let config_value = match serde_json::to_value(&config) {
            Ok(v) => v,
            Err(e) => {
                log::warn!("[TTS tray] 序列化配置失败: {}", e);
                sync_tray_tts_toggle(&app_handle, !desired_enabled);
                return;
            }
        };

        match crate::interface::commands::config::save_settings_command(
            &app_handle,
            &config_dir,
            &manager,
            &close_to_tray,
            config_value,
        )
        .await
        {
            Ok(_) => {
                tts.set_enabled(desired_enabled);
            }
            Err(e) => {
                log::warn!("[TTS tray] 保存配置失败: {}", e);
                sync_tray_tts_toggle(&app_handle, !desired_enabled);
            }
        }
    });
}
