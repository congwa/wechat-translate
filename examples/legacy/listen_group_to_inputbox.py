import argparse
import os
import sys
import time

import pyperclip

ROOT_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if ROOT_DIR not in sys.path:
    sys.path.insert(0, ROOT_DIR)

from wechat_auto import WxAuto
from wechat_auto.controls import (
    find_message_list,
    is_meaningful_message_text,
    normalize_session_name,
)


def _iter_controls(root, control_type: str, max_nodes: int = 600):
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


def find_chat_input(window):
    best = None
    best_score = -1
    for ctrl in _iter_controls(window, "EditControl"):
        try:
            score = 0
            aid = (ctrl.AutomationId or "").lower()
            cls = (ctrl.ClassName or "").lower()
            name = (ctrl.Name or "").strip()
            if "chat_input_field" in aid:
                score += 150
            if "chatinputfield" in cls:
                score += 120
            if name and name not in ("搜索",):
                score += 10
            if "validatortextedit" in cls:
                score -= 100

            if score > best_score:
                best_score = score
                best = ctrl
        except Exception:
            continue
    if best and best_score >= 100:
        return best
    return None


def write_to_input(window, text: str) -> bool:
    input_box = find_chat_input(window)
    if not input_box:
        return False

    input_box.Click()
    time.sleep(0.05)
    input_box.SendKeys("{Ctrl}a")
    time.sleep(0.05)
    pyperclip.copy(text)
    window.SendKeys("{Ctrl}v")
    return True


def get_last_message_signature(window):
    msg_list = find_message_list(window)
    if not msg_list or not msg_list.Exists(0.4):
        return None

    items = msg_list.GetChildren()
    if not items:
        return (0, "")

    text_value = ""
    for msg_item in reversed(items):
        candidates = []
        raw = (msg_item.Name or "").strip()
        if raw:
            candidates.append(raw)
        try:
            for child in msg_item.GetChildren():
                t = (child.Name or "").strip()
                if t:
                    candidates.append(t)
        except Exception:
            pass

        for text in candidates:
            if is_meaningful_message_text(text):
                text_value = text
                break
        if text_value:
            break

    return (len(items), text_value)


def ensure_target_chat(wx: WxAuto, target: str) -> bool:
    input_box = find_chat_input(wx.window)
    if input_box and input_box.Exists(0.2):
        current_name = normalize_session_name((input_box.Name or "").strip())
        if current_name == target:
            return True
    return wx.chat_with(target)


def main():
    parser = argparse.ArgumentParser(
        description="Listen one group and mirror message text into chat input box."
    )
    parser.add_argument("--group", required=True, help="Target group name")
    parser.add_argument("--interval", type=float, default=1.0, help="Poll interval")
    args = parser.parse_args()

    wx = WxAuto()
    if not wx.load_wechat():
        print("load_wechat failed")
        raise SystemExit(1)

    target = args.group.strip()
    if not wx.chat_with(target):
        print(f"open target chat failed: {target}")
        raise SystemExit(1)

    baseline = get_last_message_signature(wx.window)
    print(f"listener started, target={target}, interval={args.interval}, baseline={baseline}")

    last_sig = baseline
    while True:
        try:
            if not ensure_target_chat(wx, target):
                print("target chat not ready, retrying...")
                time.sleep(args.interval)
                continue

            sig = get_last_message_signature(wx.window)
            if sig and last_sig and sig != last_sig:
                _, msg = sig
                if msg:
                    ok = write_to_input(wx.window, msg)
                    if ok:
                        print(f"typed to input: {msg}")
                    else:
                        print(f"input box not found, skip: {msg}")
            if sig:
                last_sig = sig
        except KeyboardInterrupt:
            break
        except Exception as e:
            print(f"loop error: {e}")
        time.sleep(args.interval)


if __name__ == "__main__":
    main()
