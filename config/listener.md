# listener.json 配置说明

## 启动参数约束
- `listener_app/sidebar_translate_listener.py` 启动时仅保留 `--config`。
- 当前监听主链路为 `session-only`，其余运行行为（监听、翻译、展示、日志、TTS provider 选择、调试）统一从 `listener.json` 读取。

## 完整配置示例
```json
{
  "listen": {
    "targets": [
      "ssh 前端进阶交流群3群「禁广告」"
    ],
    "interval_seconds": 0.6,
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
    "tts_auto_read_active_chat": true,
    "on_translate_fail": "show_cn_with_reason",
    "width": 470,
    "side": "right"
  },
  "tts": {
    "provider": "doubao",
    "config_path": "config/doubao_tts.json"
  },
  "logging": {
    "file": "logs/sidebar_listener.log"
  }
}
```

## 字段说明

### `listen`
- `targets`：监听目标数组。
  - 启动时从 `listener.json` 读取。
  - 运行中可以在侧边栏顶部点击“添加群”，或在左侧菜单上右键删除 target；变更会回写 `listener.json`，并按“先停旧 worker、确认退出后再启动新 worker”的顺序生效。
  - 长度为 `1`：启动一个侧边栏窗口并监听该目标。
  - 长度 `>1`：仍然只启动一个侧边栏窗口，左侧菜单展示所有 target，点击切换右侧消息视图。
  - 监听层为“单 worker 一次扫描全部 target”。
  - 运行时不允许删到 `0` 个 target；若新增 target 已被其他侧边栏实例占用，会直接拒绝。
- `interval_seconds`：轮询间隔（秒），默认 `0.6`。越小越实时，但占用更高。
  - 必须 `>= 0.2`，否则启动阶段会直接报错退出。
  - 当前实现按“整轮采样周期”控速：UIA 扫描耗时会计入这一轮，不会再出现“扫完一轮再额外 sleep 一整段”的假慢。
  - 推荐先在 `0.5 ~ 0.8` 之间调；继续往下压会更灵敏，但会更频繁扫 UIA 树并增加 CPU 占用。
- `load_retry_seconds`：微信未启动、未登录或重连时的重试间隔（秒），默认 `10.0`。
  - 必须 `> 0`，否则启动阶段会直接报错退出。
  - 该参数同时作用于“先启动程序后启动微信”和“微信运行中关闭后再次打开”的恢复等待。
- `session_preview_dedupe_window_seconds`：`session_preview` 去重窗口秒数，默认 `20.0`。
  - 这是当前链路最关键参数。
  - 值过小：会话预览抖动导致重复展示概率上升。
  - 值过大：群里短时间重复发送相同内容时，第二条可能被抑制。
- `focus_refresh`：是否允许 worker 在“连续缺目标”或“未读快照长期不变”时自适应切回微信刷新 UIA。
  - `true` 不再表示“每轮都抢焦点”，而是按内部阈值按需触发。
  - 仍然会打扰当前工作窗口，只是比旧的“每轮切一次”克制得多。
- `worker_debug`：是否输出 worker 调试日志（例如 `debug target=... session_preview=... unread=...`）。

### `translate`
- `enabled`：是否启用翻译。`true` 调用翻译服务，`false` 原文透传。
- `provider`：翻译提供方。当前支持 `deeplx` / `passthrough`。
- `deeplx_url`：DeepLX 接口地址。
  - 当 `translate.enabled=true` 且 `provider=deeplx` 时，若配置值和 `DEEPLX_URL` 环境变量都为空，启动阶段会直接报错退出。
- `source_lang`：源语言，`auto` 表示自动检测。
- `target_lang`：目标语言，例如 `EN`。
- `timeout_seconds`：翻译请求超时时间（秒）。
  - 必须 `> 0`，否则启动阶段会直接报错退出。
- `deeplx_url` 建议放占位值，真实密钥通过 `.env.local`（已忽略）覆盖。
- DeepLX 请求遇到网络抖动时会做有限重试；HTTP 4xx/5xx 不重试。

### `display`
- `english_only`：`true` 时只显示翻译后的文本（替换原文展示）。
- `on_translate_fail`：翻译失败回退策略。
  - `show_cn_with_reason`：显示中文并附失败原因。
  - `show_cn`：只显示中文。
  - `show_reason`：只显示失败原因。
