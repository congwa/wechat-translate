from .controls import (
    find_message_list,
    find_session_list,
    is_meaningful_message_text,
    is_unread_session,
    normalize_session_name,
)
from .logger import log


def extract_wechat_name(raw_name):
    """兼容旧接口：提取会话名称。"""
    return normalize_session_name(raw_name)


def get_unread_chats(window):
    """获取所有有未读消息的联系人名称。"""
    session_list = find_session_list(window)
    if not session_list or not session_list.Exists(0.3):
        log("未找到会话列表，无法提取未读会话")
        return []

    unread_chats = []
    for item in session_list.GetChildren():
        raw_name = item.Name or ""
        if not is_unread_session(raw_name):
            continue

        name = normalize_session_name(raw_name)
        if not name or name == "折叠的聊天":
            continue

        if name not in unread_chats:
            unread_chats.append(name)

    return unread_chats


def get_last_message(window):
    """获取当前聊天窗口的最后一条有效消息文本。"""
    try:
        msg_list = find_message_list(window)
        if not msg_list or not msg_list.Exists(0.3):
            log("未找到消息列表")
            return None

        all_msgs = msg_list.GetChildren()
        if not all_msgs:
            return None

        # 从末尾回扫，跳过纯时间节点
        for msg_item in reversed(all_msgs):
            text_candidates = []
            name = (msg_item.Name or "").strip()
            if name:
                text_candidates.append(name)

            try:
                for child in msg_item.GetChildren():
                    t = (child.Name or "").strip()
                    if t:
                        text_candidates.append(t)
            except Exception:
                pass

            for text in text_candidates:
                if is_meaningful_message_text(text):
                    return text

        return None
    except Exception as e:
        log(f"读取消息失败: {e}")
        return None
