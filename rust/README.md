# WeChat PC Auto — Rust 原生版

> 基于 **Tauri 2 + React 19 + Rust** 构建的 macOS 微信桌面端自动化工具，  
> 完全移除 Python 依赖，通过 macOS Accessibility API 与 AppleScript 直接驱动微信。

## 技术栈

| 层级 | 技术 | 说明 |
|------|------|------|
| **桌面框架** | Tauri 2 | 原生窗口 + 系统托盘 + IPC |
| **前端** | React 19 + TypeScript | Vite 7 热更新 |
| **样式** | Tailwind CSS v4 + Radix UI | shadcn/ui 风格组件 |
| **状态管理** | Zustand | 轻量响应式 Store |
| **动画** | Framer Motion | 页面切换 + 交互动效 |
| **后端** | Rust (tokio async) | 高性能异步任务管理 |
| **数据库** | rusqlite (SQLite) | 本地消息持久化 + 去重 |
| **翻译** | DeepLX HTTP Client | 实时翻译 |
| **macOS 自动化** | accessibility-sys + AppleScript | 原生 AX API 读取 + 脚本操作 |

## 功能概览

- **消息监听** — 实时轮询微信消息，基于锚点+差集算法精准识别新消息
- **实时浮窗（Sidebar）** — 紧贴微信窗口的浮动翻译窗口，跟随移动，微信最小化自动隐藏
- **DeepLX 翻译** — 侧边栏消息自动翻译，支持多语言
- **自动回复** — 规则匹配自动回复（关键词触发）
- **消息发送** — 文本/文件发送，通过剪贴板 + AppleScript 实现
- **消息历史** — SQLite 存储，支持搜索、分页、会话列表
- **系统托盘** — 快捷开关浮窗/监听 + 关闭时最小化到托盘
- **环境预检** — 启动时自动检测微信进程、辅助功能权限、窗口状态

## 架构设计

### 整体架构

```
┌──────────────────────────────────────────────────────────┐
│                    前端 (React + Vite)                    │
│                                                          │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌────────────┐  │
│  │ 设置页面 │ │ 消息历史 │ │ 发送中心 │ │  日志/事件  │  │
│  └──────────┘ └──────────┘ └──────────┘ └────────────┘  │
│  ┌───────────────────────────────────────────────────┐   │
│  │              实时浮窗 (SidebarView)                │   │
│  │          独立窗口，?view=sidebar 路由              │   │
│  └───────────────────────────────────────────────────┘   │
│                                                          │
│  Stores: eventStore · sidebarStore · formStore           │
│  API 层: tauri-api.ts → invoke()                         │
└────────────────────────┬─────────────────────────────────┘
                         │ Tauri IPC (invoke / listen)
                         ▼
┌──────────────────────────────────────────────────────────┐
│                   Tauri 后端 (Rust)                       │
│                                                          │
│  ┌─────────────────────────────────────────────────────┐ │
│  │                   TaskManager                       │ │
│  │  核心调度循环 · 消息差集 · 翻译 · 自动回复 · 入库  │ │
│  └─────────────────────────────────────────────────────┘ │
│                                                          │
│  ┌────────────┐ ┌────────────┐ ┌──────────────────────┐ │
│  │ EventStore │ │ MessageDb  │ │ SidebarWindowState   │ │
│  │  事件广播  │ │  SQLite    │ │  浮窗位置/生命周期   │ │
│  └────────────┘ └────────────┘ └──────────────────────┘ │
│  ┌────────────┐ ┌────────────┐ ┌──────────────────────┐ │
│  │ Translator │ │  Config    │ │     Commands         │ │
│  │  DeepLX    │ │  读写配置  │ │  22 个 Tauri 命令    │ │
│  └────────────┘ └────────────┘ └──────────────────────┘ │
└────────────────────────┬─────────────────────────────────┘
                         │ accessibility-sys / AppleScript
                         ▼
┌──────────────────────────────────────────────────────────┐
│                  MacOSAdapter 适配层                      │
│                                                          │
│  ax_reader (AX API)              applescript (osascript)  │
│  ├ read_active_chat_name         ├ activate_wechat        │
│  ├ read_chat_messages_rich       ├ open_chat_by_search    │
│  ├ get_sessions                  ├ copy_text / paste_send │
│  └ has_popup_or_menu             └ query_wechat_window    │
└────────────────────────┬─────────────────────────────────┘
                         │
                         ▼
                   微信 macOS 客户端
```

### 消息监听流程

