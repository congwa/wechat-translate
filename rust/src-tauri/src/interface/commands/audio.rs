//! 音频命令入口：负责把词典发音缓存相关写操作通过 Tauri 暴露给前端，
//! 让音频缓存链路也逐步从旧 `commands/audio.rs` 的直接注册方式迁移出来。
use crate::audio_cache::{AudioCache, AudioCacheStats};
use std::sync::Arc;

/// 获取音频播放 URL，并在需要时触发后端下载与缓存。
#[tauri::command]
pub async fn audio_get_url(
    audio_cache: tauri::State<'_, Arc<AudioCache>>,
    url: String,
) -> Result<String, String> {
    if url.is_empty() {
        return Err("音频 URL 不能为空".to_string());
    }

    let cache_path = audio_cache
        .get_or_download(&url)
        .await
        .map_err(|e| format!("获取音频失败: {}", e))?;

    Ok(cache_path.display().to_string())
}

/// 检查指定音频资源是否已在本地缓存，供前端决定是否要显示加载态。
#[tauri::command]
pub async fn audio_is_cached(
    audio_cache: tauri::State<'_, Arc<AudioCache>>,
    url: String,
) -> Result<bool, String> {
    Ok(audio_cache.is_cached(&url))
}

/// 返回音频缓存统计，供设置页或调试入口展示当前缓存规模。
#[tauri::command]
pub async fn audio_get_stats(
    audio_cache: tauri::State<'_, Arc<AudioCache>>,
) -> Result<AudioCacheStats, String> {
    audio_cache
        .get_stats()
        .map_err(|e| format!("获取缓存统计失败: {}", e))
}

/// 清空音频缓存，供用户在缓存损坏或调试时重置本地发音资源。
#[tauri::command]
pub async fn audio_clear_cache(
    audio_cache: tauri::State<'_, Arc<AudioCache>>,
) -> Result<u64, String> {
    audio_cache
        .clear()
        .map_err(|e| format!("清空缓存失败: {}", e))
}
