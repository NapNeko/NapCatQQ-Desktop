# AI 更新日志生成指南（本地使用）

## 简介

此脚本仅在**本地使用**，通过环境变量配置 API Key，配置文件不会提交到 Git 仓库。

## 前置要求

```bash
# 安装依赖
pip install httpx
```

## 配置（本地环境变量）

```bash
# 必需：API Key
export OPENAI_API_KEY="sk-..."
# 或
export OPENROUTER_API_KEY="sk-or-v1-..."

# 可选：自定义 API 地址（默认使用 OpenRouter）
export OPENAI_API_URL="https://openrouter.ai/api/v1/chat/completions"

# 可选：自定义模型（默认使用免费模型）
export OPENAI_MODEL="z-ai/glm-4.5-air:free"
```

### API 提供商推荐

| 提供商 | URL | 免费模型 |
|--------|-----|----------|
| OpenRouter | `https://openrouter.ai/api/v1/chat/completions` | `z-ai/glm-4.5-air:free` |
| SiliconFlow | `https://api.siliconflow.cn/v1/chat/completions` | 需要 API Key |
| OpenAI | `https://api.openai.com/v1/chat/completions` | 需要付费 |

**注意**：这些配置只在你的本地终端生效，不会被提交到 Git 仓库。

## 使用方法

### 基本用法

```bash
# 自动生成当前 tag 与上一个 tag 之间的更新日志
python script/utils/generate_changelog_ai.py v1.7.28

# 指定对比版本
python script/utils/generate_changelog_ai.py v1.7.28 v1.7.27
```

### 输出示例

```
🚀 生成更新日志: v1.7.28
📌 自动检测到上一版本: v1.7.27
📝 收集 commit 记录...
   找到 12 个 commit
📊 获取文件变化统计...
🎯 生成更新日志...
🤖 正在调用 AI API (z-ai/glm-4.5-air:free)...

============================================================
生成的更新日志：
============================================================
# v1.7.28

## 🐛 修复
1. 修复 MSI 更新脚本参数传递问题 (abc1234)
2. 修复下载器异常捕获过于宽泛问题 (def5678)

## ✨ 新增
1. 新增临时目录回退机制 (ghi9012)
2. 支持多变量注入到 Batch 脚本 (jkl3456)

## 🔧 优化
1. 优化安装类型检测的路径比较逻辑 (mno7890)
============================================================

✅ 已保存到: CHANGELOG_v1.7.28.md
是否更新 docs/CHANGELOG.md? (y/n):
```

## 工作原理

1. **获取 Commit**：通过 `git log` 获取两个 Tag 之间的所有 Commit
2. **文件统计**：获取变更文件列表和统计信息
3. **AI 生成**：将 Commit 列表和文件变化发送给 AI
4. **格式化**：AI 根据系统 Prompt 生成格式化的更新日志
5. **后处理**：替换版本号占位符，保存到文件

## 与 update_version.py 的对比

| 功能 | update_version.py | generate_changelog_ai.py |
|------|-------------------|--------------------------|
| 版本号更新 | ✅ | ❌ |
| 自动生成日志 | ✅（基于规则） | ✅（基于 AI） |
| 智能分类 | ❌（简单正则） | ✅（语义理解） |
| 合并相似项 | ❌ | ✅ |
| 生成质量 | 一般 | 更好 |
| 配置方式 | 无需配置 | 本地环境变量 |

## 建议的工作流程

```bash
# 1. 确保已配置环境变量
export OPENAI_API_KEY="sk-or-v1-..."

# 2. 手动更新版本号到 pyproject.toml
# 3. 提交并打 tag
git add -A
git commit -m "chore: bump version to v1.7.28"
git tag v1.7.28

# 4. 生成本地更新日志
python script/utils/generate_changelog_ai.py v1.7.28

# 5. 按提示更新 docs/CHANGELOG.md

# 6. 推送
git push origin main --tags
```

## 故障排除

### API 调用失败

```
⚠️ AI 调用失败: [错误信息]
使用默认模板生成...
```

- 检查环境变量是否设置：`echo $OPENAI_API_KEY`
- 检查 API Key 是否正确
- 检查网络连接

### 没有生成任何内容

- 确保两个 Tag 之间有 Commit
- 检查 Git 仓库状态

### 生成质量不佳

- 尝试更换模型（如使用更强的 GPT-4）
- 手动编辑生成的日志
- 调整模型参数（无法调整，需要修改脚本）

## 隐私说明

- API Key 仅存储在本地环境变量中
- 不会被写入任何文件
- 不会被提交到 Git 仓库
- 脚本本身也不包含任何敏感信息
