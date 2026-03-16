# -*- coding: utf-8 -*-
"""构建时准备目录版运行时资源。"""

# 标准库导入
import os
import shutil
from pathlib import Path


WINDOWS_NATIVE_PATTERNS = (
    "*.linux.*",
    "*.darwin.*",
    "linux.*",
    "darwin.*",
    "win32-arm64",
)


def should_bundle_runtime() -> bool:
    """是否将 runtime 目录打进发布包。

    默认不打包，只有显式设置环境变量时才包含，
    以避免把外部 NapCat 运行时错误塞进桌面端发布包。
    """

    return os.environ.get("NAPCAT_BUNDLE_RUNTIME", "").strip().lower() in {"1", "true", "yes", "on"}


def prepare_runtime_assets(source_root: Path, build_root: Path) -> list[tuple[str, str]]:
    """复制并裁剪运行时目录，返回 PyInstaller datas 条目。"""

    if not should_bundle_runtime():
        print("[build_filters] runtime bundle disabled")
        return []

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
