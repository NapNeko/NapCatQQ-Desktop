# AI 更新日志生成指南（本地使用）

## 简介

此脚本仅在**本地使用**，通过 `.env` 文件配置 API Key。`.env` 文件已被 `.gitignore` 忽略，不会提交到 Git 仓库。

## 前置要求

```bash
# 安装依赖
pip install httpx
```

## 配置（本地 .env 文件）

### 1. 创建配置文件

```bash
cd script/utils
cp .env.example .env
```

### 2. 编辑 .env 文件

```bash
# 使用你喜欢的编辑器编辑 .env
nano .env
# 或
notepad .env
```

内容示例：

```ini
# 必需：API Key（二选一）
OPENAI_API_KEY="sk-or-v1-你的密钥"
# 或
OPENROUTER_API_KEY="sk-or-v1-你的密钥"

# 可选：API 地址（默认使用 OpenRouter）
OPENAI_API_URL="https://openrouter.ai/api/v1/chat/completions"

# 可选：模型名称（默认使用免费模型）
OPENAI_MODEL="z-ai/glm-4.5-air:free"
```

### API 提供商推荐

| 提供商 | URL | 免费模型 |
|--------|-----|----------|
| OpenRouter | `https://openrouter.ai/api/v1/chat/completions` | `z-ai/glm-4.5-air:free` |
| SiliconFlow | `https://api.siliconflow.cn/v1/chat/completions` | 需要 API Key |
| OpenAI | `https://api.openai.com/v1/chat/completions` | 需要付费 |

**注意**：`.env` 文件已被 `.gitignore` 忽略，不会被提交到仓库。

## 使用方法

```bash
# 确保在脚本目录有 .env 文件
python script/utils/generate_changelog_ai.py v1.7.28
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
============================================================

✅ 已保存到: CHANGELOG_v1.7.28.md
是否更新 docs/CHANGELOG.md? (y/n):
```

## 隐私说明

- ✅ `.env` 文件已被 `.gitignore` 忽略
- ✅ API Key 仅存储在本地 `.env` 文件中
- ✅ 不会被写入任何其他文件
- ✅ 不会被提交到 Git 仓库
- ✅ 脚本本身也不包含任何敏感信息

## 故障排除

### API 调用失败

```
⚠️ AI 调用失败: [错误信息]
使用默认模板生成...
```

- 检查 `.env` 文件是否存在且配置正确
- 检查 API Key 是否有效
- 检查网络连接

### 没有生成任何内容

- 确保两个 Tag 之间有 Commit
- 检查 Git 仓库状态

### 生成质量不佳

- 尝试更换模型（如使用更强的 GPT-4）
- 手动编辑生成的日志
