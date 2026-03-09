# WeChat PC Auto — Rust 原生版

> 基于 **Tauri 2 + React 19 + Rust** 构建的 macOS 微信桌面端自动化工具，  
> 通过 macOS Accessibility API 与 AppleScript 直接驱动微信，无 Python 依赖。

## 技术栈

| 层级 | 技术 |
|------|------|
| **桌面框架** | Tauri 2 (原生窗口 + 系统托盘 + IPC) |
| **前端** | React 19 + TypeScript + Vite 7 |
| **样式** | Tailwind CSS v4 + shadcn/ui |
| **状态管理** | Zustand |
| **后端** | Rust (tokio async) |
| **数据库** | SQLite (rusqlite) |
| **翻译** | DeepLX HTTP |
| **macOS 自动化** | accessibility-sys + AppleScript |

## 功能概览

- **消息监听** — 实时轮询微信消息，基于锚点+差集算法精准识别新消息
- **实时浮窗** — 紧贴微信窗口的浮动翻译窗口，支持原文/译文/双语模式
- **DeepLX 翻译** — 消息自动翻译，支持翻译缓存
- **消息历史** — SQLite 存储，支持搜索、分页、会话列表
- **系统托盘** — 快捷开关浮窗/监听/翻译

## 快速开始

### 前置条件