- `tts_auto_read_active_chat`：是否自动朗读“当前选中会话”的新英文消息，默认 `true`。
  - 这是启动默认值；侧边栏头部“朗读”开关可以在运行时临时开关。
  - 只在“原文”关闭时生效。
  - 不依赖侧边栏窗口焦点；只要功能开着、消息属于当前选中会话且正文可判定为英文，就会朗读。
  - 该开关只影响自动朗读，不影响“轻点正文”的手动触发。
- `width`：侧边栏宽度。
  - 当前默认 `470`。
  - 必须 `>= 280`，否则启动阶段会直接报错退出。
- `side`：侧边栏停靠位置，`left` 或 `right`。
- 置顶仅通过侧边栏头部“置顶”按钮手动切换，不再提供启动配置项。
- “原文”仅通过侧边栏头部开关手动切换，不提供启动配置项。
- “朗读”通过侧边栏头部开关手动切换，配置项 `tts_auto_read_active_chat` 仅决定启动默认值。

### `tts`
- `provider`：TTS 后端选择。当前支持 `windows_system` / `doubao`，默认 `doubao`。
  - `windows_system`：继续走本机 `System.Speech`，不依赖云接口。
  - `doubao`：走豆包单向流式 WebSocket，大模型合成结果回到本机播放。
- 若保持默认 `doubao`，启动前必须准备好 `config/doubao_tts.json` 对应凭证（推荐通过 `.env.local` 注入），否则启动阶段会直接报错退出。
- `config_path`：provider 私有配置文件路径。
  - 当前主要给 `doubao` 使用。
  - 相对路径优先按当前 `listener.json` 所在目录解析；找不到时再按项目根目录解析。
  - 推荐把 provider 私有配置拆到独立 JSON，别把不同供应商参数全堆回 `listener.json`。

### `logging`
- `file`：运行日志输出文件路径。相对路径按项目根目录解析（例如 `logs/sidebar_listener.log`）。
- 日志按大小自动轮转：默认单文件约 `10MB` 时切分，保留最近 `5` 个历史文件（`.1` ~ `.5`）。

## `config/doubao_tts.json` 示例
```json
{
  "provider": "doubao",
  "endpoint": "wss://openspeech.bytedance.com/api/v3/tts/unidirectional/stream",
  "appid_env": "VOLCENGINE_TTS_APPID",
  "access_token_env": "VOLCENGINE_TTS_ACCESS_TOKEN",
  "resource_id": "seed-tts-2.0",
  "speaker": "en_female_dacey_uranus_bigtts",
  "audio_format": "wav",
  "sample_rate": 32000,
  "speech_rate": -15,
  "loudness_rate": 0,
  "use_cache": false,
  "uid": "wechat-pc-auto",
  "connect_timeout_seconds": 10.0
}
```

### `doubao_tts.json` 字段说明
- `provider`：固定为 `doubao`，用于防止把错误 JSON 指给豆包后端。
- `endpoint`：单向流式 WebSocket 地址。通常保持默认值即可。
- `appid_env` / `access_token_env`：从环境变量读取凭证名。推荐把真实值放到 `.env.local`，不要硬写进仓库。
- `appid` / `access_token`：也支持直接写死，但不推荐；只有当对应 `_env` 未提供或环境变量为空时才会使用字面值。
- `resource_id`：豆包语音资源 ID，例如 `seed-tts-2.0`。
- `speaker`：音色 ID，必须和 `resource_id` 匹配。
- `audio_format`：当前必须是 `wav`。
  - 这不是拍脑袋限制；当前播放链路走 Windows 原生 `winsound`，它不适合直接播 `mp3`。
  - 如果你强行配 `mp3`，启动阶段会直接报错，而不是拖到运行时随机炸。
- `sample_rate`：采样率，当前默认 `32000`。
  - 当前只接受官方支持值：`8000 / 16000 / 22050 / 24000 / 32000 / 44100 / 48000`。
  - 默认提到 `32000`，是为了在“短句朗读更细一点”和“网络/播放成本别乱涨”之间取平衡；不是让你无脑拉到 `48000`。
