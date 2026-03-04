# examples/auto_reply.py
from wechat_auto import WxAuto
import time
import signal
import sys


def my_reply(name: str, content: str, wx) -> str | None:
    """
    自动回复回调函数
    :param name: 发送者名称
    :param content: 消息内容
    :param wx: WxAuto 实例，用于调用 request_stop()
    """
    content = content.lower().strip()

    # 停止命令
    if content in ["关机", "停止", "退出", "下线", "bye", "再见"]:
        wx.stop_listening()  # 关键：请求停止监听
        return "好的，机器人已下线，再见！"

    # 普通自动回复
    if "在吗" in content or "在不在" in content or "有人吗" in content:
        return "在的！有事请说～"

    if "hello" in content or "hi" in content or "你好" in content:
        return "Hello！你好啊～"

    if "天气" in content:
        return "今天天气不错，适合出门散步哦"

    if "时间" in content or "几点" in content:
        from datetime import datetime

        now = datetime.now().strftime("%H:%M:%S")
        return f"现在是 {now}"

    # 默认不回复
    return None


if __name__ == "__main__":
    wx = WxAuto()
    if not wx.load_wechat():
        print("未能找到微信窗口，请先手动打开电脑版微信并登录")
        sys.exit(1)

    print("微信机器人已启动！发送 '关机' 可停止程序")
    wx.listen(callback=my_reply, interval=10, auto_reply=True)

    # 支持 Ctrl+C 优雅退出
    def signal_handler(sig, frame):
        print("\n正在退出...")
        wx.stop_listening()
        time.sleep(1)
        sys.exit(0)

    signal.signal(signal.SIGINT, signal_handler)

