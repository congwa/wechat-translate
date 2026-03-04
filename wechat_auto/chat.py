import time

from .controls import find_search_box, find_session_list, normalize_session_name
from .logger import log


def _match_session_name(target: str, raw_name: str) -> bool:
    current = normalize_session_name(raw_name)
    if not current:
        return False
    if current == target:
        return True
    # 某些版本会话名会带特殊前后缀，做一次温和模糊匹配
    return target in current or current in target


def open_chat(window, name: str) -> bool:
    """优先会话列表点击，失败后回退搜索框。"""
    if not window.Exists():
        log("微信窗口无效")
        return False

    window.SwitchToThisWindow()
    time.sleep(0.4)

    session_list = find_session_list(window)
    if session_list and session_list.Exists(0.4):
        target_item = None
        for item in session_list.GetChildren():
            if _match_session_name(name, item.Name or ""):
                target_item = item
                break

        if target_item:
            target_item.Click(simulateMove=False)
            log(f"已在会话列表中定位并点击：{name}")
            time.sleep(1.2)
            return True

    log(f"会话列表未命中 {name}，回退搜索方式")
    search_box = find_search_box(window)
    if not search_box or not search_box.Exists(0.4):
        log("未找到搜索框")
        return False

    search_box.Click()
    time.sleep(0.3)
    search_box.SendKeys("{Ctrl}a")
    time.sleep(0.1)
    search_box.SendKeys(name)
    time.sleep(0.8)
    search_box.SendKeys("{Enter}")
    time.sleep(1.0)

    log(f"已通过搜索进入聊天：{name}")
    return True
