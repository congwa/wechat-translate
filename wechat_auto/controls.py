import re
from typing import Optional

import uiautomation as auto


_UNREAD_RE = re.compile(r"\[\d+条\]|\d+条新消息|未读")
_TIME_ONLY_RE = re.compile(r"^(?:昨天|今天|星期[一二三四五六日天])?\s*\d{1,2}:\d{2}$")


def normalize_session_name(raw_name: str) -> str:
    """提取会话项第一行作为会话名称。"""
    if not raw_name:
        return ""
    lines = [line.strip() for line in raw_name.splitlines() if line.strip()]
    if not lines:
        return ""
    name = lines[0]
    name = re.sub(r"\s*\d+[+]?\s*条新消息$", "", name)
    return name.strip()


def is_unread_session(raw_name: str) -> bool:
    if not raw_name:
        return False
    return bool(_UNREAD_RE.search(raw_name))


def is_meaningful_message_text(text: str) -> bool:
    if not text:
        return False
    s = text.strip()
    if not s:
        return False
    if _TIME_ONLY_RE.match(s):
        return False
    return True


def _iter_controls(root, control_type: str, max_nodes: int = 800):
    stack = [(root, 0)]
    visited = 0
    while stack and visited < max_nodes:
        node, depth = stack.pop()
        visited += 1
        try:
            if node.ControlTypeName == control_type:
                yield node
            if depth >= 8:
                continue
            children = node.GetChildren()
            for child in reversed(children):
                stack.append((child, depth + 1))
        except Exception:
            continue


def _exists(ctrl, timeout: float = 0.2) -> bool:
    try:
        return bool(ctrl and ctrl.Exists(timeout))
    except Exception:
        return False


def find_session_list(window) -> Optional[auto.Control]:
    candidates = [
        window.ListControl(AutomationId="session_list"),
        window.ListControl(Name="会话"),
        window.ListControl(AutomationId="search_list"),
    ]
    for ctrl in candidates:
        if _exists(ctrl):
            return ctrl

    best = None
    best_score = -1
    for ctrl in _iter_controls(window, "ListControl"):
        try:
            score = 0
            aid = (ctrl.AutomationId or "").lower()
            cls = ctrl.ClassName or ""
            name = ctrl.Name or ""
            children = ctrl.GetChildren()

            if "session" in aid:
                score += 120
            if "search_list" in aid:
                score += 90
            if name == "会话":
                score += 60
            if "XTableView" in cls:
                score += 20
            score += min(len(children), 50)
            if children and "ChatSession" in (children[0].ClassName or ""):
                score += 100

            if score > best_score:
                best_score = score
                best = ctrl
        except Exception:
            continue
    return best


def find_message_list(window) -> Optional[auto.Control]:
    candidates = [
        window.ListControl(AutomationId="chat_message_list"),
        window.ListControl(Name="消息"),
    ]
    for ctrl in candidates:
        if _exists(ctrl):
            return ctrl

    best = None
    best_score = -1
    for ctrl in _iter_controls(window, "ListControl"):
        try:
            score = 0
            aid = (ctrl.AutomationId or "").lower()
            cls = ctrl.ClassName or ""
            name = ctrl.Name or ""
            children = ctrl.GetChildren()

            if "message" in aid:
                score += 120
            if name == "消息":
                score += 80
            if "RecyclerListView" in cls:
                score += 40
            if children and "Chat" in (children[0].ClassName or ""):
                score += 80
            score += min(len(children), 50)

            if score > best_score:
                best_score = score
                best = ctrl
        except Exception:
            continue
    return best


def find_search_box(window) -> Optional[auto.Control]:
    direct = [
        window.EditControl(Name="搜索"),
        window.EditControl(ClassName="mmui::XValidatorTextEdit"),
    ]
    for ctrl in direct:
        if _exists(ctrl):
            return ctrl

    best = None
    best_score = -1
    for ctrl in _iter_controls(window, "EditControl"):
        try:
            score = 0
            name = ctrl.Name or ""
            cls = ctrl.ClassName or ""
            aid = (ctrl.AutomationId or "").lower()

            if "搜索" in name:
                score += 120
            if "validator" in cls.lower():
                score += 70
            if "chat_input" in aid or "chatinput" in cls.lower():
                score -= 100

            if score > best_score:
                best_score = score
                best = ctrl
        except Exception:
            continue
    return best

