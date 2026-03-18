//! 翻译运行时配置拼装器：把设置快照或 sidebar 启动参数转换成统一的翻译配置，
//! 避免 TaskManager 在多处重复拼装 provider 与限流参数。
use crate::config::AppConfig;
use crate::translator::{TranslateConfig, TranslateProviderConfig};

/// SidebarTranslationRuntimeParams 表示一次 sidebar 启动请求里与翻译相关的参数集合。
pub(crate) struct SidebarTranslationRuntimeParams {
    pub(crate) translate_enabled: bool,
    pub(crate) provider: String,
    pub(crate) deeplx_url: String,
    pub(crate) ai_provider_id: String,
    pub(crate) ai_model_id: String,
    pub(crate) ai_api_key: String,
    pub(crate) ai_base_url: String,
    pub(crate) source_lang: String,
    pub(crate) target_lang: String,
    pub(crate) timeout_seconds: f64,
    pub(crate) max_concurrency: usize,
    pub(crate) max_requests_per_second: usize,
}

/// 根据完整应用配置构建翻译运行时配置，保证“保存配置后应用运行态”与“实时启动 sidebar”使用同一规则。
pub(crate) fn build_translate_config_from_app_config(config: &AppConfig) -> TranslateConfig {
    TranslateConfig {
        enabled: config.translate.enabled,
        provider_config: build_provider_config(
            &config.translate.provider,
            config.translate.deeplx_url.clone(),
            config.translate.ai_provider_id.clone(),
            config.translate.ai_model_id.clone(),
            config.translate.ai_api_key.clone(),
            config.translate.ai_base_url.clone(),
        ),
        source_lang: config.translate.source_lang.clone(),
        target_lang: config.translate.target_lang.clone(),
        timeout_seconds: config.translate.timeout_seconds,
        max_concurrency: config.translate.max_concurrency,
        max_requests_per_second: config.translate.max_requests_per_second,
    }
}

/// 根据 sidebar 启动时覆盖的翻译参数构建运行时配置，保证临时启动链路与配置落盘链路语义一致。
pub(crate) fn build_translate_config_from_sidebar_params(
    params: SidebarTranslationRuntimeParams,
) -> TranslateConfig {
    TranslateConfig {
        enabled: params.translate_enabled,
        provider_config: build_provider_config(
            &params.provider,
            params.deeplx_url,
            params.ai_provider_id,
            params.ai_model_id,
            params.ai_api_key,
            params.ai_base_url,
        ),
        source_lang: params.source_lang,
        target_lang: params.target_lang,
        timeout_seconds: params.timeout_seconds,
        max_concurrency: params.max_concurrency,
        max_requests_per_second: params.max_requests_per_second,
    }
}

/// 把 provider 选择统一映射成 TranslationService 能识别的 provider 配置。
fn build_provider_config(
    provider: &str,
    deeplx_url: String,
    ai_provider_id: String,
    ai_model_id: String,
    ai_api_key: String,
    ai_base_url: String,
) -> TranslateProviderConfig {
    match provider {
        "ai" => TranslateProviderConfig::Ai {
            provider_id: ai_provider_id,
            model_id: ai_model_id,
            api_key: ai_api_key,
            base_url: if ai_base_url.is_empty() {
                None
            } else {
                Some(ai_base_url)
            },
        },
        _ => TranslateProviderConfig::Deeplx { url: deeplx_url },
    }
}
