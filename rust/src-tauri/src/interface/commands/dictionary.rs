//! 词典命令入口：负责把查词、翻译缓存、收藏和复习这组词典写操作通过 Tauri 暴露给前端，
//! 让词典子域的对外入口真正统一到 `interface/commands/dictionary.rs`。
use crate::application::runtime::service::RuntimeService;
use crate::config::{load_app_config, ConfigDir};
use crate::dictionary::db::hash_text;
use crate::dictionary::{
    DictionaryDb, DictionaryRouter, FavoriteWord, ProviderInfo, ReviewSession, ReviewStats,
    TranslationWorker, WordEntry,
};
use crate::task_manager::TaskManager;
use crate::translator::TranslationService;
use std::collections::HashMap;
use std::sync::Arc;
use tauri::AppHandle;

/// 查询单词详情，并在需要时异步触发释义翻译。
/// 业务上遵循“能用缓存就立即返回，查到新词就立即落库，再后台补翻译”的流程，避免前端等待网络后才看到结果。
#[tauri::command]
pub async fn word_lookup(
    app_handle: AppHandle,
    config_dir: tauri::State<'_, ConfigDir>,
    dict_db: tauri::State<'_, Arc<DictionaryDb>>,
    dict_router: tauri::State<'_, Arc<DictionaryRouter>>,
    translation_service: tauri::State<'_, Arc<TranslationService>>,
    word: String,
    provider: Option<String>,
) -> Result<WordEntry, String> {
    let word = word.to_lowercase().trim().to_string();
    if word.is_empty() {
        return Err("Word cannot be empty".to_string());
    }

    let provider_id =
        provider.or_else(|| load_app_config(&config_dir.0).ok().map(|c| c.dict.provider));

    if let Ok(Some(cached)) = dict_db.get_word(&word) {
        let cache_matches_provider = provider_id
            .as_ref()
            .map(|p| cached.data_source == *p)
            .unwrap_or(true);

        if cache_matches_provider && cached.translation_completed {
            return Ok(cached);
        }

        if cache_matches_provider {
            spawn_translation_task(
                &app_handle,
                &dict_db,
                &translation_service,
                &word,
                cached.clone(),
            )
            .await;
            return Ok(cached);
        }
    }

    let entry = dict_router
        .lookup(&word, provider_id.as_deref())
        .await
        .map_err(|e| e.to_string())?;

    dict_db
        .upsert_word(&word, &entry)
        .map_err(|e| e.to_string())?;

    spawn_translation_task(
        &app_handle,
        &dict_db,
        &translation_service,
        &word,
        entry.clone(),
    )
    .await;

    Ok(entry)
}

/// 返回当前可用的词典提供者列表，供设置页和查词 UI 选择来源。
#[tauri::command]
pub async fn list_dict_providers(
    dict_router: tauri::State<'_, Arc<DictionaryRouter>>,
) -> Result<Vec<ProviderInfo>, String> {
    Ok(dict_router.list_providers())
}

/// 返回当前配置中的默认词典提供者。
#[tauri::command]
pub async fn get_dict_provider(config_dir: tauri::State<'_, ConfigDir>) -> Result<String, String> {
    let config = load_app_config(&config_dir.0).map_err(|e| e.to_string())?;
    Ok(config.dict.provider)
}

/// 启动异步翻译任务，把英文释义补成中英双语结果。
/// 业务上这一步是“后台补全”，不会阻塞 `word_lookup` 的首屏返回。
async fn spawn_translation_task(
    app_handle: &AppHandle,
    dict_db: &Arc<DictionaryDb>,
    translation_service: &Arc<crate::translator::TranslationService>,
    word: &str,
    entry: WordEntry,
) {
    if !translation_service.is_available().await {
        return;
    }

    let worker = TranslationWorker::new(
        app_handle.clone(),
        dict_db.clone(),
        translation_service.clone(),
    );
    worker.spawn_translation(word.to_string(), entry);
}

