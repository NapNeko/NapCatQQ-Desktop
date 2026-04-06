#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
AI 驱动的更新日志生成脚本（本地使用版）

功能：
1. 通过对比两个 git tag 获取变更
2. 调用 AI API 生成格式化的更新日志
3. 支持 OpenAI、OpenRouter 等兼容 API

使用方法：
    python generate_changelog_ai.py <current_tag> [previous_tag]
    
示例：
    python generate_changelog_ai.py v1.7.28          # 自动查找上一个 tag
    python generate_changelog_ai.py v1.7.28 v1.7.27  # 指定对比版本

本地配置（不提交到 Git）：
    1. 复制 .env.example 为 .env
    2. 在 .env 中填入你的 API Key

注意：.env 文件已被 .gitignore 忽略，不会提交到仓库
"""

import os
import re
import subprocess
import sys
from pathlib import Path
from typing import Optional

import httpx

PROJECT_ROOT = Path(__file__).resolve().parents[2]
if str(PROJECT_ROOT) not in sys.path:
    sys.path.insert(0, str(PROJECT_ROOT))

from script.utils.release_helpers import collect_diff_stats, parse_version, render_changelog_document


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

1. **语言**：全部使用简体中文
2. **输出内容**：只输出“自动发布说明区域”的正文，不要输出整份文档，不要输出版本标题
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
- **不要附 commit hash**：最终面向用户展示，不需要在条目末尾附 hash

## 输出模板

```
## 🐛 修复功能
- 修复 xxx 问题
- 修复 yyy 崩溃

## ✨ 新增功能
- 新增 xxx 功能
- 支持 yyy 特性

## 🔧 优化功能
- 优化 xxx 性能
- 重构 yyy 模块
```

## 重要约束

1. 如果某个分类没有内容，则完全省略该分类
2. 不要编造不存在的更新
3. 保持简洁，每条更新控制在一行内
4. 使用用户友好的语言，避免过于技术化的描述
5. 重大变更（Breaking Changes）需要加粗提示
6. 允许根据用户反馈反复改写，直到用户满意为止
7. 输出必须是 Markdown，列表统一使用 `- ` 前缀
"""

ACCEPT_WORDS = {"y", "yes", "ok", "okay", "确认", "满意", "通过", "accept"}
RETRY_WORDS = {"r", "retry", "重新生成", "重写", "再来一次"}
QUIT_WORDS = {"q", "quit", "exit", "退出", "取消"}


class ReviewCancelled(Exception):
    """用户主动结束交互改稿。"""

    def __init__(self, notes: str):
        super().__init__("review cancelled")
        self.notes = notes


def load_env_file():
    """加载 .env 文件（本地使用，不提交到仓库）"""
    script_dir = Path(__file__).parent
    env_path = script_dir / ".env"
    
    if env_path.exists():
        with open(env_path, "r", encoding="utf-8") as f:
            for line in f:
                line = line.strip()
                if not line or line.startswith("#"):
                    continue
                if "=" in line:
                    key, value = line.split("=", 1)
                    key = key.strip()
                    value = value.strip().strip('"').strip("'")
                    if key and key not in os.environ:
                        os.environ[key] = value


