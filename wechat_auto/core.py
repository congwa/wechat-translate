import time

from .chat import open_chat
from .listener import get_last_message, get_unread_chats
from .logger import log
from .sender import send_files, send_message
from .window import WeChatWindow


class WxAuto:
    def __init__(self):
        self._window_manager = WeChatWindow()
        self.window = None
        self._running = False

    def load_wechat(self) -> bool:
        """加载微信窗口"""
        success = self._window_manager.load()
        if success:
            self.window = self._window_manager.get_window()
        return success

    def get_current_sessions(self) -> list:
        """获取当前会话列表"""
        return self._window_manager.get_current_sessions()

    def chat_with(self, name: str) -> bool:
        """打开聊天"""
        if not self.window:
            return False
        return open_chat(self.window, name)

    def send_msg(self, msg: str, who: str = None) -> bool:
        """发送文本消息"""
        if who and not self.chat_with(who):
            return False
        send_message(self.window, msg)
        return True

    def send_files(self, file_paths: list[str], who: str = None) -> bool:
        """发送文件"""
        if who and not self.chat_with(who):
            return False
        return send_files(self.window, file_paths)

    def listen(self, callback, interval: float = 2, auto_reply: bool = False):
        """
        监听新消息，默认仅接收不回复。
        :param callback: def callback(name: str, content: str, wx: WxAuto) -> str | None
        :param interval: 检查间隔（秒）
        :param auto_reply: True 时 callback 返回文本会自动发送
        """
        self._running = True
        log("开始监听新消息...")

        # 去重缓存，避免 UI 结构刷新导致重复触发
        last_processed_msg = {}

        while self._running:
            try:
                window = self._window_manager.window
                if not window or not window.Exists():
                    time.sleep(interval)
                    continue

                unread_names = get_unread_chats(window)
                for name in unread_names:
                    minute_key = f"{name}_{int(time.time() // 60)}"
                    if minute_key in last_processed_msg:
                        continue

                    if not open_chat(window, name):
                        log(f"打开 [{name}] 聊天失败，跳过")
                        continue

                    time.sleep(0.8)
                    content = get_last_message(window)
                    if not content:
                        continue

                    content_key = f"{name}_{content}"
                    if content_key in last_processed_msg:
                        continue

                    log(f"收到来自 [{name}] 的新消息: {content}")

                    reply = callback(name, content, self) if callback else None
                    if auto_reply and reply:
                        send_message(window, reply)
                        log(f"已自动回复 [{name}]: {reply}")

                    last_processed_msg[minute_key] = time.time()
                    last_processed_msg[content_key] = time.time()

                if len(last_processed_msg) > 400:
                    last_processed_msg.clear()

            except Exception as e:
                log(f"监听运行中出错: {e}")

            time.sleep(interval)

    def listen_messages(self, callback, interval: float = 2):
        """仅接收消息，不执行自动回复。"""
        return self.listen(callback=callback, interval=interval, auto_reply=False)

    def stop_listening(self):
        """外部调用此方法可停止监听"""
        log("正在请求停止监听...")
        self._running = False
