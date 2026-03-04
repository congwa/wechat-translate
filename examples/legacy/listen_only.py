from wechat_auto import WxAuto


def on_message(name: str, content: str, _wx):
    print(f"[新消息] {name}: {content}")
    return None


if __name__ == "__main__":
    wx = WxAuto()
    if not wx.load_wechat():
        print("未能找到微信窗口，请先手动打开电脑版微信并登录")
        raise SystemExit(1)

    print("开始仅接收监听（不会自动回复）...")
    wx.listen_messages(callback=on_message, interval=2)