def get_config(*, exit_on_error: bool = True) -> dict:
    """读取配置（优先 .env 文件，其次环境变量）"""
    config = DEFAULT_CONFIG.copy()
    
    # 先加载 .env 文件
    load_env_file()
    
    # 读取 API Key（必需）
    api_key = os.environ.get("OPENAI_API_KEY") or os.environ.get("OPENROUTER_API_KEY")
    if not api_key:
        message_lines = [
            "❌ 错误：未配置 API Key",
            "",
            "请在脚本目录创建 .env 文件：",
            f"  {Path(__file__).parent / '.env'}",
            "",
            ".env 文件内容示例：",
            '  OPENAI_API_KEY="sk-or-v1-..."',
            '  # 或',
            '  OPENROUTER_API_KEY="sk-or-v1-..."',
            "",
            "可选配置：",
            f'  OPENAI_API_URL="{DEFAULT_CONFIG["api_url"]}"',
            f'  OPENAI_MODEL="{DEFAULT_CONFIG["model"]}"',
        ]
        if exit_on_error:
            for line in message_lines:
                print(line)
            sys.exit(1)
        raise RuntimeError("\n".join(message_lines))
    
    config["api_key"] = api_key
    
    # 可选配置
    if "OPENAI_API_URL" in os.environ:
        config["api_url"] = os.environ["OPENAI_API_URL"]
    if "OPENAI_MODEL" in os.environ:
        config["model"] = os.environ["OPENAI_MODEL"]
    
    return config


def safe_print(text: object = "") -> None:
    """兼容中文控制台输出。"""
    message = str(text)
    try:
        print(message)
    except UnicodeEncodeError:
        encoding = sys.stdout.encoding or "utf-8"
        sys.stdout.flush()
        sys.stdout.buffer.write((message + "\n").encode(encoding, errors="replace"))
        sys.stdout.flush()


def safe_input(prompt: str) -> str:
    """兼容中文控制台输入提示。"""
    try:
        return input(prompt)
    except UnicodeEncodeError:
        safe_print(prompt)
        return input()


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
        tags = run_git_command("git tag -l --sort=-v:refname")
        tag_list = [t for t in tags.split("\n") if t.strip()]
        
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
    return collect_diff_stats(from_tag, root=PROJECT_ROOT, to_ref=to_tag)


def normalize_version_for_docs(current_tag: str) -> Optional[str]:
    """若当前引用是语义化版本，则返回标准 tag 形式。"""
    try:
        return parse_version(current_tag).tag
    except Exception:
        return None


def build_user_prompt(
    current_tag: str,
    previous_tag: Optional[str],
    commits: list[str],
    file_stats: str,
    file_list: list[str],
) -> str:
    """构建初始用户提示词。"""
    return f"""当前版本: {current_tag}
上一版本: {previous_tag or "(首次发布)"}

## 提交列表
{chr(10).join(f"- {c}" for c in commits)}

## 文件变化统计
{file_stats}

## 变更文件列表
共 {len(file_list)} 个文件变更
{chr(10).join(f"- {f}" for f in file_list[:80])}

请输出适合写入 docs/CHANGELOG.md 自动生成区域的 Markdown 正文。
"""


def sanitize_ai_output(content: str) -> str:
    """清理 AI 输出，确保只保留正文。"""
    text = content.strip().replace("\r\n", "\n")
    text = re.sub(r"^```(?:markdown|md)?\s*", "", text, flags=re.IGNORECASE)
    text = re.sub(r"\s*```$", "", text)
    text = re.sub(r"^#\s+[^\n]+\n+", "", text)
    return text.strip()


def call_ai_api(config: dict, messages: list[dict[str, str]]) -> str:
    """调用 AI API 并返回正文。"""
    payload = {
        "model": config["model"],
        "messages": messages,
        "temperature": config["temperature"],
        "max_tokens": config["max_tokens"],
    }

    headers = {
        "Authorization": f"Bearer {config['api_key']}",
        "Content-Type": "application/json",
    }

    safe_print(f"🤖 正在调用 AI API ({config['model']})...")
    response = httpx.post(
        config["api_url"],
        headers=headers,
        json=payload,
        timeout=120,
    )
    response.raise_for_status()

    result = response.json()
    if "choices" not in result or not result["choices"]:
        raise ValueError("AI 返回格式异常：缺少 choices")
    content = result["choices"][0]["message"]["content"]
    return sanitize_ai_output(content)


