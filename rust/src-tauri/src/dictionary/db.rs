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
