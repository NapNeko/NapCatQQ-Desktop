# -*- coding: utf-8 -*-
"""探测 Desktop 自带的 SnowLuma.Framework lite tarball (W3).

由 :mod:`script.build_scripts.build_snowluma_framework_lite` 在打包期生成
``src/resource/runtime/snowluma_framework_lite.tar.gz`` + sibling
``snowluma_framework_lite.version.txt``;
:mod:`script.build_scripts.snowluma_framework_assets` 把它们打进 PyInstaller datas.

运行时通过 :func:`find_bundled_lite_tarball` 与 :func:`read_bundled_version`
透明探测 (开发态从仓库根, 打包态从 ``base_path / resource/runtime/``).

W4 ``RemoteDeployment`` 会调 :func:`find_bundled_lite_tarball` 把它 SFTP 上传到
``$WORKSPACE_DIR/snowluma_framework_lite.tar.gz``, 然后 ``install_snowluma.sh``
``tar -xzf --strip-components=1`` 解压进 ``$SNOWLUMA_DIR``.
"""

from __future__ import annotations

import sys
from pathlib import Path

from src.core.platform.app_paths import resolve_app_base_path


# 文件名 (与构建脚本约定一致)
LITE_TARBALL_FILENAME: str = "snowluma_framework_lite.tar.gz"
LITE_VERSION_FILENAME: str = "snowluma_framework_lite.version.txt"


def _candidate_dirs() -> list[Path]:
    """返回 lite tarball 可能的容身目录 (按优先级).

    冻结态 (PyInstaller 打包后):
        - ``base_path / resource / runtime`` (PyInstaller datas dest=``resource/runtime``)
        - ``sys._MEIPASS / resource / runtime`` (one-file 模式 fallback)

    源码态:
        - 仓库根 ``src/resource/runtime`` (与 ``.qrc`` 资源同位置)
    """
    dirs: list[Path] = []
    base = resolve_app_base_path()

    if getattr(sys, "frozen", False):
        dirs.append(base / "resource" / "runtime")
        meipass = getattr(sys, "_MEIPASS", None)
        if meipass:
            dirs.append(Path(meipass) / "resource" / "runtime")
    else:
        dirs.append(base / "src" / "resource" / "runtime")

    return dirs


def find_bundled_lite_tarball() -> Path | None:
    """返回 Desktop 自带的 lite tarball 绝对路径; 未捆绑时返 ``None``.

    Returns:
        ``snowluma_framework_lite.tar.gz`` 的 :class:`Path`; 不存在返 None.
        调用方应优雅降级 (例如 UI 提示 "未内置 SnowLuma 资源, 远端部署不可用").

    Examples:
        >>> tarball = find_bundled_lite_tarball()
        >>> if tarball is None:
        ...     raise RuntimeError("Desktop 未捆绑 SnowLuma.Framework lite tarball")
    """
    for d in _candidate_dirs():
        candidate = d / LITE_TARBALL_FILENAME
        if candidate.is_file():
            return candidate
    return None


def find_bundled_version_sidecar() -> Path | None:
    """返回 sibling ``.version.txt`` 路径; 未捆绑时返 ``None``."""
    for d in _candidate_dirs():
        candidate = d / LITE_VERSION_FILENAME
        if candidate.is_file():
            return candidate
    return None


def read_bundled_version() -> str | None:
    """读取捆绑的 SnowLuma.Framework 版本号; 未捆绑或读取失败返 ``None``.

    Returns:
        版本字符串 (例如 ``"0.1.0"``, 与 ``package.json:version`` 同源);
        sibling ``.version.txt`` 缺失或为空返 ``None``.

    用途:
        :class:`VersionService` 暴露此值供 UI 显示 "Desktop 内置 SnowLuma.Framework
        版本 vs 远端已部署版本" 的对比, 提示用户是否需要重新部署.
    """
    sidecar = find_bundled_version_sidecar()
    if sidecar is None:
        return None
    try:
        text = sidecar.read_text(encoding="utf-8").strip()
        return text or None
    except (OSError, UnicodeDecodeError):
        return None
