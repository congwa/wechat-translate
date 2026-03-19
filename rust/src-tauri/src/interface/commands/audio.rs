//! 音频命令入口：负责把词典发音缓存相关写操作通过 Tauri 暴露给前端，
//! 让音频缓存链路也逐步从旧 `commands/audio.rs` 的直接注册方式迁移出来。
use crate::audio_cache::{AudioCache, AudioCacheStats};
use crate::commands;
use std::sync::Arc;

/// 获取音频播放 URL，并在需要时触发后端下载与缓存。
#[tauri::command]
pub async fn audio_get_url(
    audio_cache: tauri::State<'_, Arc<AudioCache>>,
    url: String,
) -> Result<String, String> {
    commands::audio::audio_get_url(audio_cache, url).await
}

/// 检查指定音频资源是否已在本地缓存，供前端决定是否要显示加载态。
#[tauri::command]
pub async fn audio_is_cached(
    audio_cache: tauri::State<'_, Arc<AudioCache>>,
    url: String,
) -> Result<bool, String> {
    commands::audio::audio_is_cached(audio_cache, url).await
}

/// 返回音频缓存统计，供设置页或调试入口展示当前缓存规模。
#[tauri::command]
pub async fn audio_get_stats(
    audio_cache: tauri::State<'_, Arc<AudioCache>>,
) -> Result<AudioCacheStats, String> {
    commands::audio::audio_get_stats(audio_cache).await
}

/// 清空音频缓存，供用户在缓存损坏或调试时重置本地发音资源。
#[tauri::command]
pub async fn audio_clear_cache(
    audio_cache: tauri::State<'_, Arc<AudioCache>>,
) -> Result<u64, String> {
    commands::audio::audio_clear_cache(audio_cache).await
}
