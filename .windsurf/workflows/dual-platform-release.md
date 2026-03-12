---
description: Dual platform release workflow for GitHub and Gitee
---

# 双平台发布流程

本流程用于同时向 GitHub 和 Gitee 两个平台发布版本，确保代码同步和包发布的一致性。

## 前置条件

- 已配置 GitHub CLI (`gh`) 和 Gitee API Token
- 本地 Git 远程仓库已配置 `origin` (GitHub) 和 `gitee` (Gitee)
- 已构建好需要发布的二进制包文件

## 发布步骤

### 1. 代码提交和推送

```bash
# 添加所有更改
git add .

# 提交代码（使用语义化提交信息）
git commit -m "release: v{version}

- 更新功能描述和文档
- 修复已知问题
- 添加新特性"

# 推送到两个平台
git push origin {branch}
git push gitee {branch}
```

### 2. 创建和推送标签

```bash
# 删除已存在的标签（如果需要）
git tag -d v{version}

# 创建新标签
git tag v{version}

# 推送标签到两个平台
git push origin v{version}
git push gitee v{version}
```

### 3. GitHub 发布

```bash
# 创建 GitHub Release 并上传文件
gh release create v{version} \
  --title "WeChat PC Auto v{version}" \
  --notes "## WeChat PC Auto v{version}

### 新增功能
- 功能描述1
- 功能描述2

### 功能特性
- 特性1
- 特性2

### 注意事项
- 注意事项1
- 注意事项2

### 下载
- macOS: WeChat PC Auto_{version}_aarch64.dmg" \
  rust/src-tauri/target/release/bundle/dmg/WeChat\ PC\ Auto_{version}_aarch64.dmg
```

### 4. Gitee 发布

```bash
# 创建 Gitee Release
curl -X POST "https://gitee.com/api/v5/repos/{user}/{repo}/releases" \
  -H "Content-Type: application/json" \
  -d '{
    "access_token": "'$GITEE_TOKEN'",
    "tag_name": "v{version}",
    "name": "WeChat PC Auto v{version}",
    "target_commitish": "{branch}",
    "body": "## WeChat PC Auto v{version}

### 新增功能
- 功能描述1
- 功能描述2

### 功能特性
- 特性1
- 特性2

### 注意事项
- 注意事项1
- 注意事项2

### 下载
- macOS: WeChat PC Auto_{version}_aarch64.dmg"
  }'
```

### 5. 文件上传到 Gitee

注意：Gitee 的文件上传 API 可能限制较多，建议手动上传文件到 Release 页面。

访问：https://gitee.com/{user}/{repo}/releases/v{version}

手动上传 DMG 文件到该页面。

## 自动化脚本

创建 `scripts/dual-release.sh` 脚本来自动化流程：

```bash
#!/bin/bash

set -e

VERSION=$1
BRANCH=${2:-"main"}

if [ -z "$VERSION" ]; then
    echo "Usage: $0 <version> [branch]"
    exit 1
fi

echo "🚀 开始双平台发布 v$VERSION"

# 1. 提交和推送
echo "📦 提交代码..."
git add .
git commit -m "release: v$VERSION"
git push origin $BRANCH
git push gitee $BRANCH

# 2. 标签管理
echo "🏷️  创建标签..."
git tag -d v$VERSION 2>/dev/null || true
git tag v$VERSION
git push origin v$VERSION
git push gitee v$VERSION

# 3. GitHub 发布
echo "🐙 创建 GitHub Release..."
DMG_FILE="rust/src-tauri/target/release/bundle/dmg/WeChat PC Auto_${VERSION}_aarch64.dmg"
if [ -f "$DMG_FILE" ]; then
    gh release create v$VERSION \
        --title "WeChat PC Auto v$VERSION" \
        --notes "## WeChat PC Auto v$VERSION

### 新增功能
- 更新功能描述和文档
- 修复已知问题

### 功能特性
- 智能消息监听
- 双模式实时浮窗
- 多渠道翻译
- 查词与发音
- 单词本与复习

### 注意事项
- 每次版本更新后需要重新授权辅助功能权限
- 首次安装后需要完全重启应用才能正常使用" \
        "$DMG_FILE"
else
    echo "❌ DMG 文件不存在: $DMG_FILE"
    exit 1
fi

# 4. Gitee 发布
echo "🇨🇳 创建 Gitee Release..."
curl -X POST "https://gitee.com/api/v5/repos/cong_wa/wechat-translate/releases" \
  -H "Content-Type: application/json" \
  -d '{
    "access_token": "'$GITEE_TOKEN'",
    "tag_name": "v'$VERSION'",
    "name": "WeChat PC Auto v'$VERSION'",
    "target_commitish": "'$BRANCH'",
    "body": "## WeChat PC Auto v'$VERSION'

### 新增功能
- 更新功能描述和文档
- 修复已知问题

### 功能特性
- 智能消息监听
- 双模式实时浮窗
- 多渠道翻译
- 查词与发音
- 单词本与复习

### 注意事项
- 每次版本更新后需要重新授权辅助功能权限
- 首次安装后需要完全重启应用才能正常使用

### 下载
- macOS: WeChat PC Auto_'$VERSION'_aarch64.dmg (支持 Apple Silicon Mac)"
  }'

echo "✅ 发布完成！"
echo "🔗 GitHub: https://github.com/congwa/wechat-translate/releases/tag/v$VERSION"
echo "🔗 Gitee: https://gitee.com/cong_wa/wechat-translate/releases/v$VERSION"
echo "⚠️  请手动上传 DMG 文件到 Gitee Release 页面"
```

## 使用方法

```bash
# 使用脚本发布
chmod +x scripts/dual-release.sh
./scripts/dual-release.sh 0.1.4 codex/session-single-worker
```

## 注意事项

1. **环境变量**：确保 `GITEE_TOKEN` 环境变量已设置
2. **权限**：确保对两个仓库都有写入权限
3. **文件检查**：发布前确认 DMG 文件已构建完成
4. **手动操作**：Gitee 文件上传可能需要手动完成
5. **分支管理**：确保推送到正确的分支

## 故障排除

### Gitee API 限制
- 文件上传可能失败，建议手动上传
- API 调用频率限制，适当重试

### GitHub CLI 问题
- 确保已安装 `gh` 命令
- 确保已登录 GitHub 账户

### 标签冲突
- 如果标签已存在，先删除再创建
- 确保两个平台的标签保持同步

## 最佳实践

1. **发布前检查**：运行完整测试套件
2. **版本管理**：使用语义化版本号
3. **文档更新**：同步更新 README 和 CHANGELOG
4. **备份策略**：保留重要版本的源码和构建产物
5. **发布通知**：在相关社区通知新版本发布
