//! 监听消息 ingest 兼容入口：在目录迁移阶段复用既有实现，避免一次性搬动全部逻辑。
pub(crate) use crate::runtime_monitor_ingest::*;