/// 翻译一段文本并优先复用词典翻译缓存。
/// 业务上这是词典侧的“轻量翻译能力”，与 sidebar 消息翻译链路隔离，但复用同一份运行态翻译配置。
#[tauri::command]
pub async fn translate_cached(
    dict_db: tauri::State<'_, Arc<DictionaryDb>>,
    manager: tauri::State<'_, TaskManager>,
    text: String,
    source_lang: Option<String>,
    target_lang: Option<String>,
) -> Result<String, String> {
    let runtime = RuntimeService::new(manager.inner().clone());
    let source_lang = source_lang.unwrap_or_else(|| "en".to_string());
    let target_lang = target_lang.unwrap_or_else(|| "zh".to_string());
    let hash = hash_text(&text);

    if let Ok(Some(cached)) = dict_db.get_translation(&hash, &source_lang, &target_lang) {
        return Ok(cached);
    }

    let translation_service = runtime.translation_service();
    if !translation_service.is_available().await {
        return Err("Translator not configured".to_string());
    }

    let translated = translation_service
        .translate_with_langs(&text, &source_lang, &target_lang)
        .await
        .map_err(|e| e.to_string())?;

    dict_db
        .insert_translation(&text, &hash, &translated, &source_lang, &target_lang)
        .map_err(|e| e.to_string())?;

    Ok(translated)
}

/// 批量翻译多段释义文本，供词典卡片一次性补全中文释义。
#[tauri::command]
pub async fn translate_batch(
    dict_db: tauri::State<'_, Arc<DictionaryDb>>,
    manager: tauri::State<'_, TaskManager>,
    texts: Vec<String>,
    source_lang: Option<String>,
    target_lang: Option<String>,
) -> Result<Vec<Option<String>>, String> {
    let runtime = RuntimeService::new(manager.inner().clone());
    let source_lang = source_lang.unwrap_or_else(|| "en".to_string());
    let target_lang = target_lang.unwrap_or_else(|| "zh".to_string());
    let translation_service = runtime.translation_service();

    let mut results = Vec::with_capacity(texts.len());

    for text in texts {
        let hash = hash_text(&text);

        if let Ok(Some(cached)) = dict_db.get_translation(&hash, &source_lang, &target_lang) {
            results.push(Some(cached));
            continue;
        }

        if translation_service.is_available().await {
            match translation_service
                .translate_with_langs(&text, &source_lang, &target_lang)
                .await
            {
                Ok(translated) => {
                    let _ = dict_db.insert_translation(
                        &text,
                        &hash,
                        &translated,
                        &source_lang,
                        &target_lang,
                    );
                    results.push(Some(translated));
                }
                Err(_) => results.push(None),
            }
        } else {
            results.push(None);
        }
    }

    Ok(results)
}

/// 切换单词收藏状态，作为词典收藏夹的单一写入口。
#[tauri::command]
pub async fn toggle_favorite(
    dict_db: tauri::State<'_, Arc<DictionaryDb>>,
    word: String,
    entry: Option<WordEntry>,
) -> Result<bool, String> {
    let word = word.to_lowercase().trim().to_string();
    if word.is_empty() {
        return Err("Word cannot be empty".to_string());
    }

    let is_favorited = dict_db.is_favorited(&word).map_err(|e| e.to_string())?;

    if is_favorited {
        dict_db.remove_favorite(&word).map_err(|e| e.to_string())?;
        Ok(false)
    } else {
        dict_db
            .add_favorite(&word, entry.as_ref())
            .map_err(|e| e.to_string())?;
        Ok(true)
    }
}

/// 查询某个单词当前是否已被收藏。
#[tauri::command]
pub async fn is_word_favorited(
    dict_db: tauri::State<'_, Arc<DictionaryDb>>,
    word: String,
) -> Result<bool, String> {
    dict_db.is_favorited(&word).map_err(|e| e.to_string())
}