- macOS 13+
- [Rust](https://rustup.rs/) (stable) + [Node.js](https://nodejs.org/) 18+ & [pnpm](https://pnpm.io/)
- 微信 macOS 客户端已安装并登录
- **系统设置 → 隐私与安全 → 辅助功能** 中授权终端/IDE

### 开发模式

```bash
pnpm install
pnpm tauri dev
```

### 构建发布

```bash
pnpm tauri build
```

## 架构设计

### 整体架构

```
┌─────────────────────────────────────────────────────┐
│                 前端 (React + Vite)                  │
│  ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌───────────┐  │
│  │ 设置页面 │ │消息历史 │ │发送中心 │ │ 日志/事件 │  │
│  └─────────┘ └─────────┘ └─────────┘ └───────────┘  │
│  ┌─────────────────────────────────────────────────┐│
│  │          实时浮窗 (SidebarView)                  ││
│  └─────────────────────────────────────────────────┘│
│  Stores: eventStore · sidebarStore · formStore      │
└────────────────────────┬────────────────────────────┘
                         │ Tauri IPC
                         ▼
┌─────────────────────────────────────────────────────┐
│                  Tauri 后端 (Rust)                   │
│  ┌───────────────────────────────────────────────┐  │
│  │ TaskManager (消息差集 · 翻译 · 入库 · 事件发布) │  │
│  └───────────────────────────────────────────────┘  │
│  EventStore · MessageDb · SidebarRuntime · Config   │
└────────────────────────┬────────────────────────────┘
                         │ AX API / AppleScript
                         ▼
                   微信 macOS 客户端
```

### 消息监听流程

```
启动监听 → TaskManager::monitor_loop (tokio::spawn)
    │
    ├─ 1. AX API 读取当前会话名 (read_active_chat_name)
    ├─ 2. AX API 读取消息列表 (read_chat_messages_rich)
    ├─ 3. 消息差集算法 (锚点法 → 尾部追加 → Bag Diff)
    │
    └─ 新消息处理
        ├─ MessageDb 入库 (SHA256 去重)
        ├─ EventStore 广播事件
        └─ Sidebar? → 翻译推送 (DeepLX)
```

## 悬浮窗数据流

### 核心数据流

```
微信客户端          Rust 后端                    前端 (React)
    │                   │                           │
    │  AX API 轮询      │                           │
    │◀──────────────────│                           │
    │  返回消息         │                           │
    │──────────────────▶│                           │
    │                   │                           │
    │                   │ 消息入库 (SQLite)          │
    │                   │ 更新 SidebarRuntime       │
    │                   │   current_chat            │
    │                   │   refresh_version++       │
    │                   │                           │
    │                   │ 发布 sidebar-refresh ────▶│ requestRefresh()
    │                   │                           │ → refreshVersion++
    │                   │                           │
    │                   │◀───────────────────────────│ fetchSnapshot()
    │                   │                           │
    │                   │ 返回快照 ────────────────▶│ hydrateSnapshot()
    │                   │ { current_chat,           │ → items 替换
    │                   │   messages,               │ → UI 渲染
    │                   │   refresh_version }       │
    │                   │                           │
    │                   │ 异步翻译 (spawn)           │
    │                   │ db.update_translation()   │
    │                   │                           │
    │                   │ 发布 sidebar-refresh ────▶│ 再次拉取快照
    │                   │                           │ → 译文显示 ✓
```

### 消息入库路径

消息入库有两个代码路径，**均会触发翻译和悬浮窗刷新**：

| 路径 | 条件 | 数据质量 |
|------|------|---------|
| **详细消息** (chat) | `use_right_panel_details=true` | 高 (完整内容) |
| **会话预览** (session_preview) | `use_right_panel_details=false` | 低 (预览片段) |

### 核心状态

**后端 (Rust)**：

```rust
pub struct SidebarRuntime {
    current_chat: Mutex<String>,   // 当前聊天（后端真相源）
    refresh_version: AtomicU64,    // 刷新版本号
}
```

**前端 (TypeScript)**：

```typescript
interface SidebarStoreState {
  items: SidebarMessage[];      // 消息列表
  currentChat: string;          // 当前聊天
  refreshVersion: number;       // 触发 fetchSnapshot
  remoteRefreshVersion: number; // 后端版本
}
```

### 事件机制

| 事件 | 触发时机 | 前端处理 |
|------|---------|---------|
| `chat_switched` | 聊天切换后 | `setCurrentChat()` → 标题更新 |
| `sidebar-refresh` | 聊天切换 / 消息入库 / 译文写回 | `requestRefresh()` → 拉取快照 |

**聊天切换流程**（用户在微信中点击不同聊天）：
1. AX API 检测到 `chat_name` 变化
2. 防抖阈值（连续 2 次相同）确认切换
3. 发布 `chat_switched` → 前端标题立即更新
4. 发布 `sidebar-refresh` → 前端拉取新聊天历史消息

### 翻译流程

```
消息入库 → spawn 异步翻译
    │
    ├─ 检查翻译缓存 (message_translations)
    │   ├─ 命中 → 写回 messages.content_en
    │   └─ 未命中 → DeepLX API → 写入缓存 + 写回
    │
    └─ 发布 sidebar-refresh → 前端拉取快照 → 译文显示
```

### 关键设计

| 设计 | 原因 |
|------|------|
| **后端维护 current_chat** | 避免前端拼状态不一致 |
| **提交后刷新** | 确保前端拉取时数据已落盘 |
| **快照拉取** | 简化逻辑，避免消息乱序/丢失 |
| **防空列表闪断** | 空列表时保留旧内容 |

## 项目结构

```
rust/
├── src/                    # 前端 (React + TypeScript)
│   ├── components/         # 页面组件
│   ├── stores/             # Zustand stores
│   └── lib/                # API + 类型
│
└── src-tauri/              # Rust 后端
    └── src/
        ├── task_manager.rs # 核心任务调度
        ├── db.rs           # SQLite 持久化
        ├── translator.rs   # DeepLX 客户端
        ├── adapter/        # macOS 适配层
        └── commands/       # Tauri 命令
```

## 数据存储

| 数据 | 路径 |
|------|------|
| 消息数据库 | `~/Library/Application Support/com.wang.wechat-pc-auto/messages.db` |
| 配置文件 | `~/Library/Application Support/com.wang.wechat-pc-auto/config/listener.json` |

## 调试命令

```bash
# AX API 测试
cargo run --bin ax-test

# AX 控件树导出
cargo run --bin ax-dump
```

## 许可证

MIT
