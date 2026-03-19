//! 词典命令入口：负责把查词、翻译缓存、收藏和复习这组词典写操作通过 Tauri 暴露给前端，
//! 让词典子域的对外入口也逐步统一到 `interface/commands/dictionary.rs`。
use crate::commands;
use crate::config::ConfigDir;
use crate::dictionary::{
    DictionaryDb, DictionaryRouter, FavoriteWord, ProviderInfo, ReviewSession, ReviewStats,
    WordEntry,
};
use crate::task_manager::TaskManager;
use crate::translator::TranslationService;
use std::collections::HashMap;
use std::sync::Arc;
use tauri::AppHandle;

/// 查询单词详情，并在需要时异步触发释义翻译。
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
    commands::dictionary::word_lookup(
        app_handle,
        config_dir,
        dict_db,
        dict_router,
        translation_service,
        word,
        provider,
    )
    .await
}

/// 返回当前可用的词典提供者列表，供设置页和查词 UI 选择来源。
#[tauri::command]
pub async fn list_dict_providers(
    dict_router: tauri::State<'_, Arc<DictionaryRouter>>,
) -> Result<Vec<ProviderInfo>, String> {
    commands::dictionary::list_dict_providers(dict_router).await
}

/// 返回当前配置中的默认词典提供者。
#[tauri::command]
pub async fn get_dict_provider(config_dir: tauri::State<'_, ConfigDir>) -> Result<String, String> {
    commands::dictionary::get_dict_provider(config_dir).await
}

/// 翻译一段文本并优先复用词典翻译缓存。
#[tauri::command]
pub async fn translate_cached(
    dict_db: tauri::State<'_, Arc<DictionaryDb>>,
    manager: tauri::State<'_, TaskManager>,
    text: String,
    source_lang: Option<String>,
    target_lang: Option<String>,
) -> Result<String, String> {
    commands::dictionary::translate_cached(dict_db, manager, text, source_lang, target_lang).await
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
    commands::dictionary::translate_batch(dict_db, manager, texts, source_lang, target_lang).await
}

/// 切换单词收藏状态，作为词典收藏夹的单一写入口。
#[tauri::command]
pub async fn toggle_favorite(
    dict_db: tauri::State<'_, Arc<DictionaryDb>>,
    word: String,
    entry: Option<WordEntry>,
) -> Result<bool, String> {
    commands::dictionary::toggle_favorite(dict_db, word, entry).await
}

/// 查询某个单词当前是否已被收藏。
#[tauri::command]
pub async fn is_word_favorited(
    dict_db: tauri::State<'_, Arc<DictionaryDb>>,
    word: String,
) -> Result<bool, String> {
    commands::dictionary::is_word_favorited(dict_db, word).await
}

/// 批量查询收藏状态，供消息分词和单词本批量渲染收藏标记。
#[tauri::command]
pub async fn get_favorites_batch(
    dict_db: tauri::State<'_, Arc<DictionaryDb>>,
    words: Vec<String>,
) -> Result<HashMap<String, bool>, String> {
    commands::dictionary::get_favorites_batch(dict_db, words).await
}

/// 分页返回收藏列表，供词典收藏页展示历史收藏单词。
#[tauri::command]
pub async fn list_favorites(
    dict_db: tauri::State<'_, Arc<DictionaryDb>>,
    offset: Option<u32>,
    limit: Option<u32>,
) -> Result<Vec<FavoriteWord>, String> {
    commands::dictionary::list_favorites(dict_db, offset, limit).await
}

/// 更新收藏笔记，作为单词收藏附加笔记的写入口。
#[tauri::command]
pub async fn update_favorite_note(
    dict_db: tauri::State<'_, Arc<DictionaryDb>>,
    word: String,
    note: String,
) -> Result<bool, String> {
    commands::dictionary::update_favorite_note(dict_db, word, note).await
}

/// 记录一次简化复习，用于无会话上下文下的复习记忆更新。
#[tauri::command]
pub async fn record_review(
    dict_db: tauri::State<'_, Arc<DictionaryDb>>,
    word: String,
) -> Result<bool, String> {
    commands::dictionary::record_review(dict_db, word).await
}

/// 返回收藏总数，供词典页快速展示当前收藏规模。
#[tauri::command]
pub async fn count_favorites(dict_db: tauri::State<'_, Arc<DictionaryDb>>) -> Result<u32, String> {
    commands::dictionary::count_favorites(dict_db).await
}

/// 返回待复习单词列表，供复习模式按优先级抽取单词。
#[tauri::command]
pub async fn get_words_for_review(
    dict_db: tauri::State<'_, Arc<DictionaryDb>>,
    limit: Option<u32>,
) -> Result<Vec<FavoriteWord>, String> {
    commands::dictionary::get_words_for_review(dict_db, limit).await
}

/// 创建一轮复习会话，供前端开始一组结构化复习流程。
#[tauri::command]
pub async fn start_review_session(
    dict_db: tauri::State<'_, Arc<DictionaryDb>>,
    mode: String,
    word_count: i32,
) -> Result<i64, String> {
    commands::dictionary::start_review_session(dict_db, mode, word_count).await
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
    commands::dictionary::record_review_feedback(
        dict_db,
        session_id,
        word,
        feedback,
        response_time_ms,
    )
    .await
}

/// 结束一轮复习会话并返回会话统计结果。
#[tauri::command]
pub async fn finish_review_session(
    dict_db: tauri::State<'_, Arc<DictionaryDb>>,
    session_id: i64,
) -> Result<ReviewSession, String> {
    commands::dictionary::finish_review_session(dict_db, session_id).await
}

/// 返回复习统计，用于词典页展示长期学习进度。
#[tauri::command]
pub async fn get_review_stats(
    dict_db: tauri::State<'_, Arc<DictionaryDb>>,
) -> Result<ReviewStats, String> {
    commands::dictionary::get_review_stats(dict_db).await
}
