# AI 驱动更新日志生成架构设计方案

## 概述

本方案为 NapCatQQ-Desktop 项目设计了一套完整的 AI 驱动更新日志自动生成架构，支持本地调用多种 AI API（OpenAI、Claude、DeepSeek 等）来生成高质量的更新日志。

---

## 1. 架构设计

### 1.1 整体架构图

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         AI Changelog Generator                          │
├─────────────────────────────────────────────────────────────────────────┤
│  CLI Layer              Core Engine Layer              Output Layer     │
│  ┌──────────┐          ┌──────────────┐               ┌──────────┐     │
│  │ generate │─────────▶│ GitCollector │──┐            │ Markdown │     │
│  │ preview  │          │   (收集)      │  │            │  JSON    │     │
│  │ config   │          ├──────────────┤  │            │  GitHub  │     │
│  └──────────┘          │ContentProcess│  │            └──────────┘     │
│                        │   (处理)      │──┼──▶ AI Provider             │
│                        ├──────────────┤  │    (OpenAI/Claude/...)      │
│                        │   Generator  │──┘                              │
│                        │   (生成)      │                                 │
│                        └──────────────┘                                 │
└─────────────────────────────────────────────────────────────────────────┘
```

### 1.2 模块职责

| 模块 | 职责 | 核心类/函数 |
|-----|------|-----------|
| `cli.py` | 命令行接口 | `main()`, `cmd_generate()`, `cmd_config()` |
| `config.py` | 配置管理 | `Config`, `ConfigManager` |
| `git_collector.py` | Git 数据收集 | `GitCollector`, `CommitInfo` |
| `ai_providers.py` | AI 提供商抽象 | `AIProvider`, `OpenAIProvider`, `ClaudeProvider` |
| `prompts.py` | Prompt 模板 | `get_system_prompt()`, `get_user_prompt()` |
| `generator.py` | 核心生成器 | `ChangelogGenerator`, `ContentProcessor` |
| `integrations.py` | 集成模块 | `UpdateVersionIntegration` |

---

## 2. 数据收集策略

### 2.1 收集的数据类型

| 数据 | 必要性 | Token 成本 | 用途 |
|-----|-------|-----------|------|
| Commit message | 必需 | 低 | 基本变更描述 |
| Type/Scope | 必需 | 低 | 分类和分组 |
| Author/Date | 可选 | 低 | 元数据 |
| File stats | 推荐 | 中 | 变更影响评估 |
| 文件列表 | 推荐 | 中 | 模块识别 |
| Diff 内容 | 可选 | **高** | 详细技术变更 |

### 2.2 过滤策略

**默认排除的 patterns：**
- `^chore\(release\)` - 版本发布
- `^ci\(release\)` - CI 发布
- `^merge branch` - 合并分支
- `^WIP` - 工作进行中

**包含的 types：**
- `feat` - 新功能
- `fix` - Bug 修复
- `perf` - 性能优化
- `refactor` - 重构
- `docs` - 文档
- `breaking` - 破坏性变更

### 2.3 大量 Commits 处理

```
总 Commits
    ├──▶ 过滤（排除 patterns）
    ├──▶ 如果 > max_commits: 按优先级排序
    │           Breaking > Feat > Fix > Perf > Others
    │       取前 max_commits 个（默认 50）
    └──▶ 统计 Token 预算
```

---

## 3. Prompt 工程

### 3.1 系统提示词设计

```markdown
## 角色
你是专业的开源项目更新日志生成助手

## 核心原则
1. 用户导向 - 从用户角度描述
2. 分类清晰 - 按类型组织
3. 突出重点 - 重要变更在前
4. 保持简洁 - 描述简洁明了

## 变更类型
✨ 新增功能 / 🐛 Bug修复 / ⚡ 性能优化 / 
⚠️ 破坏性变更 / 📚 文档更新 / ♻️ 代码重构

## 输出格式
- Markdown 格式
- 适当 emoji 图标
- 按重要性排序
```

### 3.2 用户提示词模板

```markdown
## 版本信息
- 从: {from_version}
- 到: {to_version}
- 提交数: {commit_count}

## 提交记录
{formatted_commits}

## 要求
语言: {language} | 风格: {style} | Emoji: {emoji}
```

### 3.3 Prompt 优化技巧

| 技巧 | 效果 |
|-----|------|
| 角色设定 | 统一输出风格 |
| Few-shot 示例 | 提高输出质量 |
| 结构化输出 | 便于解析 |
| 明确约束 | 控制生成长度 |

---

## 4. 成本控制策略

### 4.1 Token 估算

```python
def estimate_tokens(text: str) -> int:
    cn_chars = sum(1 for c in text if '\u4e00' <= c <= '\u9fff')
    other_chars = len(text) - cn_chars
    return int(cn_chars * 1.5 + other_chars * 0.25)
