#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
AI 驱动的更新日志生成脚本

功能：
1. 通过对比两个 git tag 获取变更
2. 调用 AI API 生成格式化的更新日志
3. 支持 OpenAI、OpenRouter 等兼容 API

使用方法：
    python generate_changelog_ai.py <current_tag> [previous_tag]
    
示例：
    python generate_changelog_ai.py v1.7.28          # 自动查找上一个 tag
    python generate_changelog_ai.py v1.7.28 v1.7.27  # 指定对比版本

配置文件：
    ~/.config/ncd/changelog_config.json
    {
        "api_key": "your-api-key",
        "api_url": "https://openrouter.ai/api/v1/chat/completions",
        "model": "z-ai/glm-4.5-air:free",
        "max_tokens": 5000,
        "temperature": 0.2
    }
"""

import json
import os
import re
import subprocess
import sys
from pathlib import Path
from typing import Optional

import httpx


# 默认配置
DEFAULT_CONFIG = {
    "api_url": "https://openrouter.ai/api/v1/chat/completions",
    "model": "z-ai/glm-4.5-air:free",
    "max_tokens": 5000,
    "temperature": 0.2,
}

# 系统 Prompt - 指导 AI 生成格式化的更新日志
SYSTEM_PROMPT = """# NapCatQQ Desktop 发布说明生成器

你是 NapCatQQ Desktop 项目的发布说明生成器。请根据提供的 commit 列表生成标准格式的发布说明。

## 核心规则

1. **版本号**：第一行必须是 `# {VERSION}`，使用用户提供的版本号
2. **语言**：全部使用简体中文
3. **格式**：严格按照下方模板输出

## Commit 分析规则

将 commit 分类为以下类型：
- 🐛 **修复**：bug fix、修复、fix 相关
- ✨ **新增**：新功能、feat、add 相关  
- 🔧 **优化**：优化、重构、refactor、improve、perf 相关
- 📦 **依赖**：deps、依赖更新（通常可以忽略或合并）
- 🔨 **构建**：ci、build、workflow 相关（通常可以忽略）

## 合并和筛选

- **合并相似项**：同一功能的多个 commit 合并为一条
- **忽略琐碎项**：合并冲突、格式化、typo 等可忽略
- **控制数量**：最终保持 5-15 条更新要点
- **保留 commit hash**：每条末尾附上短 hash，格式 `(a1b2c3d)`

## 输出模板

```
# {VERSION}

## 🐛 修复
1. 修复 xxx 问题 (a1b2c3d)
2. 修复 yyy 崩溃 (b2c3d4e)

## ✨ 新增
1. 新增 xxx 功能 (c3d4e5f)
2. 支持 yyy 特性 (d4e5f6g)

## 🔧 优化
1. 优化 xxx 性能 (e5f6g7h)
2. 重构 yyy 模块 (f6g7h8i)
```

## 重要约束

