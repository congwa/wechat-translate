//! 运行时应用服务入口：统一暴露监听生命周期、消息 ingest 和翻译运行态。
pub(crate) mod lifecycle;
pub(crate) mod monitor_ingest;
pub(crate) mod monitor_loop;
pub(crate) mod read_service;
pub(crate) mod service;
pub(crate) mod sidebar_runtime;
pub(crate) mod snapshot_service;
pub(crate) mod state;
pub(crate) mod status_sync;
pub(crate) mod translation_config;
pub(crate) mod translator_runtime;
