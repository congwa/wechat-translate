# AGENTS

执行本仓库与“微信监听/侧边栏/翻译”相关任务前，先阅读：
- `docs/wechat-listening-pitfalls.md`

## 路径职责总览

### 根目录
- `.git/`：Git 元数据目录。
- `.env.local`：本地环境变量（例如 `DEEPLX_URL`），仅本机使用，已在 `.gitignore` 中忽略。
- `config/listener.json`：监听/翻译/展示主配置（目标会话、翻译策略、侧边栏参数）。
- `.gitignore`：Git 忽略规则。
- `AGENTS.md`：仓库工作约束与路径职责说明（本文件）。
- `LICENSE`：MIT 许可证。
- `README.md`：项目使用文档与示例命令。
- `pyproject.toml`：Python 包构建与依赖声明。
- `requirements.txt`：运行依赖列表。
- `@AutomationLog.txt`：本地调试日志文件（运行期产物）。

### 核心代码 `wechat_auto/`
- `wechat_auto/__init__.py`：包导出入口（`WxAuto`）。
- `wechat_auto/core.py`：`WxAuto` 主类，仅保留微信窗口加载与只读会话查询能力。
- `wechat_auto/window.py`：微信主窗口定位与激活逻辑（含 WeChat/Weixin 主窗口筛选）。
- `wechat_auto/controls.py`：UIA 控件定位与通用文本判定工具（会话列表、消息列表、搜索框）。
- `wechat_auto/logger.py`：统一日志输出函数。

### 当前主流程脚本 `listener_app/`
- `listener_app/group_listener_worker.py`：监听 worker 进程；输出 JSON 行事件；单进程多 target 扫描会话预览。
- `listener_app/sidebar_translate_listener.py`：侧边栏 UI + DeepLX 翻译 + 单 worker 管理。

### 文档与运行产物
- `docs/wechat-listening-pitfalls.md`：监听架构、坑位、排障与实现契约。
- `logs/`：运行日志目录（例如侧边栏日志）。

## 文件级路径清单（当前仓库）
- `LICENSE`：MIT 许可证文本。
- `README.md`：项目说明、安装、示例命令。
- `AGENTS.md`：仓库工作约束与路径职责。
- `pyproject.toml`：构建系统与项目元数据。
- `requirements.txt`：依赖列表。
- `.gitignore`：忽略规则。
- `.env.local`：本地环境变量。
- `config/listener.json`：监听与翻译主配置。
- `config/listener.md`：监听配置字段说明。
- `@AutomationLog.txt`：本地调试日志。
- `docs/wechat-listening-pitfalls.md`：监听与翻译链路踩坑文档。
- `listener_app/sidebar_translate_listener.py`：侧边栏 UI、翻译、worker 管理。
- `listener_app/group_listener_worker.py`：监听 worker，输出 JSON 事件。
- `wechat_auto/__init__.py`：包导出入口。
- `wechat_auto/core.py`：`WxAuto` 主类。
- `wechat_auto/window.py`：窗口定位与激活。
- `wechat_auto/controls.py`：UIA 控件查找与文本判定。
- `wechat_auto/logger.py`：日志输出。

## 维护约束
- 监听主链路默认只维护 `listener_app/sidebar_translate_listener.py` + `listener_app/group_listener_worker.py`。
- 当前分支不再维护发送消息、发送文件、自动回复、写输入框等主动操作能力。
- 任何监听相关改动都要同步更新 `docs/wechat-listening-pitfalls.md`。
- 任何 `config/listener.json` 字段新增/删除/语义变更，必须同步更新 `config/listener.md` 对应说明与示例。
