# -*- coding: utf-8 -*-
"""把 SnowLuma.Framework lite tarball 注入 PyInstaller datas (W3).

由 :file:`main.spec` 调用; 返回值会与 :func:`prepare_runtime_assets` 输出合并.

- 源文件: ``src/resource/runtime/snowluma_framework_lite.tar.gz`` +
  sibling ``snowluma_framework_lite.version.txt``
- dist 落点: ``resource/runtime/`` (与 ``base_path / resource/runtime/`` 对齐,
  让 :func:`src.core.remote.snowluma.bundled.find_bundled_lite_tarball` 在打包态
  能直接命中).

设计取舍 (与 :mod:`runtime_assets` 同模式):

- 文件不存在时返回空 list (不阻塞 build), 仅打印 WARN; 适用于:
  - 本地开发态没构建过 lite tarball
  - CI 没拉到 SnowLuma source
  - 测试环境 (e.g. unit test runner)
- W4 ``RemoteDeployment`` 在调用 :func:`find_bundled_lite_tarball` 时若返回
  ``None``, 会向 UI 抛 :class:`SnowLumaFrameworkNotBundledError`, 引导用户
  手动构建.
"""

from __future__ import annotations

from pathlib import Path

# 文件名常量与 src/core/remote/snowluma/bundled.py 保持同步; 不引入 src 包以避免
# spec 文件需要把 src 加入 sys.path (build 时 src 还没成为可导入包).
_LITE_TARBALL_FILENAME: str = "snowluma_framework_lite.tar.gz"
_LITE_VERSION_FILENAME: str = "snowluma_framework_lite.version.txt"
_BUNDLE_TARGET_DIR: str = "resource/runtime"


def prepare_snowluma_framework_assets(source_root: Path) -> list[tuple[str, str]]:
    """返回 PyInstaller datas 条目; lite tarball + version sidecar 各一项.

    Args:
        source_root: 仓库根 (与 :func:`prepare_runtime_assets` 同入参).

    Returns:
        ``[(src_path, dest_dir), ...]`` 列表; 文件不存在则对应条目省略.
        若两个文件都不存在则返回空列表 + WARN 输出.
    """
    runtime_resource_dir = source_root / "src" / "resource" / "runtime"
    tarball = runtime_resource_dir / _LITE_TARBALL_FILENAME
    version = runtime_resource_dir / _LITE_VERSION_FILENAME

    entries: list[tuple[str, str]] = []
    if tarball.is_file():
        entries.append((str(tarball), _BUNDLE_TARGET_DIR))
    else:
        print(
            f"[snowluma_framework_assets] WARN: lite tarball 不存在: {tarball}; "
            f"远端部署能力将不可用 (运行 script/build_scripts/build_snowluma_framework_lite.py 生成)"
        )

    if version.is_file():
        entries.append((str(version), _BUNDLE_TARGET_DIR))
    elif tarball.is_file():
        # 有 tarball 但没 sidecar 算异常 (构建脚本应同步生成); 给硬警告
        print(
            f"[snowluma_framework_assets] WARN: 有 tarball 但缺 version sidecar: {version}"
        )

    return entries