1. 如果某个分类没有内容，则完全省略该分类
2. 不要编造不存在的更新
3. 保持简洁，每条更新控制在一行内
4. 使用用户友好的语言，避免过于技术化的描述
5. 重大变更（Breaking Changes）需要加粗提示
"""


def get_config() -> dict:
    """读取配置文件"""
    config_path = Path.home() / ".config" / "ncd" / "changelog_config.json"
    
    if config_path.exists():
        with open(config_path, "r", encoding="utf-8") as f:
            config = json.load(f)
    else:
        config = {}
    
    # 合并默认配置
    for key, value in DEFAULT_CONFIG.items():
        if key not in config:
            config[key] = value
    
    # 检查 API key
    if "api_key" not in config or not config["api_key"]:
        # 尝试从环境变量读取
        config["api_key"] = os.environ.get("OPENAI_API_KEY") or os.environ.get("OPENROUTER_API_KEY")
    
    if not config.get("api_key"):
        print("❌ 错误：未配置 API Key")
        print(f"请在 {config_path} 中配置 api_key，或设置环境变量 OPENAI_API_KEY / OPENROUTER_API_KEY")
        print("\n配置文件示例：")
        print(json.dumps({
            "api_key": "sk-...",
            "api_url": "https://openrouter.ai/api/v1/chat/completions",
            "model": "z-ai/glm-4.5-air:free",
        }, indent=2, ensure_ascii=False))
        sys.exit(1)
    
    return config


def run_git_command(command: str) -> str:
    """运行 git 命令并返回输出"""
    try:
        result = subprocess.run(
            command,
            shell=True,
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
            check=True,
        )
        return (result.stdout or "").strip()
    except subprocess.CalledProcessError as e:
        print(f"❌ Git 命令执行失败: {command}")
        print(f"错误: {e.stderr}")
        sys.exit(1)


def get_previous_tag(current_tag: str) -> Optional[str]:
    """获取上一个版本 tag"""
    try:
        # 获取所有 tag 按版本排序
        tags = run_git_command("git tag -l --sort=-v:refname")
        tag_list = [t for t in tags.split("\n") if t.strip()]
        
        # 找到当前 tag 的索引
        for i, tag in enumerate(tag_list):
            if tag == current_tag and i + 1 < len(tag_list):
                return tag_list[i + 1]
        
        return None
    except Exception:
        return None


def get_commits_between_tags(from_tag: Optional[str], to_tag: str) -> list:
    """获取两个 tag 之间的所有 commit"""
    if from_tag:
        commit_range = f"{from_tag}..{to_tag}"
    else:
        commit_range = to_tag
    
    commits = run_git_command(f"git log {commit_range} --pretty=format:'%s (%h)'")
    return [c for c in commits.split("\n") if c.strip()]


def get_file_stats(from_tag: Optional[str], to_tag: str) -> tuple:
    """获取文件变化统计"""
    if from_tag:
        diff_range = f"{from_tag}..{to_tag}"
    else:
        diff_range = to_tag
    
    # 获取统计信息
    stats = run_git_command(f"git diff --stat {diff_range}")
    
    # 获取变更文件列表
    files = run_git_command(f"git diff --name-only {diff_range}")
    file_list = [f for f in files.split("\n") if f.strip()]
    
    return stats, file_list


def generate_changelog_with_ai(
    config: dict,
    current_tag: str,
    previous_tag: Optional[str],
    commits: list,
    file_stats: str,
    file_list: list,
) -> str:
    """调用 AI API 生成更新日志"""
    
    # 构建用户提示
    user_content = f"""当前版本: {current_tag}
上一版本: {previous_tag or "(首次发布)"}

## 提交列表
{chr(10).join(f"- {c}" for c in commits)}

## 文件变化统计
{file_stats}

