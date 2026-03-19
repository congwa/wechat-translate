//! 预检命令入口：负责把权限检查、权限申请、打开系统设置、权限恢复和兜底重启提示通过 Tauri 暴露给前端，
//! 让预检链路彻底落在 `interface/commands/preflight.rs`，不再依赖旧命令层的直接注册。
use crate::adapter::ax_reader;
use crate::application::runtime::service::RuntimeService;
use crate::config::{load_app_config, ConfigDir};
use crate::events::{EventStore, EventType};
use crate::task_manager::TaskManager;
use core_foundation::base::{CFType, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::CFDictionary;
use log::{debug, info, warn};
use serde::Serialize;
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;
use tauri::Manager;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

const ACCESSIBILITY_SETTINGS_URL: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility";

/// AccessibilityRequestResult 表示一次辅助功能权限请求前后的状态变化。
/// 前端据此判断是否真的弹出了系统权限框，以及用户是否已经在系统层完成授权。
#[derive(Debug, Clone, Serialize)]
pub struct AccessibilityRequestResult {
    trusted_before: bool,
    prompt_attempted: bool,
    trusted_after_check: bool,
    settings_opened: bool,
}

/// 检查微信进程是否存在，并返回当前可解析的进程 PID。
fn check_wechat_pid() -> Option<i32> {
    ax_reader::resolve_wechat_pid().ok()
}

/// 检查微信进程在当前授权状态下是否可通过 AX 读取窗口列表。
fn check_wechat_accessibility(pid: i32) -> bool {
    unsafe {
        let app = accessibility_sys::AXUIElementCreateApplication(pid);
        if app.is_null() {
            return false;
        }

        let attr = core_foundation::string::CFString::new("AXWindows");
        let mut value: core_foundation_sys::base::CFTypeRef = std::ptr::null();
        let err = accessibility_sys::AXUIElementCopyAttributeValue(
            app,
            core_foundation::base::TCFType::as_concrete_TypeRef(&attr),
            &mut value,
        );

        if !value.is_null() {
            core_foundation_sys::base::CFRelease(value);
        }
        core_foundation_sys::base::CFRelease(app as _);

        err == 0
    }
}

/// 返回当前进程是否已经获得辅助功能权限。
fn is_process_trusted() -> bool {
    unsafe { accessibility_sys::AXIsProcessTrusted() }
}

/// 触发系统级辅助功能权限提示。
/// 业务上这是“请求授权”唯一合法方式，不能由前端模拟状态跳过。
fn request_process_trusted_with_prompt() -> bool {
    unsafe {
        let prompt_key: core_foundation::string::CFString =
            TCFType::wrap_under_get_rule(accessibility_sys::kAXTrustedCheckOptionPrompt);
        let options: CFDictionary<CFType, CFType> = CFDictionary::from_CFType_pairs(&[(
            prompt_key.as_CFType(),
            CFBoolean::true_value().as_CFType(),
        )]);
        accessibility_sys::AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef())
    }
}

/// 构造一次辅助功能权限请求的结果模型。
/// 业务上把“请求前已授权”和“请求后才授权”分开表达，避免前端误把无动作当成成功请求。
fn build_accessibility_request_result<F, G>(
    trusted_before: bool,
    request_with_prompt: F,
    check_after: G,
) -> AccessibilityRequestResult
where
    F: FnOnce() -> bool,
    G: FnOnce() -> bool,
{
    if trusted_before {
        return AccessibilityRequestResult {
            trusted_before: true,
            prompt_attempted: false,
            trusted_after_check: true,
            settings_opened: false,
        };
    }

    let _ = request_with_prompt();
    let trusted_after_check = check_after();
    AccessibilityRequestResult {
        trusted_before: false,
        prompt_attempted: true,
        trusted_after_check,
        settings_opened: false,
    }
}

/// 检查当前微信是否已经存在窗口。
/// 业务上这用于区分“进程存在但没有真正可交互窗口”的场景。
fn check_has_window(pid: i32) -> bool {
    unsafe {
        let app = accessibility_sys::AXUIElementCreateApplication(pid);
        if app.is_null() {
            return false;
        }

        let attr = core_foundation::string::CFString::new("AXWindows");
        let mut value: core_foundation_sys::base::CFTypeRef = std::ptr::null();
        let err = accessibility_sys::AXUIElementCopyAttributeValue(
            app,
            core_foundation::base::TCFType::as_concrete_TypeRef(&attr),
            &mut value,
        );

        if err != 0 || value.is_null() {
            core_foundation_sys::base::CFRelease(app as _);
            return false;
        }

        let count = core_foundation_sys::array::CFArrayGetCount(
            value as core_foundation_sys::array::CFArrayRef,
        );

        core_foundation_sys::base::CFRelease(value);
        core_foundation_sys::base::CFRelease(app as _);

        count > 0
    }
}

