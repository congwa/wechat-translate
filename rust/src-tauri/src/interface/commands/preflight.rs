//! 预检命令入口：负责把权限检查、权限申请、打开系统设置和兜底重启提示通过 Tauri 暴露给前端，
//! 让预检链路逐步脱离旧 `commands/preflight.rs` 的直接命令注册。
use crate::commands;
use serde_json::Value;

/// 返回当前微信运行态与辅助功能权限预检结果，供前端决定是否继续初始化监听链路。
#[tauri::command]
pub fn preflight_check() -> Value {
    commands::preflight::preflight_check()
}

/// 请求辅助功能权限，并把这次请求前后是否已授权的状态返回给前端。
#[tauri::command]
pub fn accessibility_request_access() -> Value {
    commands::preflight::accessibility_request_access()
}

/// 打开系统设置中的辅助功能权限页，帮助用户跳转到正确的授权位置。
#[tauri::command]
pub fn accessibility_open_settings() -> Value {
    commands::preflight::accessibility_open_settings()
}

/// 在监听恢复失败时弹出原生“立即重启”提示，作为权限恢复链路的最终兜底。
#[tauri::command]
pub fn preflight_prompt_restart(app: tauri::AppHandle) -> Value {
    commands::preflight::preflight_prompt_restart(app)
}
