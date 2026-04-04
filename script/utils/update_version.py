#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""同步版本文件和发布说明，不创建 commit 或 tag。"""

from __future__ import annotations

# 标准库导入
import argparse
import sys
from pathlib import Path

PROJECT_ROOT = Path(__file__).resolve().parents[2]
if str(PROJECT_ROOT) not in sys.path:
    sys.path.insert(0, str(PROJECT_ROOT))

# 项目内模块导入
from script.utils.release_helpers import ReleaseError, sync_release_metadata


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
        description="同步版本文件、CHANGELOG 和 uv.lock（辅助脚本，不负责发布）",
    )
    parser.add_argument("version", help="目标版本号，例如 2.0.18 或 v2.0.18")
    parser.add_argument("--from", dest="from_tag", help="手动指定起始 tag")
    parser.add_argument(
        "--no-lock",
        action="store_true",
        help="仅调试时使用：跳过 uv lock",
    )
    args = parser.parse_args()

    try:
        result = sync_release_metadata(
            args.version,
            from_tag=args.from_tag,
            run_lock=not args.no_lock,
        )
    except ReleaseError as exc:
        print(f"[ERR] {exc}")
        return 1

    print(f"[OK] 已同步版本文件到 {result.version.tag}")
    print(f"[INFO] 起始版本: {result.previous_tag or '无'}")
    print(f"[INFO] 原始 commit 数: {len(result.commits)}")
    print(f"[INFO] 纳入日志的 commit 数: {len(result.included_commits)}")
    print("[INFO] 自动生成的发布说明：")
    print("-" * 60)
    _safe_print(result.auto_notes)
    print("-" * 60)
    return 0


if __name__ == "__main__":
    sys.exit(main())
