//! 应用层模块入口：收口运行时编排与 sidebar 投影等业务服务，
//! 让 interface 层只依赖稳定的应用服务，而不是直接操作底层实现细节。
pub(crate) mod runtime;
pub(crate) mod settings;
pub(crate) mod sidebar;
