#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
版本号自动更新和更新日志生成脚本

功能：
1. 从 git tag 提取版本号
2. 自动更新 pyproject.toml、src/core/config/__init__.py、docs/CHANGELOG.md 中的版本号
3. 从 git commit 记录自动生成更新日志
4. 保持 CHANGELOG.md 的固定结构和内容

使用方法：
    python script/utils/update_version.py <version_tag>

示例：
    python script/utils/update_version.py v1.7.9
    python script/utils/update_version.py 1.7.9  # 自动添加 v 前缀
"""

# 标准库导入
import re
import subprocess
import sys
from pathlib import Path
from typing import Dict, List, Optional


def run_git_command(command: str) -> str:
    """运行 git 命令并返回输出"""
    try:
        result = subprocess.run(
            command,
            shell=True,
            capture_output=True,
            text=True,
            check=True,
        )
        return result.stdout.strip()
    except subprocess.CalledProcessError as e:
        print(f"❌ Git 命令执行失败: {command}")
        print(f"错误: {e.stderr}")
        sys.exit(1)


def get_version_from_tag(tag: str) -> str:
    """从 tag 提取版本号（移除 v 前缀）"""
    # 移除 v 前缀（如果存在）
    version = tag.lstrip("v")

    # 验证版本号格式
    if not re.match(r"^\d+\.\d+\.\d+$", version):
        print(f"❌ 无效的版本号格式: {tag}")
        print("版本号应为 x.y.z 格式（例如：1.7.9 或 v1.7.9）")
        sys.exit(1)

    return version


def get_previous_tag() -> Optional[str]:
    """获取上一个版本 tag"""
    try:
        # 获取所有 tag 按日期排序
        tags = run_git_command("git tag -l --sort=-creatordate")
        tag_list = [t for t in tags.split("\n") if t.strip()]

        # 返回第二个 tag（如果存在）
        if len(tag_list) > 1:
            return tag_list[1]
        return None
    except Exception:
        return None


def get_commits_between_tags(from_tag: Optional[str], to_ref: str = "HEAD") -> List[str]:
    """获取两个 tag 之间的所有 commit 消息"""
    if from_tag:
        # 获取两个 tag 之间的 commit
        commit_range = f"{from_tag}..{to_ref}"
    else:
        # 获取所有 commit
        commit_range = to_ref

    commits = run_git_command(f"git log {commit_range} --pretty=format:%s")
    return [c for c in commits.split("\n") if c.strip()]


def categorize_commits(commits: List[str]) -> Dict[str, List[str]]:
    """将 commit 消息分类"""
    categories = {
        "feat": [],  # 新增功能
        "fix": [],  # 修复功能
        "perf": [],  # 优化功能
    }

    for commit in commits:
        commit = commit.strip()
        if not commit:
            continue

        # 匹配 conventional commit 格式
        if commit.startswith(("feat:", "✨")):
            # 移除前缀
            msg = re.sub(r"^(feat:|✨)\s*", "", commit)
            categories["feat"].append(msg)
        elif commit.startswith(("fix:", "🐛")):
            msg = re.sub(r"^(fix:|🐛)\s*", "", commit)
            categories["fix"].append(msg)
        elif commit.startswith(("perf:", "refactor:", "⚡", "♻️")):
            msg = re.sub(r"^(perf:|refactor:|⚡|♻️)\s*", "", commit)
            categories["perf"].append(msg)

    return categories


def generate_changelog_content(categories: Dict[str, List[str]]) -> str:
    """生成更新日志内容"""
    sections = []

    # 新增功能
    if categories["feat"]:
        sections.append("## ✌️ 新增功能")
        for item in categories["feat"]:
            sections.append(f" - {item}")
        sections.append("")

    # 修复功能
    if categories["fix"]:
        sections.append("## 😭 修复功能")
        for item in categories["fix"]:
            sections.append(f" - {item}")
        sections.append("")

    # 优化功能
    if categories["perf"]:
        sections.append("## 😘 优化功能")
        for item in categories["perf"]:
            sections.append(f" - {item}")
        sections.append("")

    # 如果没有任何分类的 commit，添加默认消息
    if not any(categories.values()):
        sections.append("## ✌️ 新增功能")
        sections.append(" - 累积更新")
        sections.append("")
        sections.append("## 😭 修复功能")
        sections.append(" - Bug修复和性能优化")
        sections.append("")

    return "\n".join(sections)


def update_pyproject_version(version: str) -> None:
    """更新 pyproject.toml 中的版本号"""
    file_path = Path("pyproject.toml")

    if not file_path.exists():
        print(f"❌ 文件不存在: {file_path}")
        sys.exit(1)

    content = file_path.read_text(encoding="utf-8")

    # 替换版本号 - 更具体的模式，匹配 [project] 段落中的 version
    new_content = re.sub(
        r'(\[project\][\s\S]*?^version\s*=\s*)"[^"]+"',
        rf'\1"{version}"',
        content,
        flags=re.MULTILINE,
    )

    if new_content == content:
        print(f"⚠️  未找到版本号模式: {file_path}")
    else:
        file_path.write_text(new_content, encoding="utf-8")
        print(f"✅ 已更新 {file_path}")


def update_init_version(version: str) -> None:
    """更新 src/core/config/__init__.py 中的版本号"""
    file_path = Path("src/core/config/__init__.py")

    if not file_path.exists():
        print(f"❌ 文件不存在: {file_path}")
        sys.exit(1)

    content = file_path.read_text(encoding="utf-8")

    # 替换版本号（带 v 前缀）
    new_content = re.sub(r'(__version__\s*=\s*)"[^"]+"', rf'\1"v{version}"', content)

    if new_content == content:
        print(f"⚠️  未找到版本号模式: {file_path}")
    else:
        file_path.write_text(new_content, encoding="utf-8")
        print(f"✅ 已更新 {file_path}")


def update_changelog(version: str, changelog_content: str) -> None:
    """更新 docs/CHANGELOG.md"""
    file_path = Path("docs/CHANGELOG.md")

    if not file_path.exists():
        print(f"❌ 文件不存在: {file_path}")
        sys.exit(1)

    content = file_path.read_text(encoding="utf-8")

    # 更新版本号标题
    content = re.sub(r"(# 🚀 v)\d+\.\d+\.\d+( - 累积更新)", r"\g<1>" + version + r"\g<2>", content)

    # 提取现有的 Tips 部分（如果存在）
    # 使用非贪婪匹配且限制匹配范围，避免 ReDoS 攻击
    # 匹配从 "## Tips" 到下一个 "##" 开头的行
    tips_match = re.search(r"(## Tips\n(?:[^#\n][^\n]*\n)*)", content, re.MULTILINE)
    tips_section = (
        tips_match.group(1)
        if tips_match
        else """## Tips
