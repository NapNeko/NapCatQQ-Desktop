# AI 更新日志生成脚本使用指南

## 快速开始

### 1. 安装依赖

```bash
pip install httpx
```

### 2. 配置 API Key

```bash
# 进入脚本目录
cd script/utils

# 复制配置模板
cp .env.example .env

# 编辑 .env 文件（用记事本/vscode/vim 都可以）
notepad .env
```

**.env 文件内容：**

```ini
# 填入你的 API Key（二选一）
OPENAI_API_KEY="sk-or-v1-你的密钥"
# 或
OPENROUTER_API_KEY="sk-or-v1-你的密钥"

# 可选：使用其他模型
OPENAI_MODEL="z-ai/glm-4.5-air:free"
```

**获取免费 API Key：**
- OpenRouter: https://openrouter.ai/keys （注册免费，有免费额度）

### 3. 使用脚本

```bash
# 基本用法（自动对比上一个 tag，并进入交互式改稿）
python script/utils/generate_changelog_ai.py v1.7.28

# 指定对比版本
python script/utils/generate_changelog_ai.py v1.7.28 v1.7.27

# 对比当前未提交的变更
python script/utils/generate_changelog_ai.py HEAD
```

## 当前推荐工作流程

### 场景一：一键正式发布（推荐）

```bash
# 1. 确保所有变更已提交

# 2. 直接执行一键发布
python script/utils/release.py v1.7.28

# 3. 在控制台中持续给 AI 反馈，直到你满意

# 4. 你确认后，脚本才会继续：
#    - 更新版本文件
#    - 写入 docs/CHANGELOG.md
#    - 执行 uv lock
#    - 创建 release commit
#    - 创建 tag
```

如果要发布后立即推送：

```bash
python script/utils/release.py v1.7.28 --push
```

### 场景二：单独预演 AI 更新日志

适用于：你只想先试写一版更新日志，不想真的发布。

```bash
# 单独生成 AI 更新日志草稿
python script/utils/generate_changelog_ai.py v1.7.28
```

### 场景三：快速生成草稿

```bash
# 直接生成，不指定 tag（使用 HEAD）
python script/utils/generate_changelog_ai.py HEAD

# 查看结果
cat CHANGELOG_HEAD.md
```

## 交互式改稿示例

```
🚀 生成更新日志: v1.7.28
📌 自动检测到上一版本: v1.7.27
📝 收集 commit 记录...
   找到 12 个 commit
📊 获取文件变化统计...
🎯 生成更新日志...
🤖 正在调用 AI API (z-ai/glm-4.5-air:free)...

============================================================
当前发布说明预览：
============================================================
## 🐛 修复功能
- 修复 MSI 更新脚本参数传递问题
- 修复下载器异常捕获过于宽泛问题

## ✨ 新增功能
- 新增临时目录回退机制
- 支持多变量注入到 Batch 脚本

## 🔧 优化
- 优化安装类型检测的路径比较逻辑
============================================================

输入说明：
- 输入 y / 满意 / ok：确认当前结果
- 输入 r / 重新生成：让 AI 在当前上下文下重写一版
- 输入其他任意内容：作为修改意见继续对话
- 输入 q / 退出：退出且不写回 docs/CHANGELOG.md

你的反馈: 把“优化”写得更偏用户视角，不要太技术化

...（AI 按反馈继续改稿，直到你确认）

✅ 已保存预览到: CHANGELOG_v1.7.28.md
是否写回 docs/CHANGELOG.md? (y/n):
```

## 常见问题

### Q: 提示 "未配置 API Key"

**原因**：`.env` 文件不存在或配置错误

**解决**：
```bash
cd script/utils
cp .env.example .env
# 编辑 .env，填入有效的 API Key
```

### Q: API 调用失败

**可能原因**：
- API Key 无效或过期
- 网络问题
- 免费额度用完

**解决**：
1. 检查 API Key 是否正确
2. 测试网络连通性
3. 更换 API 提供商

### Q: 生成质量不好，或者我想继续改

**优化建议**：
- 确保 commit message 写得清晰
- 使用英文或中文明确的提交信息
- 直接在控制台输入修改意见，脚本会继续带上下文与 AI 对话
- 如果想完全重写一版，可输入 `r`
- 满意后输入 `y`

### Q: 生成的日志在哪里？

- 文件：`CHANGELOG_{版本号}.md`
- 位置：项目根目录
- 同时会提示是否更新 `docs/CHANGELOG.md`

### Q: `docs/CHANGELOG.md` 顶部版本号会自动更新吗？

会。

当你使用语义化版本号或 tag（如 `2.0.18`、`v2.0.18`）运行脚本，并确认写回 [`docs/CHANGELOG.md`](docs/CHANGELOG.md) 时，脚本会同时：

- 更新标题中的版本号
- 替换 `<!-- BEGIN AUTO RELEASE NOTES -->` 到 `<!-- END AUTO RELEASE NOTES -->` 之间的自动生成区域
- 保留 Tips 和固定提醒等人工内容

### Q: 现在正式发布时，会自动进入 AI 改稿吗？

会。

现在 [`script/utils/release.py`](script/utils/release.py) 已经默认接入 AI 交互式发布说明流程：

- 先收集这次发布范围内的 commit 和文件变更
- 先生成一版 AI 发布说明
- 你在控制台里持续提修改意见
- 只有你确认满意后，正式发布流程才会继续

如果你只是想单独试写一版、不想发布，再使用 [`script/utils/generate_changelog_ai.py`](script/utils/generate_changelog_ai.py)。

## 与 CI 的区别

| 场景 | 使用工具 | 说明 |
|------|---------|------|
| 正式一键发布 | `release.py` | 默认先进入 AI 交互改稿，再继续提交、打 tag、可选推送 |
| 单独预演 AI 日志 | `generate_changelog_ai.py` | 只生成/改稿，不执行正式发布 |
| 本地规则预演 | `update_version.py` | 规则匹配，便于调试版本同步和 changelog 结构 |

**建议**：
- 正式发布优先使用 [`script/utils/release.py`](script/utils/release.py)
- 只想单独练稿时使用 [`script/utils/generate_changelog_ai.py`](script/utils/generate_changelog_ai.py)
- 只想验证版本同步和规则生成时使用 [`script/utils/update_version.py`](script/utils/update_version.py)
