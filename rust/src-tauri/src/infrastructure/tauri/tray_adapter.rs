//! 托盘菜单适配器：把运行态快照投影到原生托盘菜单，
//! 避免 TaskManager 自己承担平台 UI 细节更新。
use crate::application::runtime::state::TaskState;
use crate::translator::TranslatorServiceStatus;
use tauri::{AppHandle, Manager};

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