```
┌─────────┐     ┌──────────────┐     ┌───────────────────────────────┐
│  启动   │────▶│  开启监听    │────▶│  TaskManager::monitor_loop    │
│         │     │ interval=1s  │     │  tokio::spawn 异步循环         │
└─────────┘     └──────────────┘     └───────────────┬───────────────┘
                                                     │
                                                     ▼
                                      ┌──────────────────────────┐
                                      │ 1. AX API 读取当前会话名 │
                                      │    read_active_chat_name │
                                      └──────────────┬───────────┘
                                                     │
                                                     ▼
                                      ┌──────────────────────────┐
                                      │ 2. AX API 读取消息列表   │
                                      │  read_chat_messages_rich │
                                      └──────────────┬───────────┘
                                                     │
                                                     ▼
                                      ┌──────────────────────────┐
                                      │ 3. 消息差集算法          │
                                      │  ┌─ 锚点法 (3→2→1)      │
                                      │  │  用已知末尾消息定位   │
                                      │  ├─ 尾部追加             │
                                      │  │  消息数增加 → 取新增  │
                                      │  └─ Bag Diff 回退        │
                                      │     内容哈希全集比较     │
                                      └──────────────┬───────────┘
                                                     │
                                        ┌────────────┼────────────┐
                                        ▼            ▼            ▼
                                ┌────────────┐ ┌──────────┐ ┌──────────┐
                                │ EventStore │ │ MessageDb│ │ Sidebar? │
                                │ 广播事件   │ │ 消息入库 │ │ 翻译推送 │
                                │ → 前端     │ │ SHA256   │ │ DeepLX   │
                                └────────────┘ │ 去重     │ └──────────┘
                                               └──────────┘
```

### 实时浮窗（Sidebar）流程

```
用户点击「开启实时浮窗」
         │
         ▼
┌────────────────────────┐     ┌─────────────────────────┐
│ 1. 确保监听已启动      │────▶│ 2. enable_sidebar()     │
│    start_monitoring()  │     │    设置翻译器 + 目标群  │
└────────────────────────┘     └───────────┬─────────────┘
                                           │
                                           ▼
                               ┌─────────────────────────┐
                               │ 3. 打开 Sidebar 窗口    │
                               │    WebviewWindow 新建   │
                               │    URL: ?view=sidebar   │
                               │    always_on_top=true    │
                               └───────────┬─────────────┘
                                           │
                                           ▼
                               ┌─────────────────────────┐
                               │ 4. 位置跟踪循环         │
                               │    每 500ms 查询微信窗口 │
                               │    浮窗紧贴微信右侧     │
                               │    微信最小化 → 隐藏     │
                               │    微信恢复   → 显示     │
                               └───────────┬─────────────┘
                                           │
                        ┌──────────────────┼──────────────────┐
                        ▼                  ▼                  ▼
               ┌──────────────┐ ┌───────────────┐ ┌───────────────┐
               │ 新消息到达   │ │  翻译消息     │ │  渲染到浮窗   │
               │ → Sidebar    │ │  DeepLX API   │ │  SidebarView  │
               │   事件过滤   │ │  source_lang  │ │  前端组件     │
               └──────────────┘ │  target_lang  │ └───────────────┘
                                └───────────────┘
```

### 前端页面结构

```
App.tsx
 ├─ ?view=sidebar → SidebarView（独立浮窗渲染）
 └─ MainApp
     ├─ 侧边导航栏
     │   ├─ 品牌标识 (WeChat Auto)
     │   ├─ 实时浮窗 开/关 按钮
     │   └─ 页面导航: 设置 · 历史 · 发送 · 日志
     ├─ 顶栏状态: 监听/自动回复/浮窗 运行指示灯
     ├─ PreflightBar: 环境预检状态
     └─ 页面内容
         ├─ SettingsPage  — 监听配置 · 翻译参数 · 自动回复
         ├─ MessageHistory — 会话列表 · 消息搜索 · 分页浏览
         ├─ SendCenter     — 文本/文件发送 · 会话选择
         └─ LogsPage       — EventStream + ServiceLogs
```

## 项目结构

