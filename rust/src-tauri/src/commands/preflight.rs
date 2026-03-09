use core_foundation::base::{CFType, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::CFDictionary;
use log::{debug, info, warn};
use serde::Serialize;
use serde_json::Value;
use std::time::Duration;

const WECHAT_BUNDLE_ID: &str = "com.tencent.xinWeChat";
const ACCESSIBILITY_SETTINGS_URL: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility";

#[derive(Debug, Clone, Serialize)]
pub struct AccessibilityRequestResult {
    trusted_before: bool,
    prompt_attempted: bool,
    trusted_after_check: bool,
    settings_opened: bool,
}

fn check_wechat_pid() -> Option<i32> {
    let output = std::process::Command::new("osascript")
        .args([
            "-e",
            &format!(
                "tell application \"System Events\" to get unix id of (first process whose bundle identifier is \"{}\")",
                WECHAT_BUNDLE_ID
            ),
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<i32>()
        .ok()
}

fn check_accessibility(pid: i32) -> bool {
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

fn is_process_trusted() -> bool {
    unsafe { accessibility_sys::AXIsProcessTrusted() }
}

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

#[tauri::command]
pub fn preflight_check() -> Value {
    let pid = check_wechat_pid();

    let wechat_running = pid.is_some();
    let accessibility_ok = pid.map_or(false, check_accessibility);
    let wechat_has_window = pid.map_or(false, check_has_window);
    debug!(
        "preflight_check wechat_running={} accessibility_ok={} wechat_has_window={}",
        wechat_running, accessibility_ok, wechat_has_window
    );

    serde_json::json!({
        "wechat_running": wechat_running,
        "accessibility_ok": accessibility_ok,
        "wechat_has_window": wechat_has_window,
        "can_prompt_accessibility": wechat_running,
    })
}

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

#[cfg(test)]
mod tests {
    use super::{build_accessibility_request_result, preflight_check};

    #[test]
    fn preflight_check_keeps_compatible_fields() {
        let value = preflight_check();
        let obj = value
            .as_object()
            .expect("preflight_check should return json object");
        assert!(obj.contains_key("wechat_running"));
        assert!(obj.contains_key("accessibility_ok"));
        assert!(obj.contains_key("wechat_has_window"));
        assert!(obj.contains_key("can_prompt_accessibility"));
    }

    #[test]
    fn request_result_should_skip_prompt_if_already_trusted() {
        let result = build_accessibility_request_result(true, || false, || false);
        assert!(result.trusted_before);
        assert!(!result.prompt_attempted);
        assert!(result.trusted_after_check);
        assert!(!result.settings_opened);
    }

    #[test]
    fn request_result_should_attempt_prompt_when_untrusted() {
        let result = build_accessibility_request_result(false, || false, || false);
        assert!(!result.trusted_before);
        assert!(result.prompt_attempted);
        assert!(!result.trusted_after_check);
        assert!(!result.settings_opened);
    }
}
