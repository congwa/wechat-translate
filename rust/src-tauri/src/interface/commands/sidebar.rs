#![allow(unused_imports)]
//! Sidebar 命令兼容入口：聚合浮窗生命周期、手动翻译与联动启动能力。
pub(crate) use crate::commands::sidebar::{
    live_start, sidebar_start, sidebar_stop, sidebar_window_close, sidebar_window_open,
    translate_sidebar_message, translate_test,
};
