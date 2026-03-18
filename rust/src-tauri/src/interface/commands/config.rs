#![allow(unused_imports)]
//! 配置写命令兼容入口：在目录迁移阶段继续复用既有命令实现。
pub(crate) use crate::commands::app_state::settings_update;
pub(crate) use crate::commands::config::{config_default, config_get, config_put};
