use crate::adapter::ax_reader::{self, ChatMessage};
use log::debug;
use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub(crate) struct DiffResult {
    pub new_messages: Vec<ChatMessage>,
    pub anchor_failed: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct PreviewSenderHint {
    pub sender: String,
    pub preview_body: String,
    pub preview_body_key: String,
    pub unread_count: u32,
    pub updated_at: Instant,
    pub consumed: bool,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct SessionListenState {
    pub last_preview_body: String,
    pub last_unread: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChatKind {
    Group,
    Private,
    Unknown,
}

impl ChatKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            ChatKind::Group => "group",
            ChatKind::Private => "private",
            ChatKind::Unknown => "unknown",
        }
    }
}

pub(crate) const PREVIEW_SENDER_HINT_TTL: Duration = Duration::from_secs(12);

pub(crate) fn self_source_label(msg: &ChatMessage) -> &'static str {
    if !msg.sender.is_empty() {
        "prefix"
    } else {
        "fallback"
    }
}

pub(crate) fn should_emit_session_snapshot(
    snapshot: &ax_reader::SessionItemSnapshot,
    state: &SessionListenState,
) -> bool {
    if snapshot.preview_body.is_empty() {
        return false;
    }
    snapshot.preview_body != state.last_preview_body
}

pub(crate) fn apply_sender_defaults(
    messages: &mut [ChatMessage],
    chat_name: &str,
    chat_kind: ChatKind,
) {
    if matches!(chat_kind, ChatKind::Group) {
        return;
    }
    for msg in messages {
        if !msg.is_self && msg.sender.is_empty() {
            msg.sender = chat_name.to_string();
        }
    }
}

pub(crate) fn trim_for_log(text: &str, max_chars: usize) -> String {
    let normalized = ax_reader::normalize_for_match(text);
    let mut out = normalized.chars().take(max_chars).collect::<String>();
    if normalized.chars().count() > max_chars {
        out.push('…');
    }
    out
}

pub(crate) fn preview_body_matches_message(
    preview_body: &str,
    message_content: &str,
    preview_body_key: Option<&str>,
) -> bool {
    let normalized_preview = ax_reader::normalize_for_match(preview_body);
    let normalized_message = ax_reader::normalize_for_match(message_content);
    if normalized_preview.is_empty() || normalized_message.is_empty() {
        return false;
    }

    if let Some(key) = preview_body_key {
        if !key.is_empty() && key == ax_reader::prefix8_key(&normalized_message) {
            return true;
        }
    } else if ax_reader::is_same_message_prefix8(&normalized_preview, &normalized_message) {
        return true;
    }

    let preview_len = normalized_preview.chars().count();
    let message_len = normalized_message.chars().count();
    (preview_len < 8 && normalized_message.starts_with(&normalized_preview))
        || (message_len < 8 && normalized_preview.starts_with(&normalized_message))
}

pub(crate) fn apply_session_preview_sender_hint(
    messages: &mut [ChatMessage],
    preview_text: &str,
    chat_kind: &mut ChatKind,
    unread_increased: bool,
) {
    if messages.is_empty() {
        return;
    }
    let (sender, preview_body) = ax_reader::parse_session_preview_line(preview_text);
    let Some(preview_body) = preview_body else {
        return;
    };
    if let Some(last) = messages.last_mut() {
        let text_matched = preview_body_matches_message(&preview_body, &last.content, None);
        let image_equivalent =
            is_image_placeholder_like(&preview_body) && is_image_placeholder_like(&last.content);
        let matched = text_matched || image_equivalent;

        match sender {
            Some(sender) if !sender.is_empty() => {
                *chat_kind = ChatKind::Group;
                if matched {
                    last.sender = sender;
                    last.is_self = false;
                    debug!(
                        "preview_hint matched(group) body='{}' latest='{}' -> sender='{}'",
                        trim_for_log(&preview_body, 24),
                        trim_for_log(&last.content, 24),
                        last.sender,
                    );
                }
            }
            _ => {
                if *chat_kind != ChatKind::Group {
                    *chat_kind = ChatKind::Private;
                }
                if matched {
                    let is_self_hint = if matches!(*chat_kind, ChatKind::Group) {
                        true
                    } else if unread_increased {
                        false
                    } else {
                        true
                    };
                    last.sender.clear();
                    last.is_self = is_self_hint;
                    debug!(
                        "preview_hint no_sender kind={} matched={} unread_inc={} body='{}' latest='{}' -> is_self={}",
                        chat_kind.as_str(),
                        matched,
                        unread_increased,
                        trim_for_log(&preview_body, 24),
                        trim_for_log(&last.content, 24),
                        last.is_self,
                    );
                }
            }
        }
    }
}

