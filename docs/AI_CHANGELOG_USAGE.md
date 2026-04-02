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
# 基本用法（自动对比上一个 tag）
python script/utils/generate_changelog_ai.py v1.7.28

# 指定对比版本
python script/utils/generate_changelog_ai.py v1.7.28 v1.7.27

# 对比当前未提交的变更
python script/utils/generate_changelog_ai.py HEAD
```

## 完整工作流程

### 场景一：正式发布前优化更新日志

```bash
# 1. 确保所有变更已提交并推送

# 2. 打 tag（但不推送）
git tag v1.7.28

# 3. 生成 AI 更新日志
python script/utils/generate_changelog_ai.py v1.7.28

# 4. 查看生成的文件
#    CHANGELOG_v1.7.28.md

# 5. 手动复制内容到 docs/CHANGELOG.md（或按提示自动更新）

# 6. 提交更新日志
git add docs/CHANGELOG.md
git commit -m "docs: 更新 v1.7.28 更新日志"

# 7. 重新打 tag（覆盖之前的）
git tag -d v1.7.28
git tag v1.7.28

# 8. 推送
git push origin main --tags
```

### 场景二：快速生成草稿

```bash
# 直接生成，不指定 tag（使用 HEAD）
python script/utils/generate_changelog_ai.py HEAD

# 查看结果
cat CHANGELOG_HEAD.md
```

## 输出示例

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

### Q: 生成质量不好

**优化建议**：
- 确保 commit message 写得清晰
- 使用英文或中文明确的提交信息
- 手动编辑生成的日志

### Q: 生成的日志在哪里？

- 文件：`CHANGELOG_{版本号}.md`
- 位置：项目根目录
- 同时会提示是否更新 `docs/CHANGELOG.md`

## 与 CI 的区别

| 场景 | 使用工具 | 说明 |
|------|---------|------|
| 本地优化 | `generate_changelog_ai.py` | AI 理解语义，生成质量高 |
| CI 自动 | `update_version.py` | 规则匹配，无需配置 |

**建议**：
- 重要版本在本地用 AI 工具优化
- 小版本让 CI 自动生成