def generate_changelog_with_ai(
    config: dict,
    current_tag: str,
    previous_tag: Optional[str],
    commits: list,
    file_stats: str,
    file_list: list,
) -> tuple[str, list[dict[str, str]]]:
    """调用 AI API 生成更新日志，并返回会话消息。"""

    messages = [
        {"role": "system", "content": SYSTEM_PROMPT},
        {
            "role": "user",
            "content": build_user_prompt(
                current_tag,
                previous_tag,
                commits,
                file_stats,
                file_list,
            ),
        },
    ]

    try:
        return call_ai_api(config, messages), messages
    except Exception as e:
        safe_print(f"⚠️ AI 调用失败: {e}")
        safe_print("使用默认模板生成...")
        return generate_fallback_changelog(current_tag, previous_tag, commits), messages


def revise_changelog_with_ai(
    config: dict,
    messages: list[dict[str, str]],
    current_notes: str,
    feedback: str,
) -> str:
    """基于用户反馈继续对话，直到满意。"""
    revision_messages = [*messages]
    revision_messages.append({"role": "assistant", "content": current_notes})
    revision_messages.append(
        {
            "role": "user",
            "content": (
                "请根据以下反馈继续修改发布说明，只输出修改后的 Markdown 正文，不要解释。\n\n"
                f"用户反馈：{feedback}"
            ),
        }
    )

    try:
        revised = call_ai_api(config, revision_messages)
    except Exception as e:
        safe_print(f"⚠️ AI 改写失败: {e}")
        safe_print("保留当前版本，不覆盖已有草稿。")
        return current_notes

    messages[:] = revision_messages
    return revised


def generate_fallback_changelog(
    current_tag: str, 
    previous_tag: Optional[str], 
    commits: list
) -> str:
    """备用：使用简单规则生成更新日志"""
    lines: list[str] = []
    
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
        lines.append("## ✨ 新增功能")
        for item in features[:10]:
            lines.append(f"- {item}")
        lines.append("")
    
    if fixes:
        lines.append("## 🐛 修复功能")
        for item in fixes[:10]:
            lines.append(f"- {item}")
        lines.append("")
    
    if others:
        lines.append("## 🧰 其他更新")
        for item in others[:5]:
            lines.append(f"- {item}")
        lines.append("")
    
    return "\n".join(lines).strip() or "## 🧰 其他更新\n- 累积维护更新"


def render_preview_document(current_tag: str, notes: str) -> str:
    """根据当前版本生成预览文档。"""
    normalized_version = normalize_version_for_docs(current_tag)
    if normalized_version:
        existing = (PROJECT_ROOT / "docs/CHANGELOG.md").read_text(encoding="utf-8")
        return render_changelog_document(normalized_version, notes, existing_content=existing)
    return notes.strip() + "\n"


def save_preview_file(current_tag: str, preview_content: str) -> Path:
    """保存预览文件。"""
    safe_tag = re.sub(r"[^0-9A-Za-z._-]+", "_", current_tag.strip()) or "draft"
    output_file = PROJECT_ROOT / f"CHANGELOG_{safe_tag}.md"
    output_file.write_text(preview_content, encoding="utf-8")
    return output_file


def apply_to_docs_changelog(current_tag: str, notes: str) -> bool:
    """将生成结果写回 docs/CHANGELOG.md。"""
    normalized_version = normalize_version_for_docs(current_tag)
    if not normalized_version:
        safe_print("⚠️ 当前版本不是语义化版本号，跳过写回 docs/CHANGELOG.md。")
        return False

    docs_changelog = PROJECT_ROOT / "docs/CHANGELOG.md"
    existing = docs_changelog.read_text(encoding="utf-8") if docs_changelog.exists() else ""
    rendered = render_changelog_document(normalized_version, notes, existing_content=existing)
    docs_changelog.write_text(rendered, encoding="utf-8")
    return True