pub(crate) fn cleanup_preview_sender_hints(
    cache: &mut HashMap<String, PreviewSenderHint>,
    now: Instant,
) {
    cache.retain(|_, hint| {
        !hint.consumed && now.duration_since(hint.updated_at) <= PREVIEW_SENDER_HINT_TTL
    });
}

pub(crate) fn remember_preview_sender_hint(
    cache: &mut HashMap<String, PreviewSenderHint>,
    snapshot: &ax_reader::SessionItemSnapshot,
    now: Instant,
) {
    if !snapshot.has_sender_prefix {
        return;
    }

    let Some(sender) = snapshot
        .sender_hint
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    else {
        return;
    };

    let preview_body = snapshot.preview_body.trim();
    if preview_body.is_empty() {
        return;
    }

    cache.insert(
        snapshot.chat_name.clone(),
        PreviewSenderHint {
            sender: sender.to_string(),
            preview_body: preview_body.to_string(),
            preview_body_key: ax_reader::prefix8_key(preview_body),
            unread_count: snapshot.unread_count,
            updated_at: now,
            consumed: false,
        },
    );
}

pub(crate) fn apply_cached_preview_sender_hint(
    chat_name: &str,
    new_messages: &mut [ChatMessage],
    chat_kind: &mut ChatKind,
    cache: &mut HashMap<String, PreviewSenderHint>,
    now: Instant,
) -> bool {
    let Some(hint) = cache.get_mut(chat_name) else {
        return false;
    };

    if hint.consumed || now.duration_since(hint.updated_at) > PREVIEW_SENDER_HINT_TTL {
        return false;
    }

    for msg in new_messages.iter_mut().rev() {
        let text_matched = preview_body_matches_message(
            &hint.preview_body,
            &msg.content,
            Some(&hint.preview_body_key),
        );
        let image_equivalent = is_image_placeholder_like(&hint.preview_body)
            && is_image_placeholder_like(&msg.content);
        if !text_matched && !image_equivalent {
            continue;
        }

        *chat_kind = ChatKind::Group;
        msg.sender = hint.sender.clone();
        msg.is_self = false;
        hint.consumed = true;
        debug!(
            "preview_hint cache matched chat='{}' unread={} body='{}' latest='{}' -> sender='{}'",
            chat_name,
            hint.unread_count,
            trim_for_log(&hint.preview_body, 24),
            trim_for_log(&msg.content, 24),
            msg.sender,
        );
        return true;
    }

    false
}

pub(crate) fn is_image_placeholder_like(text: &str) -> bool {
    let normalized = ax_reader::normalize_for_match(text);
    if normalized.is_empty() {
        return false;
    }
    let stripped = normalized
        .trim_matches(|c| matches!(c, '[' | ']' | '【' | '】' | '(' | ')'))
        .to_lowercase();
    matches!(
        stripped.as_str(),
        "图片" | "image" | "images" | "photo" | "photos" | "照片"
    )
}

