//! 托盘菜单适配器：把运行态快照投影到原生托盘菜单，
//! 避免 TaskManager 自己承担平台 UI 细节更新。
use crate::application::runtime::state::TaskState;
use crate::translator::TranslatorServiceStatus;
use crate::TrayBlinkState;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Manager};
use tokio_util::sync::CancellationToken;

/// 根据当前运行态和翻译健康状态刷新托盘菜单文案。
pub(crate) fn update_tray_menu(
    app: &AppHandle,
    state: &TaskState,
    translator_status: &TranslatorServiceStatus,
) {
    if let Some(tray) = app.try_state::<crate::TrayMenuState>() {
        let _ = tray.sidebar_status.set_text(if state.sidebar {
            "● 浮窗运行中"
        } else {
            "○ 浮窗未运行"
        });
        let _ = tray.sidebar_toggle.set_text(if state.sidebar {
            "关闭浮窗"
        } else {
            "开启实时浮窗"
        });

        let _ = tray.listen_status.set_text(if state.monitoring {
            "● 监听运行中"
        } else {
            "○ 监听未运行"
        });
        let _ = tray.listen_toggle.set_text(if state.monitoring {
            "暂停监听"
        } else {
            "开启监听"
        });
        let _ = tray
            .translate_status
            .set_text(translator_status.menu_text());
    }
}

/// 启动托盘图标闪动：监听异常时调用
pub(crate) async fn start_tray_blink(app: &AppHandle) {
    let Some(blink_state) = app.try_state::<TrayBlinkState>() else {
        return;
    };

    // 如果已经在闪动，不重复启动
    if blink_state.blink_active.load(Ordering::Relaxed) {
        return;
    }

    blink_state.blink_active.store(true, Ordering::Relaxed);

    // 取消之前的闪动任务（如果有）
    if let Ok(mut guard) = blink_state.cancel_token.lock() {
        if let Some(old_token) = guard.take() {
            old_token.cancel();
        }
    }

    let token = CancellationToken::new();
    if let Ok(mut guard) = blink_state.cancel_token.lock() {
        *guard = Some(token.clone());
    }

    // 更新菜单状态提示
    if let Some(tray_menu) = app.try_state::<crate::TrayMenuState>() {
        let _ = tray_menu.listen_status.set_text("⚠ 监听异常");
    }

    let app_handle = app.clone();
    tokio::spawn(async move {
        let mut show_warning = true;
        loop {
            if token.is_cancelled() {
                break;
            }

            let Some(blink_state) = app_handle.try_state::<TrayBlinkState>() else {
                break;
            };

            if let Ok(tray_guard) = blink_state.tray_icon.lock() {
                if let Some(tray) = tray_guard.as_ref() {
                    let icon = if show_warning {
                        blink_state.warning_icon.lock().ok().and_then(|g| g.clone())
                    } else {
                        blink_state.normal_icon.lock().ok().and_then(|g| g.clone())
                    };
                    if let Some(icon) = icon {
                        let _ = tray.set_icon(Some(icon));
                    }
                }
            }

            show_warning = !show_warning;

            tokio::select! {
                _ = token.cancelled() => break,
                _ = tokio::time::sleep(std::time::Duration::from_millis(500)) => {}
            }
        }
    });
}

/// 停止托盘图标闪动：监听恢复正常时调用
pub(crate) async fn stop_tray_blink(app: &AppHandle) {
    let Some(blink_state) = app.try_state::<TrayBlinkState>() else {
        return;
    };

    // 如果没有在闪动，直接返回
    if !blink_state.blink_active.load(Ordering::Relaxed) {
        return;
    }

    blink_state.blink_active.store(false, Ordering::Relaxed);

    // 取消闪动任务
    if let Ok(mut guard) = blink_state.cancel_token.lock() {
        if let Some(token) = guard.take() {
            token.cancel();
        }
    }

    // 恢复正常图标
    let normal_icon = blink_state.normal_icon.lock().ok().and_then(|g| g.clone());
    if let Ok(tray_guard) = blink_state.tray_icon.lock() {
        if let Some(tray) = tray_guard.as_ref() {
            if let Some(icon) = normal_icon {
                let _ = tray.set_icon(Some(icon));
            }
        }
    };
}
