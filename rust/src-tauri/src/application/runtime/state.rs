//! 运行时状态模型：定义监听生命周期、sidebar 运行态与首次轮询信号等共享状态，
//! 让 TaskManager 与 monitor loop 复用同一份状态语义，而不是各自内嵌结构体。
use crate::translator::TranslationLimiter;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;

/// TaskState 表示当前运行时对外可见的任务快照，供前端和托盘展示监听/浮窗是否运行。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskState {
    pub monitoring: bool,
    pub sidebar: bool,
}

/// SidebarConfig 保存 sidebar 运行链路当前依赖的译文器、限流器与目标会话集合。
pub(crate) struct SidebarConfig {
    pub(crate) translator: Option<Arc<dyn crate::translator::Translator>>,
    pub(crate) limiter: Option<Arc<TranslationLimiter>>,
    pub(crate) target_set: HashSet<String>,
    pub(crate) image_capture: bool,
}

/// 监听循环首次 poll 完成信号，用于需要等待监听真正就绪的业务链路。
pub struct FirstPollSignal {
    tx: watch::Sender<Option<String>>,
    rx: watch::Receiver<Option<String>>,
}

impl FirstPollSignal {
    /// 创建一组新的首次轮询信号通道，初始状态为空。
    pub(crate) fn new() -> Self {
        let (tx, rx) = watch::channel(None);
        Self { tx, rx }
    }

    /// 由监听循环在首次成功 poll 后标记 ready，并带上当前活跃聊天名称。
    pub(crate) fn signal_ready(&self, chat_name: &str) {
        let _ = self.tx.send(Some(chat_name.to_string()));
    }

    /// 在监听重启前重置信号，避免新一轮等待误读上一轮的 ready 结果。
    pub(crate) fn reset(&self) {
        let _ = self.tx.send(None);
    }

    /// 等待监听循环首次成功 poll；超时则返回 None，避免调用方无限挂起。
    pub async fn wait_ready(&self, timeout: Duration) -> Option<String> {
        let mut rx = self.rx.clone();
        tokio::select! {
            result = async {
                loop {
                    if let Some(chat_name) = rx.borrow().clone() {
                        return Some(chat_name);
                    }
                    if rx.changed().await.is_err() {
                        return None;
                    }
                }
            } => result,
            _ = tokio::time::sleep(timeout) => None,
        }
    }
}

/// MonitorConfig 保存监听循环在运行中可热更新的采集策略。
#[derive(Debug, Clone, Copy)]
pub(crate) struct MonitorConfig {
    pub(crate) use_right_panel_details: bool,
}
