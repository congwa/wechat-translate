//! 监听消息 ingest 规则服务：负责把微信 UI 轮询结果转成“哪些是新增消息、谁发的、该不该推给 sidebar”，
//! 这是监听链路里最核心的业务判定层，避免这些规则继续散落在 monitor loop 或 TaskManager 里。
use crate::adapter::ax_reader::{self, ChatMessage};
use log::debug;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// DiffResult 表示一轮消息比较后的新增消息结果。
/// `anchor_failed=true` 说明顺序锚点法失败，当前结果来自更保守的 bag diff 回退。
#[derive(Debug, Clone)]
pub(crate) struct DiffResult {
    pub new_messages: Vec<ChatMessage>,
    pub anchor_failed: bool,
}

/// PreviewSenderHint 保存会话预览里暂存的“发言人提示”。
/// 业务上用于把 session preview 里出现的发送人线索补到后续真正读到的消息上。
#[derive(Debug, Clone)]
pub(crate) struct PreviewSenderHint {
    pub sender: String,
    pub preview_body: String,
    pub preview_body_key: String,
    pub unread_count: u32,
    pub updated_at: Instant,
    pub consumed: bool,
}

/// SessionListenState 是每个会话在 session preview 模式下的上次已知状态。
/// 它只记录预览文本和未读数，用于判断是否需要把本轮快照视为新变化。
#[derive(Debug, Clone, Default)]
pub(crate) struct SessionListenState {
    pub last_preview_body: String,
    pub last_unread: u32,
}

/// ChatKind 表示当前聊天被推断成群聊、私聊还是未知。
/// 这会直接影响 sender 默认值、自发消息判断和 sidebar 展示语义。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChatKind {
    Group,
    Private,
    Unknown,
}

impl ChatKind {
    /// 返回聊天类型的对外字符串表示，供入库、事件和 snapshot 一致复用。
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            ChatKind::Group => "group",
            ChatKind::Private => "private",
            ChatKind::Unknown => "unknown",
        }
    }
}

/// 预览 sender hint 的有效期。
/// 业务上只在最近几轮轮询里承认这份线索，避免旧预览错误污染后续消息。
pub(crate) const PREVIEW_SENDER_HINT_TTL: Duration = Duration::from_secs(12);

/// 标记一条消息的“self_source”来源，用于调试 sender 推断是来自前缀还是兜底判定。
pub(crate) fn self_source_label(msg: &ChatMessage) -> &'static str {
    if !msg.sender.is_empty() {
        "prefix"
    } else {
        "fallback"
    }
}

/// 判断一条 session snapshot 是否值得继续处理。
/// 业务规则：空预览不处理；只有预览正文变化时才视为新的候选事件。
pub(crate) fn should_emit_session_snapshot(
    snapshot: &ax_reader::SessionItemSnapshot,
    state: &SessionListenState,
) -> bool {
    if snapshot.preview_body.is_empty() {
        return false;
    }
    snapshot.preview_body != state.last_preview_body
}

/// 为私聊消息补默认 sender。
/// 业务规则：群聊不做兜底，因为群聊 sender 误补会污染后续发言人总结和 sidebar 展示。
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

/// 对日志文本做规范化与截断。
/// 业务上用于输出稳定、短小、可比对的调试日志，避免原始消息过长污染日志页。
pub(crate) fn trim_for_log(text: &str, max_chars: usize) -> String {
    let normalized = ax_reader::normalize_for_match(text);
    let mut out = normalized.chars().take(max_chars).collect::<String>();
    if normalized.chars().count() > max_chars {
        out.push('…');
    }
    out
}

/// 判断 session preview 正文是否能和真正的消息内容视为同一条消息。
/// 业务上支持 prefix8 匹配和短文本前缀匹配，减少“预览提示”和“详情正文”之间的错位。
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

/// 把当前 session preview 里的 sender 线索应用到刚读到的消息列表。
/// 业务上这是“预览补 sender”的第一道规则，用于群聊 preview 与聊天正文衔接。
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

/// 清理过期或已消费的 preview sender hint。
/// 业务规则：过期线索必须尽快丢弃，避免把旧会话预览误补到新消息。
pub(crate) fn cleanup_preview_sender_hints(
    cache: &mut HashMap<String, PreviewSenderHint>,
    now: Instant,
) {
    cache.retain(|_, hint| {
        !hint.consumed && now.duration_since(hint.updated_at) <= PREVIEW_SENDER_HINT_TTL
    });
}

/// 记住一条 session preview sender hint。
/// 业务规则：只有 preview 明确带 sender 前缀时才缓存，避免把不确定线索长期保存。
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

/// 用缓存中的 preview sender hint 为新消息补 sender。
/// 业务上这是“预览补 sender”的第二道规则，用于跨轮询补齐刚刚新增的详情消息。
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

/// 判断一段文本是否可以视为“图片占位文本”。
/// 业务上用于把 preview 的 `[图片]` 和聊天详情里的图片消息视为同一类消息。
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

/// 当当前消息缺少 sender 时，尝试从参考消息集合中按内容回填 sender。
/// 业务上用于把预览或低质量消息与已有高质量消息对齐，减少 sender 丢失。
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

/// 比较前后两轮消息列表，尽量找出真正新增的消息。
/// 业务策略：优先完全相等判断，再尝试尾部追加、锚点匹配，最后才回退到 bag diff。
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

/// 返回消息比较时的“身份正文”。
/// 当前业务上只用 content 参与比较，便于在 sender 仍未补齐时先判断新增消息。
fn msg_identity_content(message: &ChatMessage) -> &str {
    &message.content
}

/// 判断两组消息在当前比较规则下是否完全相同。
/// 业务上用于快速识别“本轮轮询其实没有新增消息”。
fn contents_match(a: &[ChatMessage], b: &[ChatMessage]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter()
        .zip(b.iter())
        .all(|(left, right)| msg_identity_content(left) == msg_identity_content(right))
}

/// 用渐进式锚点算法寻找新增消息段。
/// 业务上优先相信“尾部锚点”而不是完全 bag diff，以保留消息时序。
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

/// 用 bag diff 回退比较消息集合。
/// 业务上只在顺序锚点完全失效时使用，宁可保守多报，也尽量不漏掉新增消息。
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

/// 判断 session preview 消息是否应该直接转发入库。
/// 业务规则：若右侧详情面板已开启，则活跃聊天的 preview 不再重复入库，避免和高质量消息双写。
pub(crate) fn should_forward_session_preview(
    use_right_panel_details: bool,
    snapshot_chat_name: &str,
    active_chat_name: &str,
) -> bool {
    !use_right_panel_details || snapshot_chat_name != active_chat_name
}

/// 判断某个聊天是否允许作为 sidebar 当前聊天被继续推送。
/// 当前业务规则只要求 chat_name 非空，避免把空聊天名推给前端快照。
pub(crate) fn should_forward_sidebar_chat(event_chat_name: &str) -> bool {
    !event_chat_name.is_empty()
}

/// 把过长的错误文本截断成适合前端展示和日志观察的短字符串。
pub(crate) fn short_error_text(message: &str) -> String {
    const MAX_CHARS: usize = 120;
    let shortened: String = message.chars().take(MAX_CHARS).collect();
    if message.chars().count() > MAX_CHARS {
        format!("{shortened}...")
    } else {
        shortened
    }
}
