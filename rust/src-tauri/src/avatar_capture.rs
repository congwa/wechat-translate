use anyhow::{Context, Result};
use image::GenericImageView;
use log::debug;
use std::collections::HashSet;
use std::io::Cursor;
use std::path::{Path, PathBuf};

pub struct AvatarCache {
    cache_dir: PathBuf,
    known_senders: HashSet<String>,
}

impl AvatarCache {
    pub fn new(app_data_dir: &Path) -> Self {
        let cache_dir = app_data_dir.join("avatar_cache");
        let _ = std::fs::create_dir_all(&cache_dir);

        let mut known_senders = HashSet::new();
        if let Ok(entries) = std::fs::read_dir(&cache_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.ends_with(".png") {
                    known_senders.insert(name.trim_end_matches(".png").to_string());
                }
            }
        }
        debug!(
            "AvatarCache: loaded {} cached avatars from {}",
            known_senders.len(),
            cache_dir.display()
        );

        Self {
            cache_dir,
            known_senders,
        }
    }

    pub fn has_avatar(&self, sender: &str) -> bool {
        let hash = sender_hash(sender);
        self.known_senders.contains(&hash)
    }

    pub fn get_avatar_path(&self, sender: &str) -> Option<PathBuf> {
        let hash = sender_hash(sender);
        if self.known_senders.contains(&hash) {
            let path = self.cache_dir.join(format!("{}.png", hash));
            if path.exists() {
                return Some(path);
            }
        }
        None
    }

    pub fn save_avatar(&mut self, sender: &str, png_bytes: &[u8]) -> Result<PathBuf> {
        let hash = sender_hash(sender);
        let path = self.cache_dir.join(format!("{}.png", hash));
        std::fs::write(&path, png_bytes).context("write avatar file")?;
        self.known_senders.insert(hash);
        Ok(path)
    }
}

fn sender_hash(sender: &str) -> String {
    format!("{:x}", md5::compute(sender.as_bytes()))
}

/// Capture the WeChat window using screencapture.
/// Uses -l flag with window ID for targeted capture without user interaction.
pub fn capture_wechat_window() -> Result<PathBuf> {
    let tmp = std::env::temp_dir().join("wechat_avatar_capture.png");

    // -x: silent (no shutter sound), -o: no shadow
    // First try to capture using the focused window
    let output = std::process::Command::new("screencapture")
        .args(["-x", "-o", "-w", tmp.to_str().unwrap()])
        .output()
        .context("failed to run screencapture")?;

    if !output.status.success() {
        anyhow::bail!(
            "screencapture failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    if !tmp.exists() || std::fs::metadata(&tmp).map(|m| m.len()).unwrap_or(0) == 0 {
        anyhow::bail!("screencapture produced empty file");
    }

    Ok(tmp)
}

/// Extract an avatar from a screenshot given the avatar element's screen position.
///
/// - `screenshot_path`: path to the full window screenshot
/// - `avatar_screen_pos`: (x, y) in screen coordinates of the AXImage element
/// - `window_pos`: (x, y) screen position of the WeChat window
/// - `window_size`: (width, height) logical size of the WeChat window
pub fn extract_avatar(
    screenshot_path: &Path,
    avatar_screen_pos: (f64, f64),
    window_pos: (f64, f64),
    window_size: (f64, f64),
) -> Result<Vec<u8>> {
    let img = image::open(screenshot_path).context("open screenshot")?;
    let (img_w, _img_h) = img.dimensions();

    // Calculate Retina scale factor
    let scale = img_w as f64 / window_size.0;

    // Convert screen coordinates to window-relative, then to pixel coordinates
    let rel_x = ((avatar_screen_pos.0 - window_pos.0) * scale) as u32;
    let rel_y = ((avatar_screen_pos.1 - window_pos.1) * scale) as u32;

    // Avatar size: ~36pt in WeChat, scaled for Retina
    let avatar_size = (36.0 * scale) as u32;

    // Clamp to image bounds
    let x = rel_x.min(img.width().saturating_sub(avatar_size));
    let y = rel_y.min(img.height().saturating_sub(avatar_size));
    let w = avatar_size.min(img.width() - x);
    let h = avatar_size.min(img.height() - y);

    if w < 10 || h < 10 {
        anyhow::bail!("avatar crop region too small: {}x{}", w, h);
    }

    let cropped = img.crop_imm(x, y, w, h);

    let mut buf = Vec::new();
    cropped
        .write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Png)
        .context("encode avatar PNG")?;

    Ok(buf)
}

/// Combined function: capture window, extract avatar, save to cache.
pub fn capture_and_cache_avatar(
    avatar_cache: &mut AvatarCache,
    sender: &str,
    avatar_screen_pos: (f64, f64),
    window_pos: (f64, f64),
    window_size: (f64, f64),
) -> Result<PathBuf> {
    let screenshot = capture_wechat_window()?;
    let png_bytes = extract_avatar(&screenshot, avatar_screen_pos, window_pos, window_size)?;
    let path = avatar_cache.save_avatar(sender, &png_bytes)?;
    let _ = std::fs::remove_file(&screenshot);
    debug!("Captured avatar for '{}' -> {}", sender, path.display());
    Ok(path)
}
