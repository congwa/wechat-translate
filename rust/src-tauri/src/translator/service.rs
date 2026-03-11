use crate::translator::client::DeepLXTranslator;
use crate::translator::limiter::TranslationLimiter;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

/// 翻译配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslateConfig {
    pub enabled: bool,
    pub deeplx_url: String,
    pub source_lang: String,
    pub target_lang: String,
    pub timeout_seconds: f64,
    pub max_concurrency: usize,
    pub max_requests_per_second: usize,
}

impl Default for TranslateConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            deeplx_url: String::new(),
            source_lang: "en".to_string(),
            target_lang: "zh".to_string(),
            timeout_seconds: 8.0,
            max_concurrency: 3,
            max_requests_per_second: 10,
        }
    }
}

/// 翻译服务状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TranslatorServiceStatus {
    pub enabled: bool,
    pub configured: bool,
    pub checking: bool,
    pub healthy: Option<bool>,
    pub last_error: Option<String>,
}

impl TranslatorServiceStatus {
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            configured: false,
            checking: false,
            healthy: None,
            last_error: None,
        }
    }

    pub fn unconfigured() -> Self {
        Self {
            enabled: true,
            configured: false,
            checking: false,
            healthy: None,
            last_error: None,
        }
    }

    pub fn checking() -> Self {
        Self {
            enabled: true,
            configured: true,
            checking: true,
            healthy: None,
            last_error: None,
        }
    }

    pub fn healthy() -> Self {
        Self {
            enabled: true,
            configured: true,
            checking: false,
            healthy: Some(true),
            last_error: None,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        let msg = message.into();
        let short_msg = if msg.len() > 100 {
            format!("{}...", &msg[..100])
        } else {
            msg
        };
        Self {
            enabled: true,
            configured: true,
            checking: false,
            healthy: Some(false),
            last_error: Some(short_msg),
        }
    }

    pub fn menu_text(&self) -> &'static str {
        if !self.enabled {
            "○ 翻译未启用"
        } else if !self.configured {
            "○ 翻译未配置"
        } else if self.checking {
            "◐ 翻译检测中"
        } else if self.healthy == Some(true) {
            "● 翻译服务可用"
        } else {
            "⚠ 翻译服务异常"
        }
    }

    pub fn as_json(&self) -> serde_json::Value {
        serde_json::json!({
            "enabled": self.enabled,
            "configured": self.configured,
            "checking": self.checking,
            "healthy": self.healthy,
            "last_error": self.last_error,
        })
    }
}

/// 全局翻译服务
/// 统一管理 DeepLX 客户端和限流器
pub struct TranslationService {
    translator: RwLock<Option<Arc<DeepLXTranslator>>>,
    limiter: RwLock<Option<Arc<TranslationLimiter>>>,
    config: RwLock<TranslateConfig>,
    status: RwLock<TranslatorServiceStatus>,
}

impl TranslationService {
    pub fn new() -> Self {
        Self {
            translator: RwLock::new(None),
            limiter: RwLock::new(None),
            config: RwLock::new(TranslateConfig::default()),
            status: RwLock::new(TranslatorServiceStatus::disabled()),
        }
    }

    /// 更新翻译配置
    pub async fn update_config(&self, config: TranslateConfig) -> TranslatorServiceStatus {
        let deeplx_url = config.deeplx_url.trim();

        let (translator, limiter, status) = if !config.enabled {
            (None, None, TranslatorServiceStatus::disabled())
        } else if deeplx_url.is_empty() {
            (None, None, TranslatorServiceStatus::unconfigured())
        } else {
            let translator = Arc::new(DeepLXTranslator::new(
                deeplx_url,
                &config.source_lang,
                &config.target_lang,
                config.timeout_seconds,
            ));
            let limiter = Arc::new(TranslationLimiter::new(
                config.max_concurrency,
                config.max_requests_per_second,
            ));
            (Some(translator), Some(limiter), TranslatorServiceStatus::checking())
        };

        {
            let mut t = self.translator.write().await;
            *t = translator;
        }
        {
            let mut l = self.limiter.write().await;
            *l = limiter;
        }
        {
            let mut c = self.config.write().await;
            *c = config;
        }
        {
            let mut s = self.status.write().await;
            *s = status.clone();
        }

        status
    }

    /// 设置翻译服务状态
    pub async fn set_status(&self, status: TranslatorServiceStatus) {
        let mut s = self.status.write().await;
        *s = status;
    }

    /// 获取当前状态
    pub async fn get_status(&self) -> TranslatorServiceStatus {
        self.status.read().await.clone()
    }

    /// 获取当前配置
    pub async fn get_config(&self) -> TranslateConfig {
        self.config.read().await.clone()
    }

    /// 检查服务是否可用
    pub async fn is_available(&self) -> bool {
        self.translator.read().await.is_some()
    }

    /// 翻译文本（使用默认语言配置）
    pub async fn translate(&self, text: &str) -> Result<String> {
        let translator = {
            let guard = self.translator.read().await;
            guard.clone()
        };

        let translator = translator.ok_or_else(|| anyhow::anyhow!("翻译服务未配置"))?;

        let limiter = {
            let guard = self.limiter.read().await;
            guard.clone()
        };

        if let Some(limiter) = limiter {
            let _permit = limiter.acquire().await;
            translator.translate(text).await
        } else {
            translator.translate(text).await
        }
    }

    /// 翻译文本（指定语言）
    pub async fn translate_with_langs(
        &self,
        text: &str,
        source_lang: &str,
        target_lang: &str,
    ) -> Result<String> {
        let translator = {
            let guard = self.translator.read().await;
            guard.clone()
        };

        let translator = translator.ok_or_else(|| anyhow::anyhow!("翻译服务未配置"))?;

        let limiter = {
            let guard = self.limiter.read().await;
            guard.clone()
        };

        if let Some(limiter) = limiter {
            let _permit = limiter.acquire().await;
            translator.translate_with_langs(text, source_lang, target_lang).await
        } else {
            translator.translate_with_langs(text, source_lang, target_lang).await
        }
    }

    /// 健康检查
    pub async fn check_health(&self) -> Result<()> {
        let translator = {
            let guard = self.translator.read().await;
            guard.clone()
        };

        let translator = translator.ok_or_else(|| anyhow::anyhow!("翻译服务未配置"))?;

        let limiter = {
            let guard = self.limiter.read().await;
            guard.clone()
        };

        if let Some(limiter) = limiter {
            let _permit = limiter.acquire().await;
            translator.check_health().await
        } else {
            translator.check_health().await
        }
    }

    /// 获取翻译器（供需要直接访问的场景）
    pub async fn get_translator(&self) -> Option<Arc<DeepLXTranslator>> {
        self.translator.read().await.clone()
    }

    /// 获取限流器（供需要直接访问的场景）
    pub async fn get_limiter(&self) -> Option<Arc<TranslationLimiter>> {
        self.limiter.read().await.clone()
    }
}

impl Default for TranslationService {
    fn default() -> Self {
        Self::new()
    }
}
