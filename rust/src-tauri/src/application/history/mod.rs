//! 历史应用服务入口：收口消息历史查询与 AI 总结这类只读业务用例，
//! 让 command 层只负责 Tauri 参数拆装，不再直接拼接数据库和总结逻辑。
pub(crate) mod query_service;
pub(crate) mod summary_service;
