pub mod adapter;
mod avatar_capture;
mod commands;
mod config;
pub mod db;
mod events;
mod image_cache;
pub mod sidebar_window;
mod task_manager;
pub mod translator;

use adapter::MacOSAdapter;
use avatar_capture::AvatarCache;
use config::ConfigDir;
use db::MessageDb;
use events::EventStore;
use image_cache::WeChatImageCache;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use task_manager::TaskManager;
use tauri::menu::{CheckMenuItemBuilder, MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::Manager;

pub struct CloseToTray(pub Arc<AtomicBool>);

pub struct TrayMenuState {
    pub sidebar_status: tauri::menu::MenuItem<tauri::Wry>,
    pub listen_status: tauri::menu::MenuItem<tauri::Wry>,
    pub sidebar_toggle: tauri::menu::MenuItem<tauri::Wry>,
    pub listen_toggle: tauri::menu::MenuItem<tauri::Wry>,
    pub close_to_tray_check: tauri::menu::CheckMenuItem<tauri::Wry>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let adapter = Arc::new(MacOSAdapter::new());
    let event_store = Arc::new(EventStore::new());
    let close_to_tray = CloseToTray(Arc::new(AtomicBool::new(true)));

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(adapter.clone())
        .manage(event_store.clone())
        .manage(close_to_tray)
        .setup(move |app| {
            let data_dir = app.path().app_data_dir().unwrap_or_else(|_| {
                std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
            });
            let db_path = data_dir.join("messages.db");
            let message_db =
                Arc::new(MessageDb::new(&db_path).expect("failed to open message database"));

            let image_cache = Arc::new(std::sync::Mutex::new(WeChatImageCache::new()));
            let avatar_cache = Arc::new(std::sync::Mutex::new(AvatarCache::new(&data_dir)));

            let manager = TaskManager::new(
                adapter.clone(),
                event_store.clone(),
                message_db.clone(),
                image_cache.clone(),
                avatar_cache.clone(),
            );
            let sidebar_state = sidebar_window::create_state();

            app.manage(ConfigDir(data_dir));
            app.manage(message_db);
            app.manage(image_cache);
            app.manage(avatar_cache);
            app.manage(manager.clone());
            app.manage(sidebar_state);

            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                manager.set_app_handle(handle).await;
                let _ = manager.start_monitoring(1.0).await;
            });

            // -- Tray menu items --
            let title_item = MenuItemBuilder::with_id("title", "WeChat PC Auto")
                .enabled(false)
                .build(app)?;

            let sidebar_status = MenuItemBuilder::with_id("sidebar_status", "○ 浮窗未运行")
                .enabled(false)
                .build(app)?;
            let listen_status = MenuItemBuilder::with_id("listen_status", "○ 监听未运行")
                .enabled(false)
                .build(app)?;

            let sidebar_toggle =
                MenuItemBuilder::with_id("toggle_sidebar", "开启实时浮窗").build(app)?;
            let listen_toggle = MenuItemBuilder::with_id("toggle_listen", "开启监听").build(app)?;

            let show_item = MenuItemBuilder::with_id("show", "显示主窗口").build(app)?;
            let close_to_tray_check =
                CheckMenuItemBuilder::with_id("toggle_close_to_tray", "关闭时最小化到托盘")
                    .checked(true)
                    .build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "退出").build(app)?;

            let menu = MenuBuilder::new(app)
                .item(&title_item)
                .separator()
                .item(&sidebar_status)
                .item(&listen_status)
                .separator()
                .item(&sidebar_toggle)
                .item(&listen_toggle)
                .separator()
                .item(&show_item)
                .item(&close_to_tray_check)
                .separator()
                .item(&quit_item)
                .build()?;

            app.manage(TrayMenuState {
                sidebar_status,
                listen_status,
                sidebar_toggle,
                listen_toggle,
                close_to_tray_check,
            });

            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .tooltip("WeChat PC Auto")
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    "toggle_sidebar" => {
                        let app = app.clone();
                        tauri::async_runtime::spawn(async move {
                            let manager = app.state::<TaskManager>();
                            let state = manager.get_task_state();
                            if state.sidebar {
                                let _ = manager.disable_sidebar().await;
                                let sidebar_ws =
                                    app.state::<Arc<sidebar_window::SidebarWindowState>>();
                                let _ = sidebar_ws.close(&app).await;
                            } else {
                                if !state.monitoring {
                                    let _ = manager.start_monitoring(1.0).await;
                                }
                                let _ = manager
                                    .enable_sidebar(
                                        vec![],
                                        false,
                                        String::new(),
                                        "auto".into(),
                                        "EN".into(),
                                        8.0,
                                        false,
                                        false,
                                    )
                                    .await;
                                let sidebar_ws =
                                    app.state::<Arc<sidebar_window::SidebarWindowState>>();
                                let _ = sidebar_ws
                                    .open(&app, None, sidebar_window::WindowMode::default())
                                    .await;
                            }
                        });
                    }
                    "toggle_listen" => {
                        let app = app.clone();
                        tauri::async_runtime::spawn(async move {
                            let manager = app.state::<TaskManager>();
                            let state = manager.get_task_state();
                            if state.autoreply {
                                let _ = manager.disable_autoreply().await;
                            } else if state.monitoring {
                                manager.stop_all().await;
                            } else {
                                let _ = manager.start_monitoring(1.0).await;
                            }
                        });
                    }
                    "toggle_close_to_tray" => {
                        let close = app.state::<CloseToTray>();
                        let tray = app.state::<TrayMenuState>();
                        let checked = tray.close_to_tray_check.is_checked().unwrap_or(true);
                        close.0.store(checked, Ordering::Relaxed);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let tauri::tray::TrayIconEvent::DoubleClick { .. } = event {
                        if let Some(window) = tray.app_handle().get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app)?;

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    let close_to_tray = window.state::<CloseToTray>();
                    if close_to_tray.0.load(Ordering::Relaxed) {
                        let _ = window.hide();
                        api.prevent_close();
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::send::send_text,
            commands::send::send_files,
            commands::sessions::get_sessions,
            commands::listen::listen_start,
            commands::listen::listen_stop,
            commands::listen::autoreply_start,
            commands::listen::autoreply_stop,
            commands::listen::get_task_status,
            commands::listen::health_check,
            commands::sidebar::sidebar_start,
            commands::sidebar::sidebar_stop,
            commands::sidebar::live_start,
            commands::sidebar::sidebar_window_open,
            commands::sidebar::sidebar_window_close,
            commands::sidebar::translate_test,
            commands::config::config_get,
            commands::config::config_put,
            commands::config::config_default,
            commands::db::db_clear_restart,
            commands::db::db_query_messages,
            commands::db::db_get_chats,
            commands::db::db_get_stats,
            commands::tray::get_close_to_tray,
            commands::tray::set_close_to_tray,
            commands::preflight::preflight_check,
            commands::preflight::accessibility_request_access,
            commands::preflight::accessibility_open_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
