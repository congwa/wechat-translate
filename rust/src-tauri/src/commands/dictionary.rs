use crate::dictionary::{api::DictionaryApiClient, db::hash_text, DictionaryDb, WordEntry};
use crate::task_manager::TaskManager;
use std::sync::Arc;

/// 查询单词（带缓存）
#[tauri::command]
pub async fn word_lookup(
    dict_db: tauri::State<'_, Arc<DictionaryDb>>,
    word: String,
) -> Result<WordEntry, String> {
    let word = word.to_lowercase().trim().to_string();
    if word.is_empty() {
        return Err("Word cannot be empty".to_string());
    }

    // 1. 检查数据库缓存
    if let Ok(Some(cached)) = dict_db.get_word(&word) {
        return Ok(cached);
    }

    // 2. 调用 freeDictionaryAPI
    let client = DictionaryApiClient::new().map_err(|e| e.to_string())?;
    let entry = client.lookup(&word).await.map_err(|e| e.to_string())?;

    // 3. 存入数据库
    dict_db
        .upsert_word(&word, &entry)
        .map_err(|e| e.to_string())?;

    Ok(entry)
}

/// 翻译文本（带缓存）
/// source_lang 和 target_lang 用于指定翻译语言对
#[tauri::command]
pub async fn translate_cached(
    dict_db: tauri::State<'_, Arc<DictionaryDb>>,
    manager: tauri::State<'_, TaskManager>,
    text: String,
    source_lang: Option<String>,
    target_lang: Option<String>,
) -> Result<String, String> {
    let source_lang = source_lang.unwrap_or_else(|| "en".to_string());
    let target_lang = target_lang.unwrap_or_else(|| "zh".to_string());
    let hash = hash_text(&text);

    // 1. 检查数据库缓存
    if let Ok(Some(cached)) = dict_db.get_translation(&hash, &source_lang, &target_lang) {
        return Ok(cached);
    }

    // 2. 获取翻译器
    let translator = manager
        .get_translator()
        .await
        .ok_or_else(|| "Translator not configured".to_string())?;

    // 3. 调用 DeepLX 翻译（使用传入的语言对）
    let translated = translator
        .translate_with_langs(&text, &source_lang, &target_lang)
        .await
        .map_err(|e| e.to_string())?;

    // 4. 存入数据库
    dict_db
        .insert_translation(&text, &hash, &translated, &source_lang, &target_lang)
        .map_err(|e| e.to_string())?;

    Ok(translated)
}

/// 批量翻译（用于一次性翻译多个释义）
#[tauri::command]
pub async fn translate_batch(
    dict_db: tauri::State<'_, Arc<DictionaryDb>>,
    manager: tauri::State<'_, TaskManager>,
    texts: Vec<String>,
    source_lang: Option<String>,
    target_lang: Option<String>,
) -> Result<Vec<Option<String>>, String> {
    let source_lang = source_lang.unwrap_or_else(|| "en".to_string());
    let target_lang = target_lang.unwrap_or_else(|| "zh".to_string());

    let mut results = Vec::with_capacity(texts.len());

    for text in texts {
        let hash = hash_text(&text);

        // 检查缓存
        if let Ok(Some(cached)) = dict_db.get_translation(&hash, &source_lang, &target_lang) {
            results.push(Some(cached));
            continue;
        }

        // 获取翻译器
        let translator = manager.get_translator().await;

        if let Some(translator) = translator {
            match translator.translate_with_langs(&text, &source_lang, &target_lang).await {
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
                Err(_) => {
                    results.push(None);
                }
            }
        } else {
            results.push(None);
        }
    }

    Ok(results)
}
