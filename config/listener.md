# listener.json 配置说明

## 启动参数约束
- `examples/sidebar_translate_listener.py` 启动时仅保留 `--config`。
- 当前监听主链路为 `session-only`，其余运行行为（监听、翻译、展示、日志、调试）统一从 `listener.json` 读取。

## 完整配置示例
```json
{
  "listen": {
    "targets": [
      "ssh 前端进阶交流群3群「禁广告」"
    ],
    "interval_seconds": 1.0,
    "load_retry_seconds": 10.0,
    "session_preview_dedupe_window_seconds": 20.0,
    "focus_refresh": false,
    "worker_debug": false
  },
  "translate": {
    "enabled": true,
    "provider": "deeplx",
    "deeplx_url": "https://api.deeplx.org/<your-key>/translate",
    "source_lang": "auto",
    "target_lang": "EN",
    "timeout_seconds": 8.0
  },
  "display": {
    "english_only": true,
    "on_translate_fail": "show_cn_with_reason",
    "width": 420,
    "side": "right"
  },
  "logging": {
    "file": "logs/sidebar_listener.log"
  }
}
```

## 字段说明

### `listen`
- `targets`：监听目标数组。
  - 长度为 `1`：启动一个侧边栏窗口并监听该目标。
  - 长度 `>1`：仍然只启动一个侧边栏窗口，左侧菜单展示所有 target，点击切换右侧消息视图。
  - 监听层为“单 worker 一次扫描全部 target”。
- `interval_seconds`：轮询间隔（秒）。越小越实时，但占用更高。
  - 必须 `> 0`，否则启动阶段会直接报错退出。
- `load_retry_seconds`：微信未启动、未登录或重连时的重试间隔（秒），默认 `10.0`。
  - 必须 `> 0`，否则启动阶段会直接报错退出。
  - 该参数同时作用于“先启动程序后启动微信”和“微信运行中关闭后再次打开”的恢复等待。
- `session_preview_dedupe_window_seconds`：`session_preview` 去重窗口秒数，默认 `20.0`。
  - 这是当前链路最关键参数。
  - 值过小：会话预览抖动导致重复展示概率上升。
  - 值过大：群里短时间重复发送相同内容时，第二条可能被抑制。
- `focus_refresh`：是否每轮强制切回微信刷新 UIA。`true` 更稳但会抢焦点。
- `worker_debug`：是否输出 worker 调试日志（例如 `debug target=... session_preview=... unread=...`）。

### `translate`
- `enabled`：是否启用翻译。`true` 调用翻译服务，`false` 原文透传。
- `provider`：翻译提供方。当前支持 `deeplx` / `passthrough`。
- `deeplx_url`：DeepLX 接口地址。
- `source_lang`：源语言，`auto` 表示自动检测。
- `target_lang`：目标语言，例如 `EN`。
- `timeout_seconds`：翻译请求超时时间（秒）。
  - 必须 `> 0`，否则启动阶段会直接报错退出。
- `deeplx_url` 建议放占位值，真实密钥通过 `.env.local`（已忽略）覆盖。

### `display`
- `english_only`：`true` 时只显示翻译后的文本（替换原文展示）。
- `on_translate_fail`：翻译失败回退策略。
  - `show_cn_with_reason`：显示中文并附失败原因。
  - `show_cn`：只显示中文。
  - `show_reason`：只显示失败原因。
- `width`：侧边栏宽度。
  - 必须 `>= 280`，否则启动阶段会直接报错退出。
- `side`：侧边栏停靠位置，`left` 或 `right`。
- 置顶仅通过侧边栏头部“置顶”按钮手动切换，不再提供启动配置项。

### `logging`
- `file`：运行日志输出文件路径。相对路径按项目根目录解析（例如 `logs/sidebar_listener.log`）。
- 日志按大小自动轮转：默认单文件约 `10MB` 时切分，保留最近 `5` 个历史文件（`.1` ~ `.5`）。

## 当前消息渲染规则（代码行为）
- 图片占位文本会被过滤，不显示到侧边栏：`[图片]` / `[image]` / `[images]` / `[photo]`。
- 若消息是 `发送人: 正文` 格式，仅翻译“正文”，发送人姓名保持原样。
- 若消息不含发送人前缀，视为“自己消息”，在侧边栏右对齐显示。
- UI 不显示 `source=session_preview`，该字段仅用于内部日志。
- 当前侧边栏仅用于监听与展示，不提供消息发送输入框。
- 多目标模式下只保留一个窗口：左侧 target 菜单 + 右侧消息区；未选中目标的新消息会累计未读计数。
- 每个 target 的消息缓存上限固定为 `100` 条（超出后丢弃最旧消息）。
- 翻译在后台线程执行，避免网络抖动时卡住侧边栏 UI。
- 翻译队列是有界队列（默认上限 `300`）；队列满时会丢弃最旧待翻译任务并记录 `translate queue overflow` 日志。

## 多目标运行约束
- 当前主链路固定为 `session-only`，禁止恢复 `chat` / `mixed` 配置。
- 多目标监听不会再派生多个 worker；所有 targets 由同一个 worker 在一次 UIA 扫描中完成。
