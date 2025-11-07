#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
测试版本更新脚本的功能

验证：
1. 版本号提取和验证
2. 文件更新功能
3. Commit 分类功能
"""

# 标准库导入
import sys
from pathlib import Path

# 添加项目根目录到路径
sys.path.insert(0, str(Path(__file__).parent.parent.parent))

# 项目内模块导入
from script.utils.update_version import (
    categorize_commits,
    generate_changelog_content,
    get_version_from_tag,
)


def test_version_extraction():
    """测试版本号提取"""
    print("测试版本号提取...")

    # 测试带 v 前缀的 tag
    assert get_version_from_tag("v1.7.9") == "1.7.9"
    assert get_version_from_tag("v2.0.0") == "2.0.0"

    # 测试不带 v 前缀的 tag
    assert get_version_from_tag("1.7.9") == "1.7.9"
    assert get_version_from_tag("2.0.0") == "2.0.0"

    print("✅ 版本号提取测试通过")


def test_commit_categorization():
    """测试 Commit 分类"""
    print("\n测试 Commit 分类...")

    commits = [
        "feat: 添加用户管理功能",
        "fix: 修复登录失败问题",
        "perf: 优化启动速度",
        "refactor: 重构用户模块",
        "✨ 添加主题切换功能",
        "🐛 修复内存泄漏",
        "⚡ 提升性能",
        "♻️ 代码重构",
        "docs: 更新文档",  # 应该被忽略
        "chore: 更新依赖",  # 应该被忽略
    ]

    categories = categorize_commits(commits)

    # 验证分类结果
    assert len(categories["feat"]) == 2, f"Expected 2 feat commits, got {len(categories['feat'])}"
    assert len(categories["fix"]) == 2, f"Expected 2 fix commits, got {len(categories['fix'])}"
    assert len(categories["perf"]) == 4, f"Expected 4 perf commits, got {len(categories['perf'])}"

    # 验证内容
    assert "添加用户管理功能" in categories["feat"]
    assert "添加主题切换功能" in categories["feat"]
    assert "修复登录失败问题" in categories["fix"]
    assert "修复内存泄漏" in categories["fix"]
    assert "优化启动速度" in categories["perf"]

    print("✅ Commit 分类测试通过")


def test_changelog_generation():
    """测试更新日志生成"""
    print("\n测试更新日志生成...")

    categories = {
        "feat": ["添加用户管理功能", "添加主题切换"],
        "fix": ["修复登录问题"],
        "perf": ["优化性能"],
    }

    changelog = generate_changelog_content(categories)

    # 验证生成的内容包含所有分类
    assert "## ✌️ 新增功能" in changelog
    assert "## 😭 修复功能" in changelog
    assert "## 😘 优化功能" in changelog

    # 验证内容
    assert "添加用户管理功能" in changelog
    assert "修复登录问题" in changelog
    assert "优化性能" in changelog

    print("✅ 更新日志生成测试通过")


def test_empty_changelog():
    """测试空更新日志"""
    print("\n测试空更新日志...")

    categories = {
        "feat": [],
        "fix": [],
        "perf": [],
    }

    changelog = generate_changelog_content(categories)

    # 应该生成默认内容
    assert "累积更新" in changelog or "Bug修复" in changelog

    print("✅ 空更新日志测试通过")


def main():
    """运行所有测试"""
    print("=" * 60)
    print("版本更新脚本功能测试")
    print("=" * 60)

    try:
        test_version_extraction()
        test_commit_categorization()
        test_changelog_generation()
        test_empty_changelog()

        print("\n" + "=" * 60)
        print("✅ 所有测试通过！")
        print("=" * 60)
        return 0
    except AssertionError as e:
        print(f"\n❌ 测试失败: {e}")
        return 1
    except Exception as e:
        print(f"\n❌ 测试出错: {e}")
        # 标准库导入
        import traceback

        traceback.print_exc()
        return 1


if __name__ == "__main__":
    sys.exit(main())
