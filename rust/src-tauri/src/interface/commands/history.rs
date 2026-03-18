#![allow(unused_imports)]
//! 历史总结命令兼容入口：承接历史总结与参与成员列表等一次性读取型命令。
pub(crate) use crate::commands::history::{
    history_summary_generate, history_summary_participants_get,
};