- `speech_rate`：语速，当前默认 `-15`。
  - 允许范围：`-50 ~ 100`。
  - 默认明显慢一点，适合当前英文短句学习场景；继续下压会更像慢放，不是更自然。
- `loudness_rate`：音量倍率调节，当前默认 `0`。
  - 允许范围：`-50 ~ 100`。
  - 只有在某个音色明显偏小声时再加，不要把它当成“音质增强”开关。
- `use_cache`：是否启用豆包文本缓存，当前默认 `false`。
  - 开启后，相同文本可以直接命中服务端缓存，加快重复合成。
  - 默认关闭；聊天消息重复率没那么高，而且调音色/语速时缓存会污染对比结果。
  - 当前实现启用缓存时固定按“普通文本”发送 `cache_config.text_type=1`，不支持把 SSML 一起混进来。
- `uid`：业务侧用户标识，用于请求体。
- `connect_timeout_seconds`：建连超时秒数，必须 `> 0`。

## 当前消息渲染规则（代码行为）
- 明显的媒体占位文本会被过滤，不显示到侧边栏，也不会送进 DeepLX：`[图片]` / `[image]` / `[images]` / `[photo]` / `[视频]` / `[video]` / `[动画表情]` / `[animated emoticon]` / `[语音] 2"` / `[Voice Over] 3"` 等同类方括号占位文本。
- 额外兜底规则：只要整条消息被 ASCII 方括号完整包住（例如 `[系统提示]`），也会直接过滤。这条规则是故意偏激进的，会一并吞掉合法的方括号文本。
- 带显式 `http://` 或 `https://` 链接的消息也会直接过滤，不显示到侧边栏，也不会送进 DeepLX 或 TTS。
- 若消息是 `发送人: 正文` 格式，仅翻译“正文”，发送人姓名保持原样。
- 若消息不含发送人前缀，视为“自己消息”，在侧边栏右对齐显示。
- 启用 `deeplx` 时，右侧会先插入一条 `Loading...` 占位；翻译返回后原位替换成英文结果，不再先展示中文。
- 侧边栏头部“原文”开关打开后，右侧消息区会优先显示消息原文；关闭后恢复显示译文。
- 关闭“原文”且消息正文可判定为英文时，消息正文支持“轻点朗读”。
- 正文点击只绑定在正文字符范围，不包括时间、发送人和空白区。
- 若按下后位移很小再松开，会判定为“轻点播放”；若位移超过阈值、形成文本选区，或触发双击/三击选词，则不会播放。
- 当 `tts.provider=windows_system` 时，默认优先选用 `Microsoft Zira Desktop`，不存在时回退到其他英文 voice。
- 当 `tts.provider=doubao` 时，会先请求豆包单向流式 WebSocket，再把返回的 `wav` 在本机顺序播放。
- 豆包请求当前会固定带上 `audio_params.sample_rate` / `speech_rate` / `loudness_rate`；启用 `use_cache=true` 时，还会附带 `cache_config.use_cache=true`。
- 当 `display.tts_auto_read_active_chat=true` 时，当前选中会话的新英文消息在翻译结果落地后会自动朗读；切到其他会话后，旧会话新消息不会补读。
- UI 不显示 `source=session_preview`，该字段仅用于内部日志。
- 当前侧边栏仅用于监听与展示，不提供消息发送输入框。
- 多目标模式下只保留一个窗口：左侧 target 菜单 + 右侧消息区；未选中目标的新消息会累计未读计数。
- 每个 target 的消息缓存上限固定为 `100` 条（超出后丢弃最旧消息）。
- 翻译在后台线程执行，避免网络抖动时卡住侧边栏 UI。
- 翻译队列是有界队列（默认上限 `300`）；队列满时会丢弃最旧待翻译任务并记录 `translate queue overflow` 日志。

## 多目标运行约束
- 当前主链路固定为 `session-only`，禁止恢复 `chat` / `mixed` 配置。
- 多目标监听不会再派生多个 worker；所有 targets 由同一个 worker 在一次 UIA 扫描中完成。
- 运行时增删 target 也不会派生第二个 worker；当前实现是“回写 `listener.json` + 平滑重启单 worker”，不是 IPC 热更新。
