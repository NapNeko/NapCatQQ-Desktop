# -*- coding: utf-8 -*-
"""构建时准备目录版运行时资源。"""

# 标准库导入
import shutil
from pathlib import Path


WINDOWS_NATIVE_PATTERNS = (
    "*.linux.*",
    "*.darwin.*",
    "linux.*",
    "darwin.*",
    "win32-arm64",
)


def prepare_runtime_assets(source_root: Path, build_root: Path) -> list[tuple[str, str]]:
    """复制并裁剪运行时目录，返回 PyInstaller datas 条目。"""

    target_root = build_root / "runtime_assets"
    if target_root.exists():
        shutil.rmtree(target_root)

    runtime_src = source_root / "runtime"
    runtime_dst = target_root / "runtime"
    shutil.copytree(runtime_src, runtime_dst)
    prune_runtime_assets(runtime_dst)
    return [(str(runtime_dst), "runtime")]


def prune_runtime_assets(runtime_root: Path) -> None:
    """移除非 Windows 运行时文件。"""

    native_root = runtime_root / "NapCatQQ" / "native"
    if not native_root.exists():
        return

    for pattern in WINDOWS_NATIVE_PATTERNS:
        for path in native_root.rglob(pattern):
            if path.is_dir():
                shutil.rmtree(path, ignore_errors=True)
            else:
                path.unlink(missing_ok=True)
