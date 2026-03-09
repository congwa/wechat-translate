use anyhow::{Context, Result};
use core_foundation::base::TCFType;
use core_foundation::string::CFString;
use std::process::Command;

use super::ax_reader::WECHAT_BUNDLE_ID;

extern "C" {
    fn CGWindowListCopyWindowInfo(
        option: u32,
        relative_to_window: u32,
    ) -> core_foundation_sys::array::CFArrayRef;
}

const CG_WINDOW_LIST_ON_SCREEN_ONLY: u32 = 1 << 0;
const CG_WINDOW_LIST_EXCLUDE_DESKTOP: u32 = 1 << 4;
const CG_NULL_WINDOW_ID: u32 = 0;
const WECHAT_PROCESS_NAMES: &[&str] = &["WeChat", "微信", "Weixin"];

pub fn run_osascript(script: &str) -> Result<String> {
    let output = Command::new("osascript")
        .args(["-e", script])
        .output()
        .context("failed to run osascript")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        anyhow::bail!("osascript failed: {}", stderr);
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn escape_applescript_text(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

pub fn activate_wechat() -> Result<()> {
    let script = format!("tell application id \"{}\" to activate", WECHAT_BUNDLE_ID);
    run_osascript(&script)?;
    Ok(())
}

pub fn open_chat_by_search(who: &str) -> Result<()> {
    if who.is_empty() {
        return Ok(());
    }
    let target = escape_applescript_text(who);
    let script = format!(
        r#"
        tell application id "{bundle}" to activate
        delay 0.2
        tell application "System Events"
            keystroke "f" using {{command down}}
            delay 0.3
            keystroke "a" using {{command down}}
            delay 0.05
            keystroke "{target}"
            delay 0.5
            key code 36
            delay 0.4
        end tell
        "#,
        bundle = WECHAT_BUNDLE_ID,
        target = target,
    );
    run_osascript(&script)?;
    Ok(())
}

pub fn copy_text(text: &str) -> Result<()> {
    let mut child = Command::new("pbcopy")
        .stdin(std::process::Stdio::piped())
        .spawn()
        .context("failed to spawn pbcopy")?;

    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        stdin.write_all(text.as_bytes())?;
    }
    child.wait()?;
    Ok(())
}

pub fn copy_file(file_path: &str) -> Result<()> {
    let escaped = escape_applescript_text(file_path);
    let script = format!(
        r#"
        set targetFile to POSIX file "{}"
        set the clipboard to targetFile
        "#,
        escaped
    );
    run_osascript(&script)?;
    Ok(())
}

pub fn paste_and_send(with_enter: bool) -> Result<()> {
    let send_stmt = if with_enter { "key code 36" } else { "" };
    let script = format!(
        r#"
        tell application "System Events"
            keystroke "v" using {{command down}}
            delay 0.05
            {}
        end tell
        "#,
        send_stmt
    );
    run_osascript(&script)?;
    Ok(())
}

pub fn press_enter() -> Result<()> {
    let script = r#"
        tell application "System Events"
            key code 36
        end tell
    "#;
    run_osascript(script)?;
    Ok(())
}

/// 主窗口最小尺寸阈值（像素）。
/// 小于此尺寸的窗口视为辅助窗口（通知徽章、菜单栏图标等），不作为主窗口。
const MIN_MAIN_WINDOW_SIZE: f64 = 200.0;

/// 从 CGWindowBounds 子字典中读取一个 f64 数值。
///
/// kCGWindowBounds 是一个包含 "X"/"Y"/"Width"/"Height" 键的 CFDictionary，
/// 每个值是 CFNumber 类型。
unsafe fn cg_dict_get_f64(
    dict: core_foundation_sys::dictionary::CFDictionaryRef,
    key: &str,
) -> Option<f64> {
    let cf_key = CFString::new(key);
    let ptr = core_foundation_sys::dictionary::CFDictionaryGetValue(
        dict,
        cf_key.as_concrete_TypeRef() as *const std::ffi::c_void,
    );
    if ptr.is_null() {
        return None;
    }
    let mut value: f64 = 0.0;
    let ok = core_foundation_sys::number::CFNumberGetValue(
        ptr as core_foundation_sys::number::CFNumberRef,
        core_foundation_sys::number::kCFNumberFloat64Type,
        &mut value as *mut f64 as *mut std::ffi::c_void,
    );
    if ok {
        Some(value)
    } else {
        None
    }
}

/// 通过 CGWindowList 查找微信主窗口的 frame（x, y, width, height）。
///
/// 为什么不用 AppleScript 的 `window 1`：
/// - `window 1` 是 z-order 最前的窗口，弹窗（对话框、更新提示、文件传输确认等）
///   出现时会变成 `window 1`，导致侧边栏跟着弹窗跳位。
///
/// 为什么需要尺寸过滤：
/// - 微信进程可能有小型辅助窗口（通知徽章、菜单栏图标窗口等）留在屏幕上，
///   主窗口通过 Dock 图标隐藏后这些小窗口仍然存在，
///   旧版 `is_wechat_on_screen` 会误判微信仍可见，导致侧边栏不消失。
///
/// 策略：
/// 1. 使用 kCGWindowListOptionOnScreenOnly 只获取屏幕上可见的窗口
/// 2. 过滤条件：owner 匹配微信进程名 + layer == 0 + 宽高 >= 200px
/// 3. 取面积最大的窗口作为主窗口（主窗口一定比弹窗和辅助窗口大）
/// 4. 找不到时返回 None（微信已隐藏/最小化/无主窗口）
fn find_main_wechat_window_cg() -> Option<(f64, f64, f64, f64)> {
    unsafe {
        let options = CG_WINDOW_LIST_ON_SCREEN_ONLY | CG_WINDOW_LIST_EXCLUDE_DESKTOP;
        let window_list = CGWindowListCopyWindowInfo(options, CG_NULL_WINDOW_ID);
        if window_list.is_null() {
            return None;
        }

        let count = core_foundation_sys::array::CFArrayGetCount(window_list);
        let owner_key = CFString::new("kCGWindowOwnerName");
        let layer_key = CFString::new("kCGWindowLayer");
        let bounds_key = CFString::new("kCGWindowBounds");

        // (x, y, w, h, area) — 记录当前面积最大的候选窗口
        let mut best: Option<(f64, f64, f64, f64, f64)> = None;

        for i in 0..count {
            let dict = core_foundation_sys::array::CFArrayGetValueAtIndex(window_list, i)
                as core_foundation_sys::dictionary::CFDictionaryRef;

            // 检查进程名是否匹配微信（WeChat / 微信 / Weixin）
            let name_ptr = core_foundation_sys::dictionary::CFDictionaryGetValue(
                dict,
                owner_key.as_concrete_TypeRef() as *const std::ffi::c_void,
            );
            if name_ptr.is_null() {
                continue;
            }
            let name_cf =
                CFString::wrap_under_get_rule(name_ptr as core_foundation_sys::string::CFStringRef);
            let name = name_cf.to_string();
            if !WECHAT_PROCESS_NAMES.contains(&name.as_str()) {
                continue;
            }

            // 只看 layer == 0 的普通窗口（排除菜单、Dock 图标等系统层级窗口）
            let layer_ptr = core_foundation_sys::dictionary::CFDictionaryGetValue(
                dict,
                layer_key.as_concrete_TypeRef() as *const std::ffi::c_void,
            );
            if !layer_ptr.is_null() {
                let mut layer: i32 = 0;
                core_foundation_sys::number::CFNumberGetValue(
                    layer_ptr as core_foundation_sys::number::CFNumberRef,
                    core_foundation_sys::number::kCFNumberSInt32Type,
                    &mut layer as *mut i32 as *mut std::ffi::c_void,
                );
                if layer != 0 {
                    continue;
                }
            }

            // 读取窗口 bounds（kCGWindowBounds 是包含 X/Y/Width/Height 的子字典）
            let bounds_ptr = core_foundation_sys::dictionary::CFDictionaryGetValue(
                dict,
                bounds_key.as_concrete_TypeRef() as *const std::ffi::c_void,
            );
            if bounds_ptr.is_null() {
                continue;
            }
            let bounds_dict = bounds_ptr as core_foundation_sys::dictionary::CFDictionaryRef;

            let (x, y, w, h) = match (
                cg_dict_get_f64(bounds_dict, "X"),
                cg_dict_get_f64(bounds_dict, "Y"),
                cg_dict_get_f64(bounds_dict, "Width"),
                cg_dict_get_f64(bounds_dict, "Height"),
            ) {
                (Some(x), Some(y), Some(w), Some(h)) => (x, y, w, h),
                _ => continue,
            };

            // 尺寸过滤：排除通知徽章、菜单栏辅助等小型窗口
            if w < MIN_MAIN_WINDOW_SIZE || h < MIN_MAIN_WINDOW_SIZE {
                continue;
            }

            // 取面积最大的窗口作为主窗口
            let area = w * h;
            if best.as_ref().map_or(true, |b| area > b.4) {
                best = Some((x, y, w, h, area));
            }
        }

        core_foundation_sys::base::CFRelease(window_list as _);
        best.map(|(x, y, w, h, _)| (x, y, w, h))
    }
}

/// 检查微信进程可见性并获取主窗口 frame。
///
/// 返回值：
/// - `Ok(Some(frame))`：微信主窗口可见且在屏幕上
/// - `Ok(None)`：微信已隐藏、最小化、或无主窗口在屏幕上
/// - `Err`：微信进程未运行
///
/// 检测分两步：
/// 1. AppleScript 检查 `visible of process`（快速排除 Cmd+H / Dock 右键隐藏）
/// 2. CGWindowList 查找面积最大的主窗口 frame（避免弹窗干扰和辅助窗口误判）
pub fn query_wechat_window() -> Result<Option<(f64, f64, f64, f64)>> {
    // 第一步：检查进程是否可见（覆盖 Cmd+H / Dock 右键"隐藏"场景）
    let script = format!(
        r#"tell application "System Events"
            set wProc to first process whose bundle identifier is "{bid}"
            if not (visible of wProc) then return "hidden"
            return "visible"
        end tell"#,
        bid = WECHAT_BUNDLE_ID
    );
    let output = run_osascript(&script)?;
    if output.trim() == "hidden" {
        return Ok(None);
    }

    // 第二步：通过 CGWindowList 查找主窗口 frame
    // 不再使用 AppleScript 的 `window 1`（弹窗出现时 window 1 会指向弹窗）
    // 同时通过尺寸过滤排除辅助小窗口的干扰
    Ok(find_main_wechat_window_cg())
}

/// 检查微信是否为当前前台应用。
pub fn is_wechat_frontmost() -> bool {
    let script = r#"tell application "System Events" to get bundle identifier of first process whose frontmost is true"#;
    match run_osascript(script) {
        Ok(bid) => bid.trim() == WECHAT_BUNDLE_ID,
        Err(_) => false,
    }
}

/// 获取微信主窗口的 frame（用于侧边栏初始定位）。
///
/// 通过 CGWindowList 查找屏幕上面积最大的微信窗口，
/// 避免 AppleScript `window 1` 在有弹窗时返回弹窗 frame 的问题。
pub fn get_wechat_window_frame() -> Result<(f64, f64, f64, f64)> {
    find_main_wechat_window_cg().context("未找到微信主窗口（可能未打开或已最小化）")
}