def interactive_review_loop(config: dict, messages: list[dict[str, str]], notes: str) -> str:
    """在控制台中与用户持续对话，直到满意。"""
    current_notes = notes.strip()

    while True:
        safe_print("\n" + "=" * 60)
        safe_print("当前发布说明预览：")
        safe_print("=" * 60)
        safe_print(current_notes)
        safe_print("=" * 60)
        safe_print("输入说明：")
        safe_print("- 输入 y / 满意 / ok：确认当前结果")
        safe_print("- 输入 r / 重新生成：让 AI 在当前上下文下重写一版")
        safe_print("- 输入其他任意内容：作为修改意见继续对话")
        safe_print("- 输入 q / 退出：退出且不写回 docs/CHANGELOG.md")

        feedback = safe_input("\n你的反馈: ").strip()
        normalized = feedback.lower()

        if normalized in ACCEPT_WORDS:
            return current_notes
        if normalized in QUIT_WORDS:
            raise ReviewCancelled(current_notes)

        if normalized in RETRY_WORDS:
            feedback = "请完全重写一版，但仍基于同一批提交，保持更像正式发布说明。"

        current_notes = revise_changelog_with_ai(config, messages, current_notes, feedback)


def main():
    """主函数"""
    if len(sys.argv) < 2:
        print("用法: python generate_changelog_ai.py <current_tag> [previous_tag]")
        print("示例:")
        print("  python generate_changelog_ai.py v1.7.28")
        print("  python generate_changelog_ai.py v1.7.28 v1.7.27")
        print("")
        print("本地配置（不提交到 Git）：")
        print(f"  创建文件: {Path(__file__).parent / '.env'}")
        print("  内容:")
        print('    OPENAI_API_KEY="sk-or-v1-..."')
        print('    OPENAI_MODEL="z-ai/glm-4.5-air:free"')
        sys.exit(1)
    
    current_tag = sys.argv[1]
    previous_tag = sys.argv[2] if len(sys.argv) > 2 else None
    
    safe_print(f"🚀 生成更新日志: {current_tag}")
    
    if not previous_tag:
        previous_tag = get_previous_tag(current_tag)
        if previous_tag:
            safe_print(f"📌 自动检测到上一版本: {previous_tag}")
        else:
            safe_print("📌 未找到上一版本，将使用所有历史 commit")
    
    config = get_config()
    
    safe_print("📝 收集 commit 记录...")
    commits = get_commits_between_tags(previous_tag, current_tag)
    safe_print(f"   找到 {len(commits)} 个 commit")
    
    if not commits:
        safe_print("⚠️ 没有找到任何 commit，退出")
        sys.exit(0)
    
    safe_print("📊 获取文件变化统计...")
    file_stats, file_list = get_file_stats(previous_tag, current_tag)
    
    safe_print("🎯 生成更新日志...")
    changelog, messages = generate_changelog_with_ai(
        config, current_tag, previous_tag, commits, file_stats, file_list
    )

    try:
        final_notes = interactive_review_loop(config, messages, changelog)
    except ReviewCancelled as exc:
        safe_print("\n⚠️ 已取消，本次不会写回 docs/CHANGELOG.md。")
        preview = render_preview_document(current_tag, exc.notes)
        output_file = save_preview_file(current_tag, preview)
        safe_print(f"✅ 已保留当前草稿到: {output_file}")
        return

    preview = render_preview_document(current_tag, final_notes)
    output_file = save_preview_file(current_tag, preview)
    safe_print(f"\n✅ 已保存预览到: {output_file}")

    response = safe_input("是否写回 docs/CHANGELOG.md? (y/n): ").strip().lower()
    if response in ACCEPT_WORDS:
        if apply_to_docs_changelog(current_tag, final_notes):
            normalized_version = normalize_version_for_docs(current_tag)
            safe_print(f"✅ 已更新 docs/CHANGELOG.md（标题版本已同步为 {normalized_version}）")
    else:
        safe_print("ℹ️ 已跳过写回 docs/CHANGELOG.md。")


if __name__ == "__main__":
    main()