/// 构造 preflight 结果。
/// 业务上保持字段兼容性，避免前端预检条和恢复逻辑因为字段变化失效。
fn build_preflight_result(
    wechat_running: bool,
    accessibility_ok: bool,
    wechat_accessible: bool,
    wechat_has_window: bool,
) -> Value {
    serde_json::json!({
        "wechat_running": wechat_running,
        "accessibility_ok": accessibility_ok,
        "wechat_accessible": wechat_accessible,
        "wechat_has_window": wechat_has_window,
        "can_prompt_accessibility": !accessibility_ok,
    })
}

/// 返回当前微信运行态与辅助功能权限预检结果，供前端决定是否继续初始化监听链路。
#[tauri::command]
pub fn preflight_check() -> Value {
    let pid = check_wechat_pid();
    let accessibility_ok = is_process_trusted();
    let wechat_running = pid.is_some();
    let wechat_accessible = if accessibility_ok {
        pid.is_some_and(check_wechat_accessibility)
    } else {
        false
    };
    let wechat_has_window = if accessibility_ok {
        pid.is_some_and(check_has_window)
    } else {
        false
    };
    debug!(
        "preflight_check wechat_running={} accessibility_ok={} wechat_accessible={} wechat_has_window={}",
        wechat_running, accessibility_ok, wechat_accessible, wechat_has_window
    );

    build_preflight_result(
        wechat_running,
        accessibility_ok,
        wechat_accessible,
        wechat_has_window,
    )
}

/// 请求辅助功能权限，并把这次请求前后是否已授权的状态返回给前端。
#[tauri::command]
pub fn accessibility_request_access() -> Value {
    let trusted_before = is_process_trusted();
    info!(
        "accessibility_request_access trusted_before={}",
        trusted_before
    );
    let result = build_accessibility_request_result(
        trusted_before,
        request_process_trusted_with_prompt,
        || {
            std::thread::sleep(Duration::from_millis(300));
            is_process_trusted()
        },
    );
    info!(
        "accessibility_request_access prompt_attempted={} trusted_after_check={}",
        result.prompt_attempted, result.trusted_after_check
    );
    serde_json::to_value(result).unwrap_or_else(|_| {
        serde_json::json!({
            "trusted_before": trusted_before,
            "prompt_attempted": !trusted_before,
            "trusted_after_check": false,
            "settings_opened": false,
        })
    })
}

/// 打开系统设置中的辅助功能权限页，帮助用户跳转到正确的授权位置。
#[tauri::command]
pub fn accessibility_open_settings() -> Value {
    info!("accessibility_open_settings opening system settings url");
    let status = std::process::Command::new("open")
        .arg(ACCESSIBILITY_SETTINGS_URL)
        .status();

    match status {
        Ok(s) if s.success() => serde_json::json!({
            "ok": true,
            "settings_opened": true
        }),
        Ok(s) => {
            warn!("accessibility_open_settings failed: {}", s);
            serde_json::json!({
                "ok": false,
                "settings_opened": false,
                "message": format!("open settings exit status: {}", s)
            })
        }
        Err(e) => {
            warn!("accessibility_open_settings failed to execute: {}", e);
            serde_json::json!({
                "ok": false,
                "settings_opened": false,
                "message": e.to_string()
            })
        }
    }
}

