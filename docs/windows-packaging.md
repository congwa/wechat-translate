# Windows 打包说明

这条 `session-only` 分支支持直接打包成 Windows 应用，但不是“一个 exe 包打天下”的花活。

正确产物是：
- 一个主程序：`wechat_sidebar.exe`
- 一个同目录 worker：`group_listener_worker.exe`

主程序负责侧边栏 UI、翻译、单 worker 管理。  
worker 负责实际监听并通过标准输出回传 JSON 事件。  
把它们拆成两个 exe，不是重复造轮子，而是为了避免把 GUI 程序硬拿去当 stdout worker，最后把事件流彻底搞死。

## 前提

- Windows 11
- Python 3.11
- 已安装依赖：`pip install -r requirements.txt`
- 已安装 PyInstaller：`python -m pip install pyinstaller`

## 构建

在仓库根目录执行：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\build_windows_exe.ps1
```

如果你想要主程序保留控制台窗口，便于调试日志：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\build_windows_exe.ps1 -Console
```

## 产物位置

默认输出目录：

```text
artifacts\windows-app\wechat_sidebar\
```

目录里至少会有：

```text
wechat_sidebar.exe
group_listener_worker.exe
_internal\
```

构建成功后，脚本会自动清理 `build\pyinstaller\` 中间目录和对应日志；如果顶层 `build\` 目录因此变空，也会一并删除，只保留 `artifacts\windows-app\wechat_sidebar\` 最终产物。

## 运行约束

- 只启动 `wechat_sidebar.exe`
- `group_listener_worker.exe` 必须和主程序放在同一目录
- 第一次启动时，如果主程序目录下还没有 `config\listener.json`，程序会从打包内置模板自动拷一份出来
- `.env.local` 也按主程序目录解析；如果你要给 DeepLX 配地址，就把 `.env.local` 放在 `wechat_sidebar.exe` 同目录
- 日志、运行时锁、默认配置都写在主程序目录，不写回源码目录

## 最小验证

构建完成后，先确认这两个文件都在：

```powershell
Get-ChildItem .\artifacts\windows-app\wechat_sidebar\
```

再做一个最小 worker 验证：

```powershell
.\artifacts\windows-app\wechat_sidebar\group_listener_worker.exe --help
```

然后启动主程序：

```powershell
.\artifacts\windows-app\wechat_sidebar\wechat_sidebar.exe
```

## 常见问题

### 1) 主程序能启动，但立刻提示 worker 失败

先看 `group_listener_worker.exe` 是否还在主程序目录。  
没有它，主程序没法监听。

### 2) 打包后找不到配置

正常情况下，第一次启动会自动生成：

```text
config\listener.json
```

如果没有生成，优先看主程序目录是否有写权限。

### 3) 打包后 DeepLX 不生效

主程序不再读源码目录下的 `.env.local`。  
要把 `.env.local` 放到 `wechat_sidebar.exe` 同目录。

### 4) 双击 `wechat_sidebar.exe` 没反应

这通常不是 exe 没启动，而是启动阶段就自己退了。  
当前最常见原因是：

- `config\listener.json` 里 `translate.enabled=true`
- `provider=deeplx`
- 但打包目录下没有 `.env.local`
- 同时 `translate.deeplx_url` 也没直接写进配置

这种情况下，控制台版会直接报：

```text
[sidebar] invalid config: translate.enabled=true and provider=deeplx require translate.deeplx_url or DEEPLX_URL
```

修法只有两个：

- 把 `.env.local` 放到 `wechat_sidebar.exe` 同目录
- 或者直接把 `translate.deeplx_url` 写进 `config\listener.json`

不要把“点了没反应”误判成 DLL 警告导致。  
这次场景里，真正的直接原因是 DeepLX 配置缺失，不是 `UIAutomationClient_VC140_*.dll` 构建告警。
