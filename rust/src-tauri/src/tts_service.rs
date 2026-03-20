//! TTS 服务：封装系统 TTS 引擎（macOS 上为 AVFoundation），提供朗读、停止和状态查询能力。
//! 通过 `on_utterance_begin/end` 回调向前端推送 `tts-utterance-begin/end` 事件，
//! 供侧边栏消息卡片呈现"正在朗读"动画。
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

/// TTS 运行态，作为 Tauri managed state 注册。
/// 使用 `Arc<TtsState>` 以便在回调闭包与主业务逻辑之间共享引用。
pub struct TtsState {
    tts: std::sync::Mutex<Option<tts::Tts>>,
    pub enabled: AtomicBool,
    /// speak() 调用前写入，on_utterance_begin 回调中读取，避免回调与 speak() 争锁
    pending_message_id: Arc<AtomicU64>,
    /// on_utterance_begin 回调写入，on_utterance_end 回调读取
    speaking_message_id: Arc<AtomicU64>,
}

impl TtsState {
    pub fn new() -> Self {
        Self {
            tts: std::sync::Mutex::new(None),
            enabled: AtomicBool::new(false),
            pending_message_id: Arc::new(AtomicU64::new(0)),
            speaking_message_id: Arc::new(AtomicU64::new(0)),
        }
    }

    /// 初始化 TTS 引擎并注册 utterance 事件回调。
    /// 应在 Tauri setup 完成后、拿到 AppHandle 时调用一次。
    /// 失败时静默降级（TTS 功能不可用但应用正常运行）。
    pub fn init(&self, app_handle: AppHandle) {
        let mut guard = match self.tts.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        if guard.is_some() {
            return;
        }

        match tts::Tts::default() {
            Ok(tts_instance) => {
                let pending_id = Arc::clone(&self.pending_message_id);
                let speaking_id_begin = Arc::clone(&self.speaking_message_id);
                let speaking_id_end = Arc::clone(&self.speaking_message_id);
                let speaking_id_stop = Arc::clone(&self.speaking_message_id);
                let app_begin = app_handle.clone();
                let app_end = app_handle.clone();
                let app_stop = app_handle;

                if let Err(e) =
                    tts_instance.on_utterance_begin(Some(Box::new(move |_uid| {
                        let msg_id = pending_id.load(Ordering::SeqCst);
                        speaking_id_begin.store(msg_id, Ordering::SeqCst);
                        let _ = app_begin.emit(
                            "tts-utterance-begin",
                            serde_json::json!({ "message_id": msg_id }),
                        );
                    })))
                {
                    log::warn!("[TtsState] on_utterance_begin 注册失败: {}", e);
                }

                if let Err(e) =
                    tts_instance.on_utterance_end(Some(Box::new(move |_uid| {
                        let msg_id = speaking_id_end.load(Ordering::SeqCst);
                        let _ = app_end.emit(
                            "tts-utterance-end",
                            serde_json::json!({ "message_id": msg_id }),
                        );
                    })))
                {
                    log::warn!("[TtsState] on_utterance_end 注册失败: {}", e);
                }

                if let Err(e) =
                    tts_instance.on_utterance_stop(Some(Box::new(move |_uid| {
                        let msg_id = speaking_id_stop.load(Ordering::SeqCst);
                        let _ = app_stop.emit(
                            "tts-utterance-end",
                            serde_json::json!({ "message_id": msg_id }),
                        );
                    })))
                {
                    log::warn!("[TtsState] on_utterance_stop 注册失败: {}", e);
                }

                *guard = Some(tts_instance);
                log::info!("[TtsState] TTS 引擎初始化成功");
            }
            Err(e) => {
                log::warn!("[TtsState] TTS 引擎初始化失败（静默降级）: {}", e);
            }
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
        if !enabled {
            self.stop();
        }
    }

    /// 朗读文本。`message_id` 用于前端动画追踪（传 0 表示无消息 ID）。
    /// `interrupt = true` 保证永远只朗读最新一条消息，不积压队列。
    pub fn speak(&self, message_id: u64, text: &str) {
        if !self.is_enabled() {
            return;
        }
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return;
        }

        let mut guard = match self.tts.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        let tts = match guard.as_mut() {
            Some(t) => t,
            None => return,
        };

        self.pending_message_id.store(message_id, Ordering::SeqCst);
        if let Err(e) = tts.speak(trimmed, true) {
            log::warn!("[TtsState] speak 失败: {}", e);
        }
    }

    pub fn stop(&self) {
        let mut guard = match self.tts.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        if let Some(tts) = guard.as_mut() {
            let _ = tts.stop();
        }
    }

    pub fn is_initialized(&self) -> bool {
        self.tts
            .lock()
            .map(|g| g.is_some())
            .unwrap_or(false)
    }

    pub fn is_speaking(&self) -> bool {
        let guard = match self.tts.lock() {
            Ok(g) => g,
            Err(_) => return false,
        };
        guard
            .as_ref()
            .map(|t| t.is_speaking().unwrap_or(false))
            .unwrap_or(false)
    }
}

// ---------------------------------------------------------------------------
// 语言检测与 TTS 文本选择工具函数
// ---------------------------------------------------------------------------

/// 判断文本中是否含有 CJK（中日韩）统一表意文字
#[allow(dead_code)]
pub fn has_cjk(text: &str) -> bool {
    text.chars().any(|c| {
        ('\u{4E00}'..='\u{9FFF}').contains(&c)
            || ('\u{3400}'..='\u{4DBF}').contains(&c)
            || ('\u{F900}'..='\u{FAFF}').contains(&c)
    })
}

/// 根据内容和翻译目标语言选择朗读文本：
/// - 纯英文（无 CJK）→ 读原文 `content`
/// - 含 CJK + target_lang 以 "ZH" 开头 → 读中文原文 `content`
/// - 含 CJK + target_lang = "EN" → 优先读英文译文 `content_en`，无译文则读原文
#[allow(dead_code)]
pub fn select_tts_text<'a>(
    content: &'a str,
    content_en: &'a str,
    target_lang: &str,
) -> Option<&'a str> {
    let text = content.trim();
    if text.is_empty() {
        return None;
    }

    if !has_cjk(content) {
        Some(content)
    } else if target_lang.to_uppercase().starts_with("ZH") {
        Some(content)
    } else {
        let en = content_en.trim();
        if !en.is_empty() && en != content {
            Some(content_en)
        } else {
            Some(content)
        }
    }
}
