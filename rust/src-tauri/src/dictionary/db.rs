use anyhow::Result;
use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::Mutex;

use super::types::WordEntry;

const WORD_CACHE_TTL_DAYS: i64 = 7;
const TRANSLATION_CACHE_TTL_DAYS: i64 = 30;

pub struct DictionaryDb {
    conn: Mutex<Connection>,
}

impl DictionaryDb {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let conn = Connection::open(path)?;
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.init_tables()?;
        Ok(db)
    }

    fn init_tables(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS word_dictionary (
                word TEXT PRIMARY KEY NOT NULL,
                phonetic_uk TEXT,
                phonetic_us TEXT,
                audio_url_uk TEXT,
                audio_url_us TEXT,
                raw_json TEXT NOT NULL,
                fetched_at TEXT NOT NULL,
                created_at TEXT DEFAULT CURRENT_TIMESTAMP
            );

            CREATE TABLE IF NOT EXISTS translation_cache (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                source_text TEXT NOT NULL,
                source_hash TEXT NOT NULL,
                translated_text TEXT NOT NULL,
                source_lang TEXT DEFAULT 'en',
                target_lang TEXT DEFAULT 'zh',
                created_at TEXT DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(source_hash, source_lang, target_lang)
            );

            CREATE INDEX IF NOT EXISTS idx_translation_hash 
                ON translation_cache(source_hash, source_lang, target_lang);

            CREATE TABLE IF NOT EXISTS word_favorites (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                word TEXT NOT NULL UNIQUE,
                phonetic TEXT,
                meanings_json TEXT,
                note TEXT DEFAULT '',
                review_count INTEGER DEFAULT 0,
                last_review_at TEXT,
                created_at TEXT DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT DEFAULT CURRENT_TIMESTAMP
            );

            CREATE INDEX IF NOT EXISTS idx_favorites_word ON word_favorites(word);
            CREATE INDEX IF NOT EXISTS idx_favorites_created ON word_favorites(created_at DESC);
            "#,
        )?;
        Ok(())
    }

    /// 获取缓存的单词条目
    pub fn get_word(&self, word: &str) -> Result<Option<WordEntry>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT raw_json, fetched_at FROM word_dictionary WHERE word = ?1",
        )?;

        let result = stmt.query_row(params![word.to_lowercase()], |row| {
            let raw_json: String = row.get(0)?;
            let fetched_at: String = row.get(1)?;
            Ok((raw_json, fetched_at))
        });

        match result {
            Ok((raw_json, fetched_at)) => {
                if Self::is_word_expired(&fetched_at) {
                    return Ok(None);
                }
                let entry: WordEntry = serde_json::from_str(&raw_json)
                    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
                Ok(Some(entry))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// 插入或更新单词条目
    pub fn upsert_word(&self, word: &str, entry: &WordEntry) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let raw_json = serde_json::to_string(entry)?;

        let phonetic_uk = entry
            .phonetics
            .iter()
            .find(|p| p.region.as_deref() == Some("uk"))
            .and_then(|p| p.text.clone());
        let phonetic_us = entry
            .phonetics
            .iter()
            .find(|p| p.region.as_deref() == Some("us"))
            .and_then(|p| p.text.clone());
        let audio_url_uk = entry
            .phonetics
            .iter()
            .find(|p| p.region.as_deref() == Some("uk"))
            .and_then(|p| p.audio_url.clone());
        let audio_url_us = entry
            .phonetics
            .iter()
            .find(|p| p.region.as_deref() == Some("us"))
            .and_then(|p| p.audio_url.clone());

        conn.execute(
            r#"
            INSERT OR REPLACE INTO word_dictionary 
            (word, phonetic_uk, phonetic_us, audio_url_uk, audio_url_us, raw_json, fetched_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                word.to_lowercase(),
                phonetic_uk,
                phonetic_us,
                audio_url_uk,
                audio_url_us,
                raw_json,
                entry.fetched_at,
            ],
        )?;
        Ok(())
    }

    /// 获取缓存的翻译
    pub fn get_translation(
        &self,
        source_hash: &str,
        source_lang: &str,
        target_lang: &str,
    ) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            r#"
            SELECT translated_text, created_at 
            FROM translation_cache 
            WHERE source_hash = ?1 AND source_lang = ?2 AND target_lang = ?3
            "#,
        )?;

        let result = stmt.query_row(
            params![source_hash, source_lang, target_lang],
            |row| {
                let translated: String = row.get(0)?;
                let created_at: String = row.get(1)?;
                Ok((translated, created_at))
            },
        );

        match result {
            Ok((translated, created_at)) => {
                if Self::is_translation_expired(&created_at) {
                    return Ok(None);
                }
                Ok(Some(translated))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// 插入翻译缓存
    pub fn insert_translation(
        &self,
        source_text: &str,
        source_hash: &str,
        translated_text: &str,
        source_lang: &str,
        target_lang: &str,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"
            INSERT OR REPLACE INTO translation_cache 
            (source_text, source_hash, translated_text, source_lang, target_lang, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'))
            "#,
            params![source_text, source_hash, translated_text, source_lang, target_lang],
        )?;
        Ok(())
    }

    fn is_word_expired(fetched_at: &str) -> bool {
        if let Ok(fetched) = chrono::NaiveDateTime::parse_from_str(fetched_at, "%Y-%m-%d %H:%M:%S")
        {
            let now = chrono::Local::now().naive_local();
            let diff = now.signed_duration_since(fetched);
            return diff.num_days() > WORD_CACHE_TTL_DAYS;
        }
        true
    }

    fn is_translation_expired(created_at: &str) -> bool {
        if let Ok(created) = chrono::NaiveDateTime::parse_from_str(created_at, "%Y-%m-%d %H:%M:%S")
        {
            let now = chrono::Local::now().naive_local();
            let diff = now.signed_duration_since(created);
            return diff.num_days() > TRANSLATION_CACHE_TTL_DAYS;
        }
        true
    }

    // ========== 收藏功能 ==========

    /// 收藏单词（如果已存在则返回 false）
    pub fn add_favorite(&self, word: &str, entry: Option<&WordEntry>) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let word_lower = word.to_lowercase();

        let (phonetic, meanings_json) = if let Some(e) = entry {
            let phonetic = e.phonetics.iter().find(|p| p.text.is_some()).and_then(|p| p.text.clone());
            let meanings_json = serde_json::to_string(&e.meanings).ok();
            (phonetic, meanings_json)
        } else {
            (None, None)
        };

        let result = conn.execute(
            r#"
            INSERT OR IGNORE INTO word_favorites (word, phonetic, meanings_json, created_at, updated_at)
            VALUES (?1, ?2, ?3, datetime('now'), datetime('now'))
            "#,
            params![word_lower, phonetic, meanings_json],
        )?;

        Ok(result > 0)
    }

    /// 取消收藏
    pub fn remove_favorite(&self, word: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let result = conn.execute(
            "DELETE FROM word_favorites WHERE word = ?1",
            params![word.to_lowercase()],
        )?;
        Ok(result > 0)
    }

    /// 检查是否已收藏
    pub fn is_favorited(&self, word: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT 1 FROM word_favorites WHERE word = ?1")?;
        let exists = stmt.exists(params![word.to_lowercase()])?;
        Ok(exists)
    }

    /// 批量检查收藏状态
    pub fn get_favorites_batch(&self, words: &[String]) -> Result<Vec<(String, bool)>> {
        let conn = self.conn.lock().unwrap();
        let mut results = Vec::with_capacity(words.len());

        for word in words {
            let word_lower = word.to_lowercase();
            let mut stmt = conn.prepare("SELECT 1 FROM word_favorites WHERE word = ?1")?;
            let exists = stmt.exists(params![&word_lower])?;
            results.push((word_lower, exists));
        }

        Ok(results)
    }

    /// 获取收藏列表
    pub fn list_favorites(&self, offset: u32, limit: u32) -> Result<Vec<FavoriteWord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            r#"
            SELECT word, phonetic, meanings_json, note, review_count, last_review_at, created_at
            FROM word_favorites
            ORDER BY created_at DESC
            LIMIT ?1 OFFSET ?2
            "#,
        )?;

        let rows = stmt.query_map(params![limit, offset], |row| {
            Ok(FavoriteWord {
                word: row.get(0)?,
                phonetic: row.get(1)?,
                meanings_json: row.get(2)?,
                note: row.get(3)?,
                review_count: row.get(4)?,
                last_review_at: row.get(5)?,
                created_at: row.get(6)?,
            })
        })?;

        let mut favorites = Vec::new();
        for row in rows {
            favorites.push(row?);
        }
        Ok(favorites)
    }

    /// 更新收藏笔记
    pub fn update_favorite_note(&self, word: &str, note: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let result = conn.execute(
            r#"
            UPDATE word_favorites 
            SET note = ?1, updated_at = datetime('now')
            WHERE word = ?2
            "#,
            params![note, word.to_lowercase()],
        )?;
        Ok(result > 0)
    }

    /// 记录复习
    pub fn record_review(&self, word: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let result = conn.execute(
            r#"
            UPDATE word_favorites 
            SET review_count = review_count + 1, 
                last_review_at = datetime('now'),
                updated_at = datetime('now')
            WHERE word = ?1
            "#,
            params![word.to_lowercase()],
        )?;
        Ok(result > 0)
    }

    /// 获取收藏总数
    pub fn count_favorites(&self) -> Result<u32> {
        let conn = self.conn.lock().unwrap();
        let count: u32 = conn.query_row(
            "SELECT COUNT(*) FROM word_favorites",
            [],
            |row| row.get(0),
        )?;
        Ok(count)
    }
}

/// 收藏单词数据结构
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FavoriteWord {
    pub word: String,
    pub phonetic: Option<String>,
    pub meanings_json: Option<String>,
    pub note: Option<String>,
    pub review_count: i32,
    pub last_review_at: Option<String>,
    pub created_at: String,
}

/// 生成文本哈希（用于翻译缓存 key）
pub fn hash_text(text: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_hash_text() {
        let hash1 = hash_text("hello world");
        let hash2 = hash_text("hello world");
        let hash3 = hash_text("hello");

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_translation_cache() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let db = DictionaryDb::open(&db_path).unwrap();

        let hash = hash_text("hello");
        db.insert_translation("hello", &hash, "你好", "en", "zh")
            .unwrap();

        let result = db.get_translation(&hash, "en", "zh").unwrap();
        assert_eq!(result, Some("你好".to_string()));

        let result2 = db.get_translation(&hash, "en", "ja").unwrap();
        assert_eq!(result2, None);
    }
}
