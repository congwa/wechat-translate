import ctypes
import time
from ctypes import wintypes

import uiautomation as auto

from .controls import find_session_list, normalize_session_name
from .logger import log


class _WinApi:
    PROCESS_QUERY_LIMITED_INFORMATION = 0x1000
    SW_SHOW = 5
    SW_RESTORE = 9

    def __init__(self):
        self.user32 = ctypes.windll.user32
        self.kernel32 = ctypes.windll.kernel32

        self.EnumWindows = self.user32.EnumWindows
        self.EnumWindowsProc = ctypes.WINFUNCTYPE(
            ctypes.c_bool, wintypes.HWND, wintypes.LPARAM
        )
        self.GetWindowTextLengthW = self.user32.GetWindowTextLengthW
        self.GetWindowTextW = self.user32.GetWindowTextW
        self.GetClassNameW = self.user32.GetClassNameW
        self.GetWindowThreadProcessId = self.user32.GetWindowThreadProcessId
        self.IsWindowVisible = self.user32.IsWindowVisible
        self.ShowWindow = self.user32.ShowWindow
        self.SetForegroundWindow = self.user32.SetForegroundWindow

        self.OpenProcess = self.kernel32.OpenProcess
        self.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
        self.OpenProcess.restype = wintypes.HANDLE

        self.QueryFullProcessImageNameW = self.kernel32.QueryFullProcessImageNameW
        self.QueryFullProcessImageNameW.argtypes = [
            wintypes.HANDLE,
            wintypes.DWORD,
            wintypes.LPWSTR,
            ctypes.POINTER(wintypes.DWORD),
        ]
        self.QueryFullProcessImageNameW.restype = wintypes.BOOL

        self.CloseHandle = self.kernel32.CloseHandle

    def window_title(self, hwnd) -> str:
        size = self.GetWindowTextLengthW(hwnd)
        buf = ctypes.create_unicode_buffer(size + 1)
        self.GetWindowTextW(hwnd, buf, size + 1)
        return buf.value

    def window_class(self, hwnd) -> str:
        buf = ctypes.create_unicode_buffer(256)
        self.GetClassNameW(hwnd, buf, 256)
        return buf.value

    def process_image_name(self, pid: int) -> str:
        handle = self.OpenProcess(self.PROCESS_QUERY_LIMITED_INFORMATION, False, pid)
        if not handle:
            return ""
        try:
            size = wintypes.DWORD(260)
            buf = ctypes.create_unicode_buffer(size.value)
            ok = self.QueryFullProcessImageNameW(handle, 0, buf, ctypes.byref(size))
            if not ok:
                return ""
            return buf.value.split("\\")[-1]
        finally:
            self.CloseHandle(handle)

    def activate_hwnd(self, hwnd: int):
        self.ShowWindow(hwnd, self.SW_RESTORE)
        self.ShowWindow(hwnd, self.SW_SHOW)
        self.SetForegroundWindow(hwnd)