pub(crate) fn inherit_sender_from_reference(
    current: &mut [ChatMessage],
    reference: &[ChatMessage],
) {
    if current.is_empty() || reference.is_empty() {
        return;
    }

    let mut sender_bag: HashMap<String, Vec<String>> = HashMap::new();
    for msg in reference.iter().rev() {
        if !msg.sender.is_empty() {
            sender_bag
                .entry(msg.content.clone())
                .or_default()
                .push(msg.sender.clone());
        }
    }

    for msg in current.iter_mut().rev() {
        if !msg.sender.is_empty() {
            continue;
        }
        if let Some(candidates) = sender_bag.get_mut(&msg.content) {
            if let Some(sender) = candidates.pop() {
                msg.sender = sender;
            }
        }
    }
}

pub(crate) fn diff_messages(old: &[ChatMessage], new: &[ChatMessage]) -> DiffResult {
    if old.is_empty() || new.is_empty() {
        return DiffResult {
            new_messages: vec![],
            anchor_failed: false,
        };
    }

    if new.len() == old.len() && contents_match(old, new) {
        return DiffResult {
            new_messages: vec![],
            anchor_failed: false,
        };
    }

    if new.len() > old.len() {
        let old_len = old.len();
        if contents_match(&new[..old_len], old) {
            return DiffResult {
                new_messages: new[old_len..].to_vec(),
                anchor_failed: false,
            };
        }
    }

    if let Some(result) = anchor_diff_progressive(old, new) {
        return DiffResult {
            new_messages: result,
            anchor_failed: false,
        };
    }

    DiffResult {
        new_messages: bag_diff(old, new),
        anchor_failed: true,
    }
}

fn msg_identity_content(message: &ChatMessage) -> &str {
    &message.content
}

fn contents_match(a: &[ChatMessage], b: &[ChatMessage]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter()
        .zip(b.iter())
        .all(|(left, right)| msg_identity_content(left) == msg_identity_content(right))
}

pub(crate) fn anchor_diff_progressive(
    old: &[ChatMessage],
    new: &[ChatMessage],
) -> Option<Vec<ChatMessage>> {
    if old.is_empty() || new.is_empty() {
        return Some(vec![]);
    }

    let max_anchor = 3.min(old.len());
    for anchor_size in (1..=max_anchor).rev() {
        let anchor: Vec<&str> = old[old.len() - anchor_size..]
            .iter()
            .map(msg_identity_content)
            .collect();

        for index in 0..new.len() {
            if index + anchor_size > new.len() {
                break;
            }
            let window: Vec<&str> = new[index..index + anchor_size]
                .iter()
                .map(msg_identity_content)
                .collect();
            if window == anchor {
                let after = index + anchor_size;
                if after < new.len() {
                    return Some(new[after..].to_vec());
                }
                return Some(vec![]);
            }
        }
    }

    None
}

pub(crate) fn bag_diff(old: &[ChatMessage], new: &[ChatMessage]) -> Vec<ChatMessage> {
    let mut bag: HashMap<&str, usize> = HashMap::new();
    for message in old {
        *bag.entry(msg_identity_content(message)).or_default() += 1;
    }
    let mut result = Vec::new();
    for message in new {
        let key = msg_identity_content(message);
        match bag.get_mut(&key) {
            Some(count) if *count > 0 => {
                *count -= 1;
            }
            _ => {
                result.push(message.clone());
            }
        }
    }
    result
}

pub(crate) fn should_forward_session_preview(
    use_right_panel_details: bool,
    snapshot_chat_name: &str,
    active_chat_name: &str,
) -> bool {
    !use_right_panel_details || snapshot_chat_name != active_chat_name
}

pub(crate) fn should_forward_sidebar_chat(event_chat_name: &str) -> bool {
    !event_chat_name.is_empty()
}

pub(crate) fn short_error_text(message: &str) -> String {
    const MAX_CHARS: usize = 120;
    let shortened: String = message.chars().take(MAX_CHARS).collect();
    if message.chars().count() > MAX_CHARS {
        format!("{shortened}...")
    } else {
        shortened
    }
}
