//! 旧命令实现层：保留已迁移到 `interface/*` 之后仍需复用的内部实现函数，
//! 不再作为 Tauri 暴露面的主入口，只承担兼容转发与内部共享逻辑。
pub(crate) mod app_state;
pub(crate) mod audio;
pub(crate) mod config;
pub(crate) mod db;
pub(crate) mod dictionary;
pub(crate) mod listen;
pub(crate) mod preflight;
pub(crate) mod sidebar;
pub(crate) mod tray;
