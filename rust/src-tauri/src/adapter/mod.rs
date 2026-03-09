pub mod applescript;
pub mod ax_reader;

use anyhow::Result;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

pub struct MacOSAdapter {
    ui_paused: AtomicBool,
}

impl MacOSAdapter {
    pub fn new() -> Self {
        Self {
            ui_paused: AtomicBool::new(false),
        }
    }

    pub fn pause_ui(&self) {
        self.ui_paused.store(true, Ordering::SeqCst);
    }

    pub fn resume_ui(&self) {
        self.ui_paused.store(false, Ordering::SeqCst);
    }

    pub fn is_ui_paused(&self) -> bool {
        self.ui_paused.load(Ordering::SeqCst)
    }

    pub fn is_supported(&self) -> bool {
        cfg!(target_os = "macos")
    }

    pub fn support_reason(&self) -> String {
        if self.is_supported() {
            "ok".to_string()
        } else {
            "当前不是 macOS 环境".to_string()
        }
    }

    fn ensure_supported(&self) -> Result<()> {
        if !self.is_supported() {
            anyhow::bail!("{}", self.support_reason());
        }
        Ok(())
    }

    pub fn get_current_sessions(&self) -> Result<Vec<String>> {
        self.ensure_supported()?;
        ax_reader::get_current_sessions()
    }

    pub fn send_text(&self, who: &str, text: &str) -> Result<()> {
        self.ensure_supported()?;
        let body = text.trim();
        if body.is_empty() {
            anyhow::bail!("消息不能为空");
        }

        applescript::activate_wechat()?;
        let who_trimmed = who.trim();
        if !who_trimmed.is_empty() {
            applescript::open_chat_by_search(who_trimmed)?;
        }
        applescript::copy_text(body)?;
        applescript::paste_and_send(true)?;
        Ok(())
    }

    pub fn send_files(&self, who: &str, file_paths: &[String]) -> Result<()> {
        self.ensure_supported()?;
        let files: Vec<String> = file_paths
            .iter()
            .filter(|p| !p.trim().is_empty())
            .map(|p| {
                let path = Path::new(p.trim());
                if path.is_absolute() {
                    p.trim().to_string()
                } else {
                    std::fs::canonicalize(path)
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_else(|_| p.trim().to_string())
                }
            })
            .collect();

        if files.is_empty() {
            anyhow::bail!("file_paths 不能为空");
        }

        for file_path in &files {
            if !Path::new(file_path).exists() {
                anyhow::bail!("文件不存在: {}", file_path);
            }
        }

        applescript::activate_wechat()?;
        let who_trimmed = who.trim();
        if !who_trimmed.is_empty() {
            applescript::open_chat_by_search(who_trimmed)?;
        }

        for file_path in &files {
            applescript::copy_file(file_path)?;
            applescript::paste_and_send(false)?;
            std::thread::sleep(std::time::Duration::from_millis(80));
        }

        applescript::press_enter()?;
        Ok(())
    }

    pub fn read_latest_message(&self) -> Result<String> {
        self.ensure_supported()?;
        ax_reader::read_latest_message()
    }

    pub fn read_active_chat_name(&self) -> Result<String> {
        self.ensure_supported()?;
        ax_reader::read_active_chat_name()
    }

    pub fn read_active_chat_member_count(&self) -> Result<Option<u32>> {
        self.ensure_supported()?;
        ax_reader::read_active_chat_member_count()
    }

    pub fn read_chat_messages_rich(&self) -> Result<Vec<ax_reader::ChatMessage>> {
        self.ensure_supported()?;
        ax_reader::read_chat_messages_rich()
    }

    pub fn read_session_snapshots(&self) -> Result<Vec<ax_reader::SessionItemSnapshot>> {
        self.ensure_supported()?;
        ax_reader::read_session_snapshots()
    }

    pub fn read_session_preview_for_chat(&self, chat_name: &str) -> Result<Option<String>> {
        self.ensure_supported()?;
        ax_reader::read_session_preview_for_chat(chat_name)
    }

    pub fn has_popup_or_menu(&self) -> bool {
        self.is_supported() && ax_reader::has_popup_or_menu()
    }
}
