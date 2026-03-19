//! 会话查询入口：负责把当前微信会话列表通过 Tauri 暴露给前端，
//! 让会话读取也落在 `interface/queries` 而不是继续注册在旧命令层。
use crate::adapter::MacOSAdapter;
use std::sync::Arc;

/// 返回当前微信可见会话列表，供前端会话选择器和诊断入口读取当前会话状态。
#[tauri::command]
pub async fn get_sessions(
    adapter: tauri::State<'_, Arc<MacOSAdapter>>,
) -> Result<serde_json::Value, String> {
    let adapter = adapter.inner().clone();
    let sessions = tokio::task::spawn_blocking(move || adapter.get_current_sessions())
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;

    Ok(serde_json::json!({ "ok": true, "data": sessions }))
}