/// 批量查询收藏状态，供词典与消息分词 UI 一次性渲染收藏标记。
#[tauri::command]
pub async fn get_favorites_batch(
    dict_db: tauri::State<'_, Arc<DictionaryDb>>,
    words: Vec<String>,
) -> Result<HashMap<String, bool>, String> {
    let results = dict_db
        .get_favorites_batch(&words)
        .map_err(|e| e.to_string())?;

    Ok(results.into_iter().collect())
}

/// 分页返回收藏列表，供收藏页展示历史收藏单词。
#[tauri::command]
pub async fn list_favorites(
    dict_db: tauri::State<'_, Arc<DictionaryDb>>,
    offset: Option<u32>,
    limit: Option<u32>,
) -> Result<Vec<FavoriteWord>, String> {
    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(50);

    dict_db
        .list_favorites(offset, limit)
        .map_err(|e| e.to_string())
}

/// 更新收藏笔记，作为收藏附加说明的写入口。
#[tauri::command]
pub async fn update_favorite_note(
    dict_db: tauri::State<'_, Arc<DictionaryDb>>,
    word: String,
    note: String,
) -> Result<bool, String> {
    dict_db
        .update_favorite_note(&word, &note)
        .map_err(|e| e.to_string())
}

/// 记录一次简化复习，用于无会话上下文下的单词记忆更新。
#[tauri::command]
pub async fn record_review(
    dict_db: tauri::State<'_, Arc<DictionaryDb>>,
    word: String,
) -> Result<bool, String> {
    dict_db.record_review(&word).map_err(|e| e.to_string())
}

/// 返回收藏总数，供词典页快速展示当前收藏规模。
#[tauri::command]
pub async fn count_favorites(
    dict_db: tauri::State<'_, Arc<DictionaryDb>>,
) -> Result<u32, String> {
    dict_db.count_favorites().map_err(|e| e.to_string())
}

/// 返回待复习单词列表，供复习模式按优先级抽取单词。
#[tauri::command]
pub async fn get_words_for_review(
    dict_db: tauri::State<'_, Arc<DictionaryDb>>,
    limit: Option<u32>,
) -> Result<Vec<FavoriteWord>, String> {
    let limit = limit.unwrap_or(20);
    dict_db
        .get_words_for_review(limit)
        .map_err(|e| e.to_string())
}

/// 创建一轮复习会话，供前端开始一组结构化复习流程。
#[tauri::command]
pub async fn start_review_session(
    dict_db: tauri::State<'_, Arc<DictionaryDb>>,
    mode: String,
    word_count: i32,
) -> Result<i64, String> {
    dict_db
        .start_review_session(&mode, word_count)
        .map_err(|e| e.to_string())
}

/// 记录某个单词在复习会话中的反馈结果。
#[tauri::command]
pub async fn record_review_feedback(
    dict_db: tauri::State<'_, Arc<DictionaryDb>>,
    session_id: i64,
    word: String,
    feedback: i32,
    response_time_ms: Option<i32>,
) -> Result<FavoriteWord, String> {
    dict_db
        .record_review_feedback(session_id, &word, feedback, response_time_ms.unwrap_or(0))
        .map_err(|e| e.to_string())
}

/// 结束一轮复习会话并返回会话统计结果。
#[tauri::command]
pub async fn finish_review_session(
    dict_db: tauri::State<'_, Arc<DictionaryDb>>,
    session_id: i64,
) -> Result<ReviewSession, String> {
    dict_db
        .finish_review_session(session_id)
        .map_err(|e| e.to_string())
}

/// 返回复习统计，用于词典页展示长期学习进度。
#[tauri::command]
pub async fn get_review_stats(
    dict_db: tauri::State<'_, Arc<DictionaryDb>>,
) -> Result<ReviewStats, String> {
    dict_db.get_review_stats().map_err(|e| e.to_string())
}
