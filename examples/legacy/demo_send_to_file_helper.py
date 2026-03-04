# examples/demo_send_to_file_helper.py
from wechat_auto import WxAuto
import time

if __name__ == "__main__":
    print("=== 微信PC版自动化工具 - 测试发送到【文件传输助手】===\n")

    wx = WxAuto()

    if not wx.load_wechat():
        print("错误：无法加载微信，请确保微信已登录")
        exit(1)

    target = "文件传输助手"

    # 发送文本消息
    message = f"""你好！这是 wechat-pc-auto v1.1.2 测试
多行消息支持正常
自动化发送成功！
时间：{time.strftime("%Y-%m-%d")}"""

    print(f"正在发送消息给：{target}")
    wx.send_msg(message, who=target)

    # 发送文件（请修改为你的真实文件路径）
    files = [
        r"E:\test.png",
        r"E:\test.xlsx",
    ]

    print(f"正在发送 {len(files)} 个文件...")
    success = wx.send_files(files, who=target)

    if success:
        print("\n所有操作完成！请检查【文件传输助手】是否收到消息和文件")
    else:
        print("\n文件发送失败，请检查路径是否正确")
