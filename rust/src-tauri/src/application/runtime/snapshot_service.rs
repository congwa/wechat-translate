//! 运行态快照服务：负责把当前运行时状态投影为前端可消费的 snapshot，
//! 让 app_state 只保留 DTO 与事件名称，而不直接依赖 TaskManager。
use crate::app_state::{
    AppRuntimeState, AppStateSnapshot, SettingsStateSnapshot, SnapshotVersionState,
};
use crate::application::runtime::service::RuntimeService;
use crate::config::{load_app_config, AppConfig, ConfigDir};
use crate::CloseToTray;
use tauri::{AppHandle, Emitter, Manager};

/// 构造当前运行态快照，统一收口 tasks、translator 与 close-to-tray 的读模型投影。
pub(crate) async fn runtime_snapshot(
    runtime: &RuntimeService,
    close_to_tray: &CloseToTray,
    versions: &SnapshotVersionState,
) -> AppRuntimeState {
    AppRuntimeState {
        version: versions.current_runtime(),
        tasks: runtime.task_state(),
        translator: runtime.translator_status().await,
        close_to_tray: close_to_tray.0.load(std::sync::atomic::Ordering::Relaxed),
    }
}

/// 把配置快照与运行态快照合成单一 AppStateSnapshot，供前端启动和重新加载使用。
pub(crate) async fn snapshot(
    settings: AppConfig,
    runtime: &RuntimeService,
    close_to_tray: &CloseToTray,
    versions: &SnapshotVersionState,
) -> AppStateSnapshot {
    AppStateSnapshot {
        settings: SettingsStateSnapshot {
            version: versions.current_settings(),
            data: settings,
        },
        runtime: runtime_snapshot(runtime, close_to_tray, versions).await,
    }
}

/// 从配置文件和运行态服务读取当前整包快照，作为前端单次查询入口。
pub(crate) async fn load_snapshot(
    config_dir: &ConfigDir,
    runtime: &RuntimeService,
    close_to_tray: &CloseToTray,
    versions: &SnapshotVersionState,
) -> Result<AppStateSnapshot, String> {
    let settings = load_app_config(&config_dir.0).map_err(|error| error.to_string())?;
    Ok(snapshot(settings, runtime, close_to_tray, versions).await)
}

/// 在同步上下文里读取整包快照；translator 状态暂时降级为 disabled，后续由事件刷新为真实值。
pub(crate) fn load_snapshot_sync(
    config_dir: &ConfigDir,
    runtime: &RuntimeService,
    close_to_tray: &CloseToTray,
    versions: &SnapshotVersionState,
) -> Result<AppStateSnapshot, String> {
    let settings = load_app_config(&config_dir.0).map_err(|error| error.to_string())?;
    Ok(AppStateSnapshot {
        settings: SettingsStateSnapshot {
            version: versions.current_settings(),
            data: settings,
        },
        runtime: AppRuntimeState {
            version: versions.current_runtime(),
            tasks: runtime.task_state(),
            translator: crate::translator::TranslatorServiceStatus::disabled(),
            close_to_tray: close_to_tray.0.load(std::sync::atomic::Ordering::Relaxed),
        },
    })
}

/// 发送最新版 runtime snapshot 事件，让前端只通过 whole snapshot 替换来同步运行态。
pub(crate) fn emit_runtime_updated(app: &AppHandle, runtime: RuntimeService) {
    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        if let (Some(close_to_tray), Some(versions)) = (
            app_clone.try_state::<CloseToTray>(),
            app_clone.try_state::<SnapshotVersionState>(),
        ) {
            let snapshot = AppRuntimeState {
                version: versions.next_runtime(),
                tasks: runtime.task_state(),
                translator: runtime.translator_status().await,
                close_to_tray: close_to_tray.0.load(std::sync::atomic::Ordering::Relaxed),
            };
            let _ = app_clone.emit("runtime-updated", snapshot);
        }
    });
}
