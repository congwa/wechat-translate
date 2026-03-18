//! Sidebar 投影服务兼容入口：当前阶段复用既有 runtime 投影实现，
//! 先把依赖路径收口到 application 层，后续再把具体实现迁移进来。
pub(crate) use crate::sidebar_projection::{emit_sidebar_invalidated, SidebarRuntime};
