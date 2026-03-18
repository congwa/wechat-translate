//! 接口层模块入口：用于承接 Tauri 的 commands/queries 边界，
//! 让 API 目录结构逐步从旧的混装命令文件迁移到更清晰的接口分层。
pub(crate) mod commands;
pub(crate) mod queries;