```
rust/
├── src/                            # 前端源码 (React + TypeScript)
│   ├── App.tsx                     # 根组件 (路由分发)
│   ├── main.tsx                    # 渲染入口
│   ├── components/                 # 业务组件
│   │   ├── SettingsPage.tsx        #   设置页面
│   │   ├── MessageHistory.tsx      #   消息历史
│   │   ├── SendCenter.tsx          #   发送中心
│   │   ├── SidebarView.tsx         #   实时浮窗 UI
│   │   ├── PreflightBar.tsx        #   环境预检
│   │   ├── EventStream.tsx         #   事件流
│   │   ├── ServiceLogs.tsx         #   服务日志
│   │   ├── layout/                 #   布局组件
│   │   └── ui/                     #   shadcn/ui 基础组件
│   ├── stores/                     # Zustand 状态管理
│   │   ├── eventStore.ts           #   事件 + 任务状态
│   │   ├── sidebarStore.ts         #   浮窗消息
│   │   ├── formStore.ts            #   表单设置 (持久化)
│   │   ├── preflightStore.ts       #   预检状态
│   │   └── toastStore.ts           #   Toast 通知
│   ├── lib/                        # 工具层
│   │   ├── tauri-api.ts            #   Tauri invoke 封装
│   │   ├── types.ts                #   共享类型定义
│   │   ├── error-messages.ts       #   错误信息映射
│   │   └── utils.ts                #   通用工具
│   └── styles/globals.css          # 全局样式
│
├── src-tauri/                      # Rust 后端
│   ├── src/
│   │   ├── main.rs                 # 入口
│   │   ├── lib.rs                  # Tauri 应用初始化 + 托盘菜单
│   │   ├── task_manager.rs         # 核心任务管理器 (监听/翻译/回复)
│   │   ├── events.rs               # 事件发布 + 历史存储
│   │   ├── db.rs                   # SQLite 消息持久化
│   │   ├── config.rs               # 配置文件读写
│   │   ├── translator.rs           # DeepLX 翻译客户端
│   │   ├── sidebar_window.rs       # 浮窗窗口管理
│   │   ├── adapter/                # macOS 适配层
│   │   │   ├── mod.rs              #   MacOSAdapter 门面
│   │   │   ├── ax_reader.rs        #   Accessibility API 消息读取
│   │   │   └── applescript.rs      #   AppleScript 操作
│   │   ├── commands/               # Tauri 命令 (22 个)
│   │   │   ├── send.rs             #   发送文本/文件
│   │   │   ├── listen.rs           #   监听/自动回复控制
│   │   │   ├── sidebar.rs          #   浮窗管理
│   │   │   ├── config.rs           #   配置读写
│   │   │   ├── db.rs               #   消息查询
│   │   │   ├── sessions.rs         #   会话列表
│   │   │   ├── preflight.rs        #   环境预检
│   │   │   └── tray.rs             #   托盘设置
│   │   └── bin/                    # 调试 CLI 工具
│   │       ├── ax_test.rs          #   AX API 功能测试
│   │       ├── ax_dump.rs          #   AX 树导出
│   │       └── ax_deep.rs          #   AX 深层节点导出
│   ├── Cargo.toml                  # Rust 依赖声明
│   └── tauri.conf.json             # Tauri 配置
│
├── package.json                    # 前端依赖
├── vite.config.ts                  # Vite 构建配置
├── tsconfig.json                   # TypeScript 配置
└── index.html                      # HTML 入口
```

## 快速开始

### 前置条件

- macOS 13+
- [Rust](https://rustup.rs/) (stable)
- [Node.js](https://nodejs.org/) 18+ & [pnpm](https://pnpm.io/)
- 微信 macOS 客户端已安装并登录
- **系统偏好设置 → 隐私与安全 → 辅助功能** 中授权终端/IDE

### 开发模式

```bash
# 安装前端依赖
pnpm install

# 启动 Tauri 开发模式 (前端热更新 + Rust 编译)
pnpm tauri dev
```

### 构建发布

```bash
# 构建生产版本 (DMG + .app)
pnpm tauri build
```

### 调试工具

```bash
# 测试 AX API 读取能力
cargo run --bin ax-test

# 导出微信 AX 控件树
cargo run --bin ax-dump

# 深层控件树导出
cargo run --bin ax-deep
```

## 数据存储

| 数据 | 路径 | 说明 |
|------|------|------|
| 消息数据库 | `~/Library/Application Support/com.wang.wechat-pc-auto/messages.db` | SQLite，SHA256 去重 |
| 配置文件 | `~/Library/Application Support/com.wang.wechat-pc-auto/config/listener.json` | 监听/翻译/展示配置 |

## 与 Electron 版的对比

| 维度 | Electron + Python 版 | Tauri + Rust 版 |
|------|---------------------|-----------------|
| 安装包体积 | ~200MB+ (含 Python 运行时) | ~8MB (原生二进制) |
| 内存占用 | ~300MB+ | ~30MB |
| 启动时间 | 3-5s (Python 进程启动) | <1s |
| 进程模型 | Electron + Python 双进程 | 单进程 (Tauri) |
| macOS 自动化 | Python atomacos + subprocess | Rust AX API 直调 |
| 依赖管理 | npm + pip 双生态 | pnpm + cargo |
| 类型安全 | TypeScript + Python (弱) | TypeScript + Rust (强) |

## 许可证

MIT
