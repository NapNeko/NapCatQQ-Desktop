#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
一键发布脚本 - 整合版本更新、AI 生成日志、自动打 tag

功能：
1. 更新版本号（pyproject.toml、__init__.py）
2. AI 生成更新日志
3. 自动提交、打 tag
4. 可选推送到远程

使用方法：
    cd script/utils && python release.py <version>
    
示例：
    python release.py 1.7.28
    python release.py 1.7.28 --push    # 自动推送
    
工作流程：
    1. 更新版本号 → 2. AI 生成日志 → 3. 提交 → 4. 打 tag → 5. 推送（可选）
"""

import os
import re
import subprocess
import sys
from pathlib import Path

import httpx


# =============================================================================
# 配置
# =============================================================================

DEFAULT_CONFIG = {
    "api_url": "https://openrouter.ai/api/v1/chat/completions",
    "model": "z-ai/glm-4.5-air:free",
    "max_tokens": 5000,
    "temperature": 0.2,
}

SYSTEM_PROMPT = """# NapCatQQ Desktop 发布说明生成器

请根据提供的 commit 列表生成标准格式的发布说明。

## 核心规则

1. **版本号**：第一行必须是 `# {VERSION}`
2. **语言**：全部使用简体中文
3. **格式**：严格按照下方模板输出

## Commit 分类

- 🐛 **修复**：bug fix、修复、fix 相关
- ✨ **新增**：新功能、feat、add 相关  
- 🔧 **优化**：优化、重构、refactor、improve、perf 相关
- 📦 **依赖**：deps、依赖更新（可忽略）
- 🔨 **构建**：ci、build、workflow 相关（可忽略）

## 合并和筛选

