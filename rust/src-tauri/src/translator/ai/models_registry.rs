use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 模型费用信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCost {
    #[serde(default)]
    pub input: f64,
    #[serde(default)]
    pub output: f64,
}

/// 模型信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub family: Option<String>,
    #[serde(default)]
    pub cost: Option<ModelCost>,
    #[serde(default)]
    pub tool_call: bool,
}

/// 渠道信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub api: Option<String>,
    #[serde(default)]
    pub doc: Option<String>,
    #[serde(default)]
    pub models: HashMap<String, ModelInfo>,
}

/// rig 框架支持的渠道列表
const SUPPORTED_PROVIDERS: &[&str] = &[
    "openai",
    "anthropic",
    "cohere",
    "deepseek",
    "gemini",
    "groq",
    "mistral",
    "openrouter",
    "perplexity",
    "together",
    "xai",
    "hyperbolic",
    "moonshot",
];

/// 从 models.dev 获取可用渠道和模型
pub async fn fetch_providers() -> Result<HashMap<String, ProviderInfo>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    let resp = client.get("https://models.dev/api.json").send().await?;

    if !resp.status().is_success() {
        anyhow::bail!("Failed to fetch models.dev API: {}", resp.status());
    }

    let providers: HashMap<String, ProviderInfo> = resp.json().await?;

    // 过滤：只保留 rig 支持的渠道
    let filtered: HashMap<String, ProviderInfo> = providers
        .into_iter()
        .filter(|(k, _)| SUPPORTED_PROVIDERS.contains(&k.as_str()))
        .collect();

    Ok(filtered)
}