/// 在辅助功能权限恢复后重建监听运行态。
/// 业务上这条链只在权限恢复成功后才应执行，且重建失败时必须回流明确错误给前端。
#[tauri::command]
pub async fn accessibility_recover_listener(
    app: tauri::AppHandle,
    config_dir: tauri::State<'_, ConfigDir>,
    manager: tauri::State<'_, TaskManager>,
) -> Result<Value, String> {
    let runtime = RuntimeService::new(manager.inner().clone());
    if !is_process_trusted() {
        return Err("辅助功能权限尚未授权".to_string());
    }

    let config = load_app_config(&config_dir.0).map_err(|e| e.to_string())?;
    runtime
        .set_use_right_panel_details(config.listen.use_right_panel_details)
        .await;

    if let Some(events) = app.try_state::<Arc<EventStore>>() {
        events.publish(
            &app,
            EventType::Status,
            "preflight",
            serde_json::json!({
                "type": "accessibility-recover-started",
            }),
        );
    }

    info!("accessibility_recover_listener restarting monitor after trust recovery");
    match runtime
        .restart_monitoring(config.listen.interval_seconds, Duration::from_secs(5), true)
        .await
    {
        Ok(first_chat) => {
            if let Some(events) = app.try_state::<Arc<EventStore>>() {
                events.publish(
                    &app,
                    EventType::Status,
                    "preflight",
                    serde_json::json!({
                        "type": "accessibility-recover-succeeded",
                        "chat_name": first_chat.clone().unwrap_or_default(),
                    }),
                );
            }

            Ok(serde_json::json!({
                "ok": true,
                "message": "监听已重建",
                "chat_name": first_chat,
                "sidebar_refreshed": runtime.task_state().sidebar,
            }))
        }
        Err(error) => {
            let error_text = error.to_string();
            warn!("accessibility_recover_listener failed: {}", error_text);
            if let Some(events) = app.try_state::<Arc<EventStore>>() {
                events.publish(
                    &app,
                    EventType::Error,
                    "preflight",
                    serde_json::json!({
                        "type": "accessibility-recover-failed",
                        "message": error_text.clone(),
                    }),
                );
            }
            Err(error_text)
        }
    }
}

/// 在监听恢复失败时弹出原生“立即重启”提示，作为权限恢复链路的最终兜底。
#[tauri::command]
pub fn preflight_prompt_restart(app: tauri::AppHandle) -> Value {
    info!("preflight_prompt_restart showing restart dialog");
    let app_handle = app.clone();
    let mut dialog = app
        .dialog()
        .message("权限已恢复，但监听重建失败。请点击“立即重启”后重新进入应用。")
        .title("重启应用")
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCustom("立即重启".into()));

    if let Some(window) = app.get_webview_window("main") {
        dialog = dialog.parent(&window);
    }

    dialog.show(move |confirmed| {
        if confirmed {
            app_handle.request_restart();
        }
    });

    serde_json::json!({
        "ok": true,
        "prompt_shown": true,
    })
}

#[cfg(test)]
mod tests {
    use super::{build_accessibility_request_result, build_preflight_result, preflight_check};

    #[test]
    fn preflight_check_keeps_compatible_fields() {
        let value = preflight_check();
        let obj = value
            .as_object()
            .expect("preflight_check should return json object");
        assert!(obj.contains_key("wechat_running"));
        assert!(obj.contains_key("accessibility_ok"));
        assert!(obj.contains_key("wechat_accessible"));
        assert!(obj.contains_key("wechat_has_window"));
        assert!(obj.contains_key("can_prompt_accessibility"));
    }

    #[test]
    fn preflight_result_should_treat_accessibility_as_process_trust() {
        let value = build_preflight_result(true, true, false, false);
        let obj = value
            .as_object()
            .expect("preflight result should return json object");
        assert_eq!(
            obj.get("accessibility_ok").and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            obj.get("can_prompt_accessibility")
                .and_then(|v| v.as_bool()),
            Some(false)
        );
    }

    #[test]
    fn preflight_result_should_prompt_only_when_untrusted() {
        let value = build_preflight_result(false, false, false, false);
        let obj = value
            .as_object()
            .expect("preflight result should return json object");
        assert_eq!(
            obj.get("accessibility_ok").and_then(|v| v.as_bool()),
            Some(false)
        );
        assert_eq!(
            obj.get("can_prompt_accessibility")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
    }

    #[test]
    fn request_result_should_skip_prompt_if_already_trusted() {
        let result = build_accessibility_request_result(true, || false, || false);
        assert!(result.trusted_before);
        assert!(!result.prompt_attempted);
        assert!(result.trusted_after_check);
    }

    #[test]
    fn request_result_should_attempt_prompt_when_untrusted() {
        let result = build_accessibility_request_result(false, || false, || true);
        assert!(!result.trusted_before);
        assert!(result.prompt_attempted);
        assert!(result.trusted_after_check);
    }
}