- 合并相似项：同一功能的多个 commit 合并为一条
- 忽略琐碎项：合并冲突、格式化、typo 等
- 控制数量：最终保持 5-15 条更新要点
- 保留 commit hash：每条末尾附上短 hash，格式 `(a1b2c3d)`

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
```

## 重要约束

1. 如果某个分类没有内容，则完全省略该分类
2. 不要编造不存在的更新
3. 保持简洁，每条更新控制在一行内
4. 使用用户友好的语言
5. 重大变更需要加粗提示
"""


# =============================================================================
# 工具函数
# =============================================================================

def load_env_file():
    """加载 .env 文件"""
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


def get_config():
    """读取配置"""
    config = DEFAULT_CONFIG.copy()
    load_env_file()
    
    api_key = os.environ.get("OPENAI_API_KEY") or os.environ.get("OPENROUTER_API_KEY")
    if not api_key:
        print("❌ 错误：未配置 API Key")
        print(f"\n请创建 {Path(__file__).parent / '.env'} 文件：")
        print('  OPENAI_API_KEY="sk-or-v1-..."')
        sys.exit(1)
    
    config["api_key"] = api_key
    if "OPENAI_API_URL" in os.environ:
        config["api_url"] = os.environ["OPENAI_API_URL"]
    if "OPENAI_MODEL" in os.environ:
        config["model"] = os.environ["OPENAI_MODEL"]
    
    return config


def run_command(command: str, check: bool = True) -> str:
    """运行 shell 命令"""
    try:
        result = subprocess.run(
            command,
            shell=True,
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
            check=check,
        )
        return (result.stdout or "").strip()
    except subprocess.CalledProcessError as e:
        print(f"❌ 命令失败: {command}")
        print(f"错误: {e.stderr}")
        if check:
            sys.exit(1)
        return ""


# =============================================================================
# 版本更新
# =============================================================================

def update_pyproject_version(version: str) -> None:
    """更新 pyproject.toml"""
    file_path = Path("pyproject.toml")
    if not file_path.exists():
        print(f"❌ 找不到 {file_path}")
        sys.exit(1)
    
    content = file_path.read_text(encoding="utf-8")
    new_content = re.sub(
        r'(\[project\][\s\S]*?^version\s*=\s*)"[^"]+"',
        rf'\1"{version}"',
        content,
        flags=re.MULTILINE,
    )
    
    if new_content != content:
        file_path.write_text(new_content, encoding="utf-8")
        print(f"  ✓ {file_path}")
    else:
        print(f"  ⚠ {file_path} 未修改")


def update_init_version(version: str) -> None:
    """更新 __init__.py"""
    file_path = Path("src/core/config/__init__.py")
    if not file_path.exists():
        return
    
    content = file_path.read_text(encoding="utf-8")
    new_content = re.sub(r'(__version__\s*=\s*)"[^"]+"', rf'\1"v{version}"', content)
    
    if new_content != content:
        file_path.write_text(new_content, encoding="utf-8")
        print(f"  ✓ {file_path}")


# =============================================================================
# AI 生成日志
# =============================================================================

def get_previous_tag(current_tag: str):
    """获取上一个 tag"""
    try:
        tags = run_command("git tag -l --sort=-v:refname")
        tag_list = [t for t in tags.split("\n") if t.strip()]
        for i, tag in enumerate(tag_list):
            if tag == current_tag and i + 1 < len(tag_list):
                return tag_list[i + 1]
        return None
    except Exception:
        return None


def get_commits_between(from_tag, to_tag: str) -> list:
    """获取 commit 列表"""
    if from_tag:
        commit_range = f"{from_tag}..{to_tag}"
    else:
        commit_range = to_tag
    
    commits = run_command(f"git log {commit_range} --pretty=format:'%s (%h)'")
    return [c for c in commits.split("\n") if c.strip()]


def generate_changelog_with_ai(config, current_tag: str, previous_tag, commits: list) -> str:
    """调用 AI"""
    
    user_content = f"""当前版本: {current_tag}
上一版本: {previous_tag or "(首次发布)"}

## 提交列表
{chr(10).join(f"- {c}" for c in commits)}
"""
    
    payload = {
        "model": config["model"],
        "messages": [
            {"role": "system", "content": SYSTEM_PROMPT},
            {"role": "user", "content": user_content},
        ],
        "temperature": config["temperature"],
        "max_tokens": config["max_tokens"],
    }
    
    headers = {
        "Authorization": f"Bearer {config['api_key']}",
        "Content-Type": "application/json",
    }
    
    print(f"\n  🤖 AI 生成中 ({config['model']})...")
    
    try:
        response = httpx.post(
            config["api_url"],
            headers=headers,
            json=payload,
            timeout=120,
        )
        response.raise_for_status()
        result = response.json()
        
        if "choices" in result and len(result["choices"]) > 0:
            content = result["choices"][0]["message"]["content"]
            content = content.replace("{VERSION}", current_tag)
            return content
    except Exception as e:
        print(f"  ⚠ AI 失败: {e}")
    
    # 降级处理
    return generate_fallback_changelog(current_tag, commits)


def generate_fallback_changelog(current_tag: str, commits: list) -> str:
    """规则生成"""
    lines = [f"# {current_tag}", ""]
    
    fixes, features, others = [], [], []
    for commit in commits:
        cl = commit.lower()
        if any(k in cl for k in ["fix", "修复"]):
            fixes.append(commit)
        elif any(k in cl for k in ["feat", "新增", "add"]):
            features.append(commit)
        else:
            others.append(commit)
    
    if features:
        lines.extend(["## ✨ 新增"] + [f"{i+1}. {c}" for i, c in enumerate(features[:10])] + [""])
    if fixes:
        lines.extend(["## 🐛 修复"] + [f"{i+1}. {c}" for i, c in enumerate(fixes[:10])] + [""])
    if others:
        lines.extend(["## 🔧 其他"] + [f"{i+1}. {c}" for i, c in enumerate(others[:5])] + [""])
    
    return "\n".join(lines)


def update_changelog_file(version: str, changelog_content: str) -> None:
    """更新 docs/CHANGELOG.md"""
    file_path = Path("docs/CHANGELOG.md")
    if not file_path.exists():
        return
    
    content = file_path.read_text(encoding="utf-8")
    
    # 更新版本号标题
    content = re.sub(
        r"(# 🚀 v)\d+\.\d+\.\d+( - 累积更新)",
        rf"\g<1>{version}\g<2>",
        content,
    )
    
    # 提取 Tips
    tips_match = re.search(r"(## Tips\n(?:[^#\n][^\n]*\n)*)", content, re.MULTILINE)
    tips = tips_match.group(1) if tips_match else "## Tips\n\n"
    
    # 替换内容
    pattern = r"(# 🚀 v\d+\.\d+\.\d+ - 累积更新\s*\n\s*\n)((?:(?!## 📝 使用须知)[\s\S])*?)(\n## 📝 使用须知)"
    replacement = rf"\g<1>{tips}{changelog_content}\n\g<3>"
    new_content = re.sub(pattern, replacement, content, flags=re.DOTALL)
    
    if new_content != content:
        file_path.write_text(new_content, encoding="utf-8")
        print(f"  ✓ {file_path}")


# =============================================================================
# Git 操作
# =============================================================================

def commit_changes(version: str) -> bool:
    """提交"""
    status = run_command("git status --porcelain", check=False)
    if not status:
        print("  ⚠ 无更改需要提交")
        return False
    
    run_command("git add pyproject.toml src/core/config/__init__.py docs/CHANGELOG.md")
    run_command(f'git commit -m "chore: release v{version}"')
    print(f"  ✓ 已提交")
    return True


def create_tag(version: str) -> None:
    """打 tag"""
    tag = f"v{version}"
    
    existing = run_command(f"git tag -l {tag}", check=False)
    if existing:
        print(f"\n  ⚠ Tag {tag} 已存在")
        if input("  覆盖? (y/n): ").lower() != "y":
            sys.exit(1)
        run_command(f"git tag -d {tag}")
    
    run_command(f"git tag {tag}")
    print(f"  ✓ 已创建 tag: {tag}")


def push_changes(version: str, auto_push: bool) -> None:
    """推送"""
    if not auto_push:
        if input("\n  推送到远程? (y/n): ").lower() != "y":
            print("  ⏭ 跳过推送")
            return
    
    print("\n  🚀 推送中...")
    run_command("git push origin main")
    run_command(f"git push origin v{version}")
    print("  ✓ 推送完成")


# =============================================================================
# 主流程
# =============================================================================

def main():
    if len(sys.argv) < 2:
        print("用法: python release.py <version> [--push]")
        print("示例:")
        print("  python release.py 1.7.28")
        print("  python release.py 1.7.28 --push")
        sys.exit(1)
    
    version = sys.argv[1].lstrip("v")
    auto_push = "--push" in sys.argv
    
    print("=" * 60)
    print(f"🚀 发布流程: v{version}")
    print("=" * 60)
    
    # 检查是否在 git 仓库
    if not Path(".git").exists():
        print("❌ 请在 git 仓库根目录运行")
        sys.exit(1)
    
    # 1. 配置
    print("\n📋 检查配置...")
    config = get_config()
    
    # 2. 更新版本号
    print("\n📦 更新版本号...")
    update_pyproject_version(version)
    update_init_version(version)
    
    # 3. 生成日志
    print("\n📝 生成更新日志...")
    current_tag = f"v{version}"
    previous_tag = get_previous_tag(current_tag)
    
    if previous_tag:
        print(f"  对比: {previous_tag} -> {current_tag}")
    
    commits = get_commits_between(previous_tag, current_tag)
    print(f"  找到 {len(commits)} 个 commit")
    
    if not commits:
        print("❌ 没有 commit，退出")
        sys.exit(1)
    
    changelog = generate_changelog_with_ai(config, current_tag, previous_tag, commits)
    
    # 显示结果
    print("\n" + "-" * 60)
    print(changelog)
    print("-" * 60)
    
    # 4. 更新文件
    update_changelog_file(version, changelog)
    
    # 5. 提交
    print("\n📤 提交更改...")
    committed = commit_changes(version)
    if not committed:
        if input("  继续打 tag? (y/n): ").lower() != "y":
            sys.exit(0)
    
    # 6. 打 tag
    print("\n🏷️ 创建 tag...")
    create_tag(version)
    
    # 7. 推送
    push_changes(version, auto_push)
    
    # 完成
    print("\n" + "=" * 60)
    print(f"✅ 发布完成: v{version}")
    print("=" * 60)
    print("\nGitHub Actions 将自动构建并发布 MSI")


if __name__ == "__main__":
    main()
