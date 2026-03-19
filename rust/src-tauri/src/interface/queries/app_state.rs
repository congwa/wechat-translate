//! 应用快照查询入口：负责把 settings/runtime 整包快照通过 Tauri 暴露给前端，
//! 让前端启动和重新加载都走同一份 whole snapshot 真相源。
use crate::app_state;
use crate::application::runtime::service::RuntimeService;
use crate::config::ConfigDir;
use crate::task_manager::TaskManager;
use crate::CloseToTray;

/// 返回当前应用整包快照，供前端初始化时一次性拿到 settings/runtime 最新状态。
#[tauri::command]
pub async fn app_state_get(
    config_dir: tauri::State<'_, ConfigDir>,
    manager: tauri::State<'_, TaskManager>,
    close_to_tray: tauri::State<'_, CloseToTray>,
    versions: tauri::State<'_, app_state::SnapshotVersionState>,
) -> Result<serde_json::Value, String> {
    let runtime = RuntimeService::new(manager.inner().clone());
    let snapshot =
        app_state::load_snapshot(&config_dir, &runtime, &close_to_tray, &versions).await?;
    Ok(serde_json::json!({ "ok": true, "data": snapshot }))
}
