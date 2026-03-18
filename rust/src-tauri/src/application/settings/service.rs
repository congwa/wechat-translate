//! 设置应用服务：负责配置校验、落盘与运行态协同刷新，
//! 让 command 层不再重复拼装“保存配置后应用到运行时”的业务流程。
use crate::application::runtime::service::RuntimeService;
use crate::config::{self as app_config, AppConfig, ConfigDir};
use std::path::PathBuf;

/// 一次配置保存动作的业务结果。
/// 若 `errors` 非空，表示配置校验未通过，此时不会更新运行态。
pub(crate) struct SettingsSaveResult {
    pub(crate) errors: Vec<String>,
    pub(crate) path: Option<String>,
    pub(crate) settings: Option<AppConfig>,
}

/// SettingsService 是配置写入的应用层 owner。
/// 它统一负责配置合法性检查、配置文件持久化和运行态同步。
pub(crate) struct SettingsService {
    config_dir: PathBuf,
    runtime: RuntimeService,
}

impl SettingsService {
    /// 基于当前配置目录与运行态服务构建配置应用服务。
    pub(crate) fn new(config_dir: &ConfigDir, runtime: RuntimeService) -> Self {
        Self {
            config_dir: config_dir.0.clone(),
            runtime,
        }
    }

    /// 保存一份原始 JSON 配置，并在成功后把规范化配置应用到运行态。
    pub(crate) async fn save_raw_config(
        &self,
        raw: &serde_json::Value,
    ) -> Result<SettingsSaveResult, String> {
        let (errors, path) = app_config::validate_and_write_config(&self.config_dir, raw)
            .map_err(|error| error.to_string())?;

        if !errors.is_empty() {
            return Ok(SettingsSaveResult {
                errors,
                path,
                settings: None,
            });
        }

        let settings =
            app_config::load_app_config(&self.config_dir).map_err(|error| error.to_string())?;
        self.runtime.apply_runtime_config(&settings).await;

        Ok(SettingsSaveResult {
            errors: Vec::new(),
            path,
            settings: Some(settings),
        })
    }
}
