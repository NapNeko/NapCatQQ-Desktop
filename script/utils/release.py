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
from script.utils.release_helpers import ReleaseError, perform_release


def _safe_print(text: object = "") -> None:
    message = str(text)
    try:
        print(message)
    except UnicodeEncodeError:
        encoding = sys.stdout.encoding or "utf-8"
        sys.stdout.flush()
        sys.stdout.buffer.write((message + "\n").encode(encoding, errors="replace"))
        sys.stdout.flush()


def main() -> int:
    parser = argparse.ArgumentParser(
        description="本地发布脚本：同步版本文件、生成 CHANGELOG、执行 uv lock、提交并创建 tag",
    )
    parser.add_argument("version", help="目标版本号，例如 2.0.18 或 v2.0.18")
    parser.add_argument("--from", dest="from_tag", help="手动指定起始 tag")
    parser.add_argument("--push", action="store_true", help="创建 tag 后推送分支和 tag")
    args = parser.parse_args()

    try:
        result = perform_release(
            args.version,
            from_tag=args.from_tag,
            push=args.push,
        )
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
