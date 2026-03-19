//! 音频命令兼容实现：保留音频缓存下载与统计的内部实现，
//! 真正的 Tauri 暴露入口已经迁移到 `interface/commands/audio.rs`。

use crate::audio_cache::{AudioCache, AudioCacheStats};
use std::sync::Arc;

/// 获取音频播放 URL（自动缓存）
///
/// 前端调用此命令获取可播放的音频 URL。
/// 如果音频已缓存，返回本地文件路径；否则下载并缓存后返回。
///
/// # 参数
/// - `url`: 远程音频 URL（如 Cambridge 词典的 MP3 链接）
///
/// # 返回
/// - 可用于 HTML Audio 元素的 URL（本地文件协议）
/// 获取音频播放 URL，并在需要时触发后端下载与缓存。
pub async fn audio_get_url(
    audio_cache: tauri::State<'_, Arc<AudioCache>>,
    url: String,
) -> Result<String, String> {
    println!("[audio_get_url] 收到请求: {}", url);

    // 验证 URL
    if url.is_empty() {
        return Err("音频 URL 不能为空".to_string());
    }

    // 获取或下载音频
    let cache_path = audio_cache.get_or_download(&url).await.map_err(|e| {
        println!("[audio_get_url] 获取失败: {}", e);
        format!("获取音频失败: {}", e)
    })?;

    let result = cache_path.display().to_string();
    println!("[audio_get_url] 返回路径: {}", result);

    // 返回纯文件路径，前端使用 convertFileSrc 转换为 asset:// 协议
    Ok(result)
}

/// 检查音频是否已缓存
///
/// # 参数
/// - `url`: 远程音频 URL
///
/// # 返回
/// - true: 已缓存，false: 未缓存
/// 检查指定音频资源是否已缓存，供前端决定是否需要下载态。
pub async fn audio_is_cached(
    audio_cache: tauri::State<'_, Arc<AudioCache>>,
    url: String,
) -> Result<bool, String> {
    Ok(audio_cache.is_cached(&url))
}

/// 获取音频缓存统计信息
///
/// # 返回
/// - 缓存文件数量、总大小等统计信息
/// 获取音频缓存统计，供设置页或调试入口展示当前缓存规模。
pub async fn audio_get_stats(
    audio_cache: tauri::State<'_, Arc<AudioCache>>,
) -> Result<AudioCacheStats, String> {
    audio_cache
        .get_stats()
        .map_err(|e| format!("获取缓存统计失败: {}", e))
}

/// 清空音频缓存
///
/// # 返回
/// - 清理的文件数量
/// 清空音频缓存，供用户在调试或缓存损坏时重置本地发音资源。
pub async fn audio_clear_cache(
    audio_cache: tauri::State<'_, Arc<AudioCache>>,
) -> Result<u64, String> {
    audio_cache
        .clear()
        .map_err(|e| format!("清空缓存失败: {}", e))
}
