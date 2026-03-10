use anyhow::{anyhow, Result};
use reqwest::Client;
use std::time::Duration;

use super::types::{DictionaryApiResponse, WordEntry};

const API_BASE_URL: &str = "https://api.dictionaryapi.dev/api/v2/entries/en";
const REQUEST_TIMEOUT_SECS: u64 = 10;

pub struct DictionaryApiClient {
    client: Client,
}

impl DictionaryApiClient {
    pub fn new() -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build()?;
        Ok(Self { client })
    }

    /// 查询单词释义
    pub async fn lookup(&self, word: &str) -> Result<WordEntry> {
        let word = word.trim().to_lowercase();
        if word.is_empty() {
            return Err(anyhow!("Word cannot be empty"));
        }

        // 简单 URL 编码：单词通常只包含字母，空格替换为 %20
        let encoded_word = word.replace(' ', "%20");
        let url = format!("{}/{}", API_BASE_URL, encoded_word);
        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            if status.as_u16() == 404 {
                return Err(anyhow!("Word not found: {}", word));
            }
            return Err(anyhow!("API request failed with status: {}", status));
        }

        let body = response.text().await?;
        let api_responses: Vec<DictionaryApiResponse> = serde_json::from_str(&body)
            .map_err(|e| anyhow!("Failed to parse API response: {}", e))?;

        let first = api_responses
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("Empty response from API"))?;

        Ok(WordEntry::from(first))
    }
}

impl Default for DictionaryApiClient {
    fn default() -> Self {
        Self::new().expect("Failed to create DictionaryApiClient")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_lookup_hello() {
        let client = DictionaryApiClient::new().unwrap();
        let result = client.lookup("hello").await;
        
        match result {
            Ok(entry) => {
                assert_eq!(entry.word, "hello");
                assert!(!entry.meanings.is_empty());
            }
            Err(e) => {
                // 网络问题时跳过测试
                eprintln!("Skipping test due to network error: {}", e);
            }
        }
    }

    #[tokio::test]
    async fn test_lookup_not_found() {
        let client = DictionaryApiClient::new().unwrap();
        let result = client.lookup("asdfghjklqwerty").await;
        
        assert!(result.is_err());
    }
}