## 变更文件列表
共 {len(file_list)} 个文件变更
{chr(10).join(f"- {f}" for f in file_list[:50])}
"""
    
    # 构建请求体
    payload = {
        "model": config["model"],
        "messages": [
            {"role": "system", "content": SYSTEM_PROMPT},
            {"role": "user", "content": user_content},
        ],
        "temperature": config["temperature"],
        "max_tokens": config["max_tokens"],
    }
    
    # 调用 API
    headers = {
        "Authorization": f"Bearer {config['api_key']}",
        "Content-Type": "application/json",
    }
    
    print(f"🤖 正在调用 AI API ({config['model']})...")
    
    try:
        response = httpx.post(
            config["api_url"],
            headers=headers,
            json=payload,
            timeout=120,
        )
        response.raise_for_status()
        
        result = response.json()
        
        # 提取生成的内容
        if "choices" in result and len(result["choices"]) > 0:
            content = result["choices"][0]["message"]["content"]
            # 替换版本号占位符
            content = content.replace("{VERSION}", current_tag)
            if previous_tag:
                content = content.replace("{PREV_VERSION}", previous_tag)
            return content
        else:
            print("⚠️ AI 返回格式异常，使用默认模板")
            return generate_fallback_changelog(current_tag, previous_tag, commits)
    
    except Exception as e:
        print(f"⚠️ AI 调用失败: {e}")
        print("使用默认模板生成...")
        return generate_fallback_changelog(current_tag, previous_tag, commits)


def generate_fallback_changelog(
    current_tag: str, 
    previous_tag: Optional[str], 
    commits: list
) -> str:
    """备用：使用简单规则生成更新日志"""
    lines = [f"# {current_tag}", ""]
    
    # 简单的分类
    fixes = []
    features = []
    others = []
    
    for commit in commits:
        commit_lower = commit.lower()
        if any(kw in commit_lower for kw in ["fix", "修复", "bug"]):
            fixes.append(commit)
        elif any(kw in commit_lower for kw in ["feat", "新增", "add", "功能"]):
            features.append(commit)
        else:
            others.append(commit)
    
    if features:
        lines.append("## ✨ 新增")
        for i, item in enumerate(features[:10], 1):
            lines.append(f"{i}. {item}")
        lines.append("")
    
    if fixes:
        lines.append("## 🐛 修复")
        for i, item in enumerate(fixes[:10], 1):
            lines.append(f"{i}. {item}")
        lines.append("")
    
    if others:
        lines.append("## 🔧 其他")
        for i, item in enumerate(others[:5], 1):
            lines.append(f"{i}. {item}")
        lines.append("")
    
    return "\n".join(lines)


def main():
    """主函数"""
    if len(sys.argv) < 2:
        print("用法: python generate_changelog_ai.py <current_tag> [previous_tag]")
        print("示例:")
        print("  python generate_changelog_ai.py v1.7.28")
        print("  python generate_changelog_ai.py v1.7.28 v1.7.27")
        sys.exit(1)
    
    current_tag = sys.argv[1]
    previous_tag = sys.argv[2] if len(sys.argv) > 2 else None
    
    print(f"🚀 生成更新日志: {current_tag}")
    
    # 如果没有提供上一个 tag，自动查找
    if not previous_tag:
        previous_tag = get_previous_tag(current_tag)
        if previous_tag:
            print(f"📌 自动检测到上一版本: {previous_tag}")
        else:
            print("📌 未找到上一版本，将使用所有历史 commit")
    
    # 读取配置
    config = get_config()
    
    # 获取 commit 列表
    print("📝 收集 commit 记录...")
    commits = get_commits_between_tags(previous_tag, current_tag)
    print(f"   找到 {len(commits)} 个 commit")
    
    if not commits:
        print("⚠️ 没有找到任何 commit，退出")
        sys.exit(0)
    
    # 获取文件变化统计
    print("📊 获取文件变化统计...")
    file_stats, file_list = get_file_stats(previous_tag, current_tag)
    
    # 生成更新日志
    print("🎯 生成更新日志...")
    changelog = generate_changelog_with_ai(
        config, current_tag, previous_tag, commits, file_stats, file_list
    )
    
    # 输出结果
    print("\n" + "=" * 60)
    print("生成的更新日志：")
    print("=" * 60)
    print(changelog)
    print("=" * 60)
    
    # 保存到文件
    output_file = Path(f"CHANGELOG_{current_tag}.md")
    output_file.write_text(changelog, encoding="utf-8")
    print(f"\n✅ 已保存到: {output_file}")
    
    # 同时更新 docs/CHANGELOG.md（可选）
    docs_changelog = Path("docs/CHANGELOG.md")
    if docs_changelog.exists():
        response = input("\n是否更新 docs/CHANGELOG.md? (y/n): ")
        if response.lower() == "y":
            # 读取现有内容
            existing = docs_changelog.read_text(encoding="utf-8")
            # 在标题后插入新内容
            new_content = existing.replace(
                "# 🚀 NapCatQQ Desktop 更新日志",
                f"# 🚀 NapCatQQ Desktop 更新日志\n\n{changelog}"
            )
            docs_changelog.write_text(new_content, encoding="utf-8")
            print("✅ 已更新 docs/CHANGELOG.md")


if __name__ == "__main__":
    main()