class WeChatWindow:
    def __init__(self):
        self.window = None
        self._winapi = _WinApi()

    def _enum_wechat_windows(self) -> list[dict]:
        targets = {"WeChat.exe", "Weixin.exe"}
        result = []

        def callback(hwnd, _):
            try:
                title = self._winapi.window_title(hwnd)
                cls = self._winapi.window_class(hwnd)

                pid = wintypes.DWORD()
                self._winapi.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
                exe_name = self._winapi.process_image_name(pid.value)

                if exe_name not in targets:
                    return True

                score = 0
                if self._winapi.IsWindowVisible(hwnd):
                    score += 100
                if title in ("微信", "Weixin"):
                    score += 80
                if "Qt" in cls or "WeChatMainWnd" in cls or "UnrealWindow" in cls:
                    score += 50
                if "MainWindow" in cls:
                    score += 120
                cls_lower = cls.lower()
                if (
                    "popover" in cls_lower
                    or "trayicon" in cls_lower
                    or "shadow" in cls_lower
                    or "toolsavebits" in cls_lower
                ):
                    score -= 180
                if title:
                    score += 15

                if score > 0:
                    result.append(
                        {
                            "hwnd": int(hwnd),
                            "pid": int(pid.value),
                            "exe": exe_name,
                            "title": title,
                            "class": cls,
                            "score": score,
                        }
                    )
            except Exception:
                pass
            return True

        self._winapi.EnumWindows(self._winapi.EnumWindowsProc(callback), 0)
        result.sort(key=lambda x: x["score"], reverse=True)
        return result

    def _window_from_native(self):
        candidates = self._enum_wechat_windows()
        if not candidates:
            return None

        for picked in candidates:
            try:
                self._winapi.activate_hwnd(picked["hwnd"])
                time.sleep(0.4)
                ctrl = auto.ControlFromHandle(picked["hwnd"])
            except Exception as e:
                log(f"句柄挂接失败(hwnd={picked['hwnd']})，原因：{e}")
                continue

            if ctrl and ctrl.Exists(0.8):
                cls_name = (ctrl.ClassName or "").strip()
                cls_lower = cls_name.lower()
                is_main_window = (
                    "mainwindow" in cls_lower
                    or cls_name in ("WeChatMainWndForPC", "WeChatMainWnd", "QWidget", "UnrealWindow")
                )
                if not is_main_window:
                    log(f"跳过非主窗口句柄(hwnd={picked['hwnd']}, class='{cls_name}')")
                    continue
                log(
                    "通过进程句柄定位微信窗口成功，"
                    f"exe={picked['exe']} class='{ctrl.ClassName}' title='{ctrl.Name}'"
                )
                return ctrl
        return None

    def _window_from_uia_fallback(self):
        possible_classes = [
            "mmui::MainWindow",
            "WeChatMainWndForPC",
            "WeChatMainWnd",
            "WeChatLoginWndForPC",
            "QWidget",
            "UnrealWindow",
        ]
        for cls in possible_classes:
            ctrl = auto.WindowControl(searchDepth=1, ClassName=cls)
            if ctrl.Exists(0.3):
                log(f"通过 UIA 类名定位到窗口，ClassName = '{cls}'")
                return ctrl

        # 兜底：标题包含“微信”且排除命令行窗口
        ctrl = auto.WindowControl(searchDepth=1, NameContains="微信")
        if ctrl.Exists(0.3) and "cmd.exe" not in (ctrl.Name or "").lower():
            log(f"通过标题兜底定位到窗口，ClassName = '{ctrl.ClassName or '未知'}'")
            return ctrl
        return None

    def _activate_window(self):
        hwnd = 0
        try:
            hwnd = int(self.window.NativeWindowHandle)
        except Exception:
            hwnd = 0

        if hwnd:
            self._winapi.activate_hwnd(hwnd)
            time.sleep(0.3)

        self.window.SwitchToThisWindow()
        time.sleep(0.3)
        if self.window.IsMinimize():
            self.window.Restore()
            time.sleep(0.6)
            self.window.SwitchToThisWindow()
            time.sleep(0.3)

    def load(self) -> bool:
        """
        定位并激活微信主窗口。优先按进程句柄定位，避免误匹配其他窗口。
        """
        log("尝试定位并激活微信窗口...")
        self.window = self._window_from_native()
        if not self.window:
            log("进程句柄定位失败，尝试 UIA 兜底定位...")
            self.window = self._window_from_uia_fallback()

        if not self.window or not self.window.Exists():
            log("最终仍未找到微信主窗口，请手动打开微信并确保已登录")
            return False

        self._activate_window()
        log("微信窗口已成功激活并置于前台")
        return True

    def get_current_sessions(self) -> list:
        """获取当前会话名称列表（前 30 个）。"""
        if not self.window or not self.window.Exists():
            log("微信窗口不存在，无法获取会话列表")
            return []

        self._activate_window()
        session_list = find_session_list(self.window)
        if not session_list or not session_list.Exists(0.5):
            log("未找到会话列表控件")
            return []

        names = []
        for item in session_list.GetChildren()[:30]:
            raw = item.Name or ""
            name = normalize_session_name(raw)
            if name and name not in names:
                names.append(name)

        log(f"获取到 {len(names)} 个会话")
        return names

    def get_window(self):
        return self.window
