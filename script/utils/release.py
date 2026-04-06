#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""本地唯一发布入口：同步版本文件、执行 uv lock、提交并打 tag。"""

from __future__ import annotations

# 标准库导入
import argparse
import sys
from pathlib import Path

PROJECT_ROOT = Path(__file__).resolve().parents[2]
if str(PROJECT_ROOT) not in sys.path:
    sys.path.insert(0, str(PROJECT_ROOT))

# 项目内模块导入
from script.utils.generate_changelog_ai import (
    ReviewCancelled,
    generate_changelog_with_ai,
    get_config,
    interactive_review_loop,
)
from script.utils.release_helpers import (
    ReleaseError,
    collect_commits,
    collect_diff_stats,
    is_release_metadata_commit,
    parse_version,
    perform_release,
    resolve_previous_tag,
)


def _safe_print(text: object = "") -> None:
    message = str(text)
    try:
        print(message)
    except UnicodeEncodeError:
        encoding = sys.stdout.encoding or "utf-8"
        sys.stdout.flush()
        sys.stdout.buffer.write((message + "\n").encode(encoding, errors="replace"))
        sys.stdout.flush()


def _generate_ai_release_notes(version: str, from_tag: str | None) -> str:
    version_info = parse_version(version)
    previous_tag = resolve_previous_tag(version_info, from_tag=from_tag)
    commits = collect_commits(previous_tag, to_ref="HEAD")
    included_commits = [commit for commit in commits if not is_release_metadata_commit(commit.subject)]
    commit_lines = [f"{commit.subject} ({commit.sha[:7]})" for commit in included_commits]
    file_stats, file_list = collect_diff_stats(previous_tag, to_ref="HEAD")
    config = get_config(exit_on_error=False)

    _safe_print("[INFO] 正在进入 AI 发布说明交互流程...")
    _safe_print(f"[INFO] 起始版本: {previous_tag or '无'}")
    _safe_print(f"[INFO] 纳入 AI 的 commit 数: {len(included_commits)}")

    notes, messages = generate_changelog_with_ai(
        config,
        version_info.tag,
        previous_tag,
        commit_lines,
        file_stats,
        file_list,
    )
    return interactive_review_loop(config, messages, notes)


def main() -> int:
    parser = argparse.ArgumentParser(
        description="本地发布脚本：先进入 AI 交互改稿，再同步版本文件、执行 uv lock、提交并创建 tag",
    )
    parser.add_argument("version", help="目标版本号，例如 2.0.18 或 v2.0.18")
    parser.add_argument("--from", dest="from_tag", help="手动指定起始 tag")
    parser.add_argument("--push", action="store_true", help="创建 tag 后推送分支和 tag")
    args = parser.parse_args()

    try:
        auto_notes_override = _generate_ai_release_notes(args.version, args.from_tag)
        result = perform_release(
            args.version,
            from_tag=args.from_tag,
            push=args.push,
            auto_notes_override=auto_notes_override,
        )
    except ReviewCancelled:
        _safe_print("[ERR] 已取消 AI 发布说明确认，发布流程未继续执行。")
        return 1
    except RuntimeError as exc:
        _safe_print(f"[ERR] {exc}")
        return 1
    except ReleaseError as exc:
        print(f"[ERR] {exc}")
        return 1

    print(f"[OK] 发布完成: {result.sync.version.tag}")
    print(f"[INFO] 起始版本: {result.sync.previous_tag or '无'}")
    print(f"[INFO] 原始 commit 数: {len(result.sync.commits)}")
    print(f"[INFO] 纳入日志的 commit 数: {len(result.sync.included_commits)}")
    print(f"[INFO] release commit: {result.commit_sha}")
    print("[INFO] 自动生成的发布说明：")
    print("-" * 60)
    _safe_print(result.sync.auto_notes)
    print("-" * 60)
    if result.pushed:
        print("[OK] 已推送分支和 tag")
    return 0


if __name__ == "__main__":
    sys.exit(main())