```

### 4.2 成本控制措施

| 措施 | 节省效果 |
|-----|---------|
| `max_commits=50` | 线性减少 |
| `max_diff_lines=100` | 大幅减少 |
| 智能过滤 | 20-40% |
| 内容压缩 | 10-20% |

### 4.3 成本估算（GPT-4）

| 场景 | 输入 | 输出 | 成本 |
|-----|------|------|-----|
| 小型 (10 commits) | 2K | 1K | ~$0.12 |
| 中型 (30 commits) | 5K | 1.5K | ~$0.24 |
| 大型 (50 commits) | 8K | 2K | ~$0.36 |

**省钱技巧：**
1. 使用 GPT-3.5（便宜 10 倍）
2. 使用 Ollama 本地模型（免费）
3. 减少 `max_commits` 数量

---

## 5. API 设计

### 5.1 核心 API

```python
# 生成更新日志
generator = ChangelogGenerator(config)
result = generator.generate(
    from_tag="v1.0.0",
    to_tag="v1.1.0"
)

# 返回结果
{
    "changelog": "## ✨ 新增功能\n- ...",
    "commits_processed": 30,
    "tokens_total": 4500,
    "model": "gpt-4"
}
```

### 5.2 CLI 接口

```bash
# 配置
changelog config --init
changelog config --set-key sk-xxx

# 生成
changelog generate
changelog generate --from v1.0 --to v1.1 -o CHANGELOG.md

# 预览
changelog preview

# 查看标签
changelog tags
```

---

## 6. 集成方案

### 6.1 与现有 update_version.py 集成

```python
# 原有调用方式（保持不变）
python script/utils/update_version.py v1.8.0

# 新增 AI 支持
python script/utils/update_version.py v1.8.0 --use-ai
python script/utils/update_version.py v1.8.0 --ai-preview
```

### 6.2 CI/CD 集成

```yaml
- name: Generate AI Changelog
  env:
    OPENAI_API_KEY: ${{ secrets.OPENAI_API_KEY }}
  run: |
    python -m script.changelog_generator generate \
      --from ${{ steps.prev.outputs.tag }} \
      --to ${{ github.ref_name }} \
      -o docs/CHANGELOG.md
```

---

## 7. 配置管理

### 7.1 配置项

| 配置项 | 默认值 | 说明 |
|-------|-------|------|
| `provider` | openai | AI 提供商 |
| `model` | gpt-4 | 模型名称 |
| `language` | zh | 输出语言 |
| `style` | conventional | 风格 |
| `max_commits` | 50 | 最大提交数 |
| `include_diff` | False | 包含 diff |
| `token_budget` | 4000 | Token 预算 |

### 7.2 配置位置

- **文件**: `~/.config/changelog-generator/config.json`
- **环境变量**: `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `DEEPSEEK_API_KEY`

---

## 8. 使用示例

### 8.1 快速开始

```bash
# 1. 初始化配置
python -m script.changelog_generator config --init

# 2. 生成更新日志
python -m script.changelog_generator generate

# 3. 保存到文件
python -m script.changelog_generator generate -o docs/CHANGELOG.md
```

### 8.2 高级用法

```bash
# 使用 Claude
python -m script.changelog_generator config --provider claude

# 使用本地 Ollama
python -m script.changelog_generator config --provider ollama

# 预览内容
python -m script.changelog_generator preview --from v1.7.0
```

---

## 9. 扩展性设计

### 9.1 添加新 AI 提供商

```python
class MyProvider(AIProvider):
    def chat(self, messages, **kwargs) -> AIResponse:
        # 实现 API 调用
        pass
```

### 9.2 自定义输出格式

在 `prompt_templates.py` 中添加新的风格模板即可。

---

## 10. 总结

本方案提供了一个**完整、可扩展、成本可控**的 AI 驱动更新日志生成架构：

1. **模块化设计** - 清晰的模块划分，易于维护和扩展
2. **多提供商支持** - 支持 OpenAI、Claude、DeepSeek、Ollama 等
3. **智能数据处理** - 过滤、分组、限制，优化 Token 使用
4. **成本控制** - 多种措施降低 API 调用成本
5. **易于集成** - 与现有工具无缝集成，向后兼容

---

## 相关文件

| 文件 | 说明 |
|-----|------|
| `script/changelog_generator/` | 核心代码目录 |
| `script/changelog_generator/ARCHITECTURE.md` | 详细架构文档 |
| `script/changelog_generator/USAGE.md` | 使用指南 |
| `docs/AI_CHANGELOG_DESIGN.md` | 本设计文档 |
