# AI 更新日志生成指南

## 简介

本项目支持使用 AI 自动生成格式化的更新日志。通过对比两个 Git Tag 之间的 Commit 记录，AI 会自动分类、合并、生成符合项目规范的更新日志。

## 前置要求

```bash
# 安装依赖
pip install httpx
```

## 配置

### 1. 创建配置文件

```bash
mkdir -p ~/.config/ncd
cp script/utils/changelog_config.example.json ~/.config/ncd/changelog_config.json
```

### 2. 编辑配置文件

```json
{
  "api_key": "你的API密钥",
  "api_url": "https://openrouter.ai/api/v1/chat/completions",
  "model": "z-ai/glm-4.5-air:free",
  "max_tokens": 5000,
  "temperature": 0.2
}
```

### API 提供商推荐

| 提供商 | URL | 免费模型 |
|--------|-----|----------|
| OpenRouter | `https://openrouter.ai/api/v1/chat/completions` | `z-ai/glm-4.5-air:free` |
| SiliconFlow | `https://api.siliconflow.cn/v1/chat/completions` | 需要 API Key |
| OpenAI | `https://api.openai.com/v1/chat/completions` | 需要付费 |

### 环境变量方式（可选）

如果不想创建配置文件，也可以设置环境变量：

```bash
export OPENAI_API_KEY="sk-..."
# 或
export OPENROUTER_API_KEY="sk-or-v1-..."
```

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

## 建议的工作流程

### 方案一：纯 AI 生成（推荐）

```bash
# 1. 手动更新版本号到 pyproject.toml
# 2. 提交并打 tag
git add -A
git commit -m "chore: bump version to v1.7.28"
git tag v1.7.28

# 3. 生成更新日志
python script/utils/generate_changelog_ai.py v1.7.28

# 4. 更新 CHANGELOG.md
# （按提示输入 y 更新 docs/CHANGELOG.md）

# 5. 推送
git push origin main --tags
```

### 方案二：与 update_version.py 结合

```bash
# 1. 使用旧脚本更新版本号（如果不需要 AI 生成）
python script/utils/update_version.py v1.7.28

# 2. 或使用 AI 脚本仅生成日志
python script/utils/generate_changelog_ai.py v1.7.28
```

## 故障排除

### API 调用失败

```
⚠️ AI 调用失败: [错误信息]
使用默认模板生成...
```

- 检查 API Key 是否正确
- 检查网络连接
- 检查 API URL 是否可访问

### 没有生成任何内容

- 确保两个 Tag 之间有 Commit
- 检查 Git 仓库状态

### 生成质量不佳

- 尝试更换模型（如使用更强的 GPT-4）
- 手动编辑生成的日志
- 调整 `temperature` 参数（越低越稳定）

## 参考

- 参考项目：[NapCatQQ](https://github.com/NapNeko/NapCatQQ)
- OpenRouter: https://openrouter.ai/