建议旧版本 1.6.17 及以下, 选择手动更新喔(不然真的可能出现奇怪的问题)

"""
    )

    # 查找并替换功能分类部分
    # 从版本标题后到 "## 📝 使用须知" 之间的部分
    # 使用非贪婪匹配，明确限定匹配范围，避免 ReDoS 攻击
    pattern = r"(# 🚀 v\d+\.\d+\.\d+ - 累积更新\s*\n\s*\n)((?:(?!## 📝 使用须知)[\s\S])*?)(\n## 📝 使用须知)"

    # 构建新的内容（版本标题 + 空行 + Tips部分 + 功能分类 + 空行）
    replacement = r"\g<1>" + tips_section + changelog_content + r"\n\g<3>"

    new_content = re.sub(pattern, replacement, content, flags=re.DOTALL)

    if new_content == content:
        print(f"⚠️  CHANGELOG 未更新（可能是格式问题）")
    else:
        file_path.write_text(new_content, encoding="utf-8")
        print(f"✅ 已更新 {file_path}")


def main():
    """主函数"""
    if len(sys.argv) < 2:
        print("用法: python script/utils/update_version.py <version_tag>")
        print("示例: python script/utils/update_version.py v1.7.9")
        sys.exit(1)

    # 获取版本号
    tag = sys.argv[1]
    version = get_version_from_tag(tag)
    version_with_v = f"v{version}"

    print(f"🚀 开始更新版本到 {version_with_v}...")
    print()

    # 获取上一个版本 tag
    prev_tag = get_previous_tag()
    if prev_tag:
        print(f"📌 上一个版本: {prev_tag}")
    else:
        print("📌 未找到上一个版本，将使用所有历史 commit")
    print()

    # 获取 commit 记录
    print("📝 收集 commit 记录...")
    commits = get_commits_between_tags(prev_tag)
    print(f"   找到 {len(commits)} 个 commit")
    print()

    # 分类 commit
    print("🔖 分类 commit...")
    categories = categorize_commits(commits)
    print(f"   新增功能: {len(categories['feat'])} 个")
    print(f"   修复功能: {len(categories['fix'])} 个")
    print(f"   优化功能: {len(categories['perf'])} 个")
    print()

    # 生成更新日志内容
    changelog_content = generate_changelog_content(categories)

    # 更新文件
    print("📝 更新版本号...")
    update_pyproject_version(version)
    update_init_version(version)
    update_changelog(version, changelog_content)
    print()

    print(f"✅ 版本更新完成: {version_with_v}")
    print()
    print("📋 更新日志预览：")
    print("-" * 60)
    print(changelog_content)
    print("-" * 60)


if __name__ == "__main__":
    main()
