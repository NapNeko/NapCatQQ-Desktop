# -*- coding: utf-8 -*-
"""构建 SnowLuma.Framework **lite tarball** (W3).

Usage:
    python script/build_scripts/build_snowluma_framework_lite.py \\
        --source ./example/SnowLuma-main \\
        --out src/resource/runtime/snowluma_framework_lite.tar.gz

入参:

- ``--source``: 已执行过 ``npm run build`` 的 SnowLuma 仓库根 (含 ``dist/`` 与
  ``packages/runtime/native/``).
- ``--out``: 目标 ``.tar.gz`` 路径; sibling ``.version.txt`` 同位置写入.

产物结构 (OQ1 修订, plan §W3 §"实现要点"):

- ``dist/**`` (vite 输出, core + webui 合并)
- ``packages/runtime/launcher.sh``
- ``packages/runtime/package.json``
- ``packages/runtime/native/snowluma-linux-{x64,arm64}.{node,so}``
- ``packages/runtime/native/websocket-linux-{x64,arm64}.node``
- ``packages/runtime/native/ffmpeg/ffmpegAddon.linux.{x64,arm64}.node``
- 仓库根 ``package.json`` (取 version)
- ``LICENSE``

严格排除: win32/darwin native, ``packages/sdk/**``, 所有 ``**/src/**``,
``node_modules/**``, ``**/*.map``, ``packages/webui/dist/**`` (已合并到根 ``dist/``).

运行时: ``find_bundled_snowluma_framework_lite()``
(``src.core.remote.snowluma.bundled``) 在开发态/打包态分别探测;
PyInstaller datas 由 ``script/build_scripts/snowluma_framework_assets.py`` 注入.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
import tarfile
from collections.abc import Iterable
from functools import lru_cache
from pathlib import Path


# ==================== 白名单 / 黑名单 ====================
# 白名单: 路径相对 source 根; 支持 glob (fnmatch).
WHITELIST_GLOBS: tuple[str, ...] = (
    # vite 输出 (core + webui 合并产物)
    "dist/**",
    # runtime manifest + Linux native (Linux x64 / arm64 部署用)
    "packages/runtime/launcher.sh",
    "packages/runtime/package.json",
    "packages/runtime/native/snowluma-linux-x64.node",
    "packages/runtime/native/snowluma-linux-x64.so",
    "packages/runtime/native/snowluma-linux-arm64.node",
    "packages/runtime/native/snowluma-linux-arm64.so",
    "packages/runtime/native/websocket-linux-x64.node",
    "packages/runtime/native/websocket-linux-arm64.node",
    "packages/runtime/native/ffmpeg/ffmpegAddon.linux.x64.node",
    "packages/runtime/native/ffmpeg/ffmpegAddon.linux.arm64.node",
    # 仓库根 manifests
    "package.json",
    "LICENSE",
)

# 黑名单: 即使白名单 glob 命中也排除 (双保险)
BLACKLIST_GLOBS: tuple[str, ...] = (
    "**/node_modules/**",
    "**/.git/**",
    "**/*.map",
    "**/test/**",
    "**/tests/**",
    "**/__tests__/**",
    "**/src/**",
    "packages/sdk/**",
    "packages/webui/dist/**",
    "packages/webui/node_modules/**",
    "packages/runtime/native/snowluma-win32-*",
    "packages/runtime/native/snowluma-darwin-*",
    "packages/runtime/native/*.dll",
    "packages/runtime/native/ffmpeg/ffmpegAddon.win32.*",
    "packages/runtime/native/ffmpeg/ffmpegAddon.darwin.*",
)

# 产物大小约束: 5MB 下限 (太小说明白名单失效); 100MB 上限 (太大说明黑名单失效)
MIN_SIZE_BYTES: int = 5 * 1024 * 1024
MAX_SIZE_BYTES: int = 100 * 1024 * 1024

# tarball 内顶层目录 (远端解压时 ``--strip-components=1`` 剥掉)
ARCHIVE_TOP_LEVEL: str = "snowluma-framework"


# ==================== 路径过滤 ====================
@lru_cache(maxsize=256)
def _glob_to_regex(pattern: str) -> re.Pattern[str]:
    """把 glob (含 ``**``) 翻译为锚定的正则模式.

    语义对齐 :meth:`pathlib.PurePath.full_match` (Python 3.13+); 之所以自己实现是因为
    ``full_match`` 在 Python 3.12 不存在 (项目 ``requires-python = "==3.12.*"``),
    且 :meth:`pathlib.PurePath.match` 不支持 ``**`` 跨目录段匹配.

    规则:

    - ``**`` (单独段): 匹配 0 或多个完整路径段 (含中间分隔符)
    - ``*``: 匹配除 ``/`` 之外的任意字符 (单段内通配)
    - ``?``: 匹配除 ``/`` 之外的单字符
    - 其它字符: 字面 (re.escape)

    Examples:
        >>> _glob_to_regex("dist/**").match("dist/index.mjs") is not None
        True
        >>> _glob_to_regex("**/runtime/native/**").match("packages/runtime/native/x.so") is not None
        True
        >>> _glob_to_regex("**/*.map").match("dist/index.mjs.map") is not None
        True
    """
    # 占位避免 ``**`` 被切成 ``[^/]*[^/]*``
    DS_TOKEN = "__SNOWLUMA_DOUBLESTAR__"
    parts = pattern.split("/")
    regex_segments: list[str] = []
    for part in parts:
        if part == "**":
            regex_segments.append(DS_TOKEN)
            continue
        seg_chars: list[str] = []
        for ch in part:
            if ch == "*":
                seg_chars.append("[^/]*")
            elif ch == "?":
                seg_chars.append("[^/]")
            else:
                seg_chars.append(re.escape(ch))
        regex_segments.append("".join(seg_chars))

    joined = "/".join(regex_segments)
    # 单独 ``**`` 模式
    if joined == DS_TOKEN:
        joined = ".*"
    else:
        # 中间 ``X/**/Y`` → ``X(?:/.*)?/Y`` (含 ``X/Y`` 直连) — 同时合并 ``**`` 前后的 ``/``
        joined = joined.replace(f"/{DS_TOKEN}/", "(?:/.*)?/")
        # 头部 ``**/Y`` → ``(?:.*/)?Y`` (允许 0 段前缀)
        if joined.startswith(f"{DS_TOKEN}/"):
            joined = "(?:.*/)?" + joined[len(f"{DS_TOKEN}/"):]
        # 尾部 ``X/**`` → ``X(?:/.*)?`` (允许 0 段后缀)
        if joined.endswith(f"/{DS_TOKEN}"):
            joined = joined[: -len(f"/{DS_TOKEN}")] + "(?:/.*)?"
    return re.compile("^" + joined + "$")


def _matches_any(rel_path: str, globs: Iterable[str]) -> bool:
    """以 POSIX 风格相对路径检查是否匹配任一 glob.

    支持 ``**`` 跨目录匹配 (与 Python 3.13 ``PurePath.full_match`` 同语义).
    """
    posix = rel_path.replace("\\", "/")
    return any(_glob_to_regex(g).match(posix) is not None for g in globs)


def collect_files(source: Path) -> list[Path]:
    """枚举 source 下符合白名单且不在黑名单的所有 regular file.

    Args:
        source: SnowLuma 仓库根 (已 build).

    Returns:
        相对 source 的 :class:`Path` 列表 (POSIX 风格), 排序后稳定.

    Raises:
        FileNotFoundError: source 不存在或非目录.
    """
    if not source.is_dir():
        raise FileNotFoundError(f"source 不是目录或不存在: {source}")

    matched: list[Path] = []
    for path in source.rglob("*"):
        if not path.is_file():
            continue
        rel = path.relative_to(source).as_posix()
        if not _matches_any(rel, WHITELIST_GLOBS):
            continue
        if _matches_any(rel, BLACKLIST_GLOBS):
            continue
        matched.append(Path(rel))

    return sorted(matched, key=lambda p: p.as_posix())


# ==================== 版本号 ====================
def read_source_version(source: Path) -> str:
    """读取 ``source/package.json:version`` 字段.

    Args:
        source: SnowLuma 仓库根.

    Returns:
        version 字符串 (例如 ``"0.1.0"``); package.json 缺失或字段空时 raise.

    Raises:
        FileNotFoundError: 根 ``package.json`` 不存在.
        ValueError: ``version`` 字段为空或非字符串.
    """
    pkg_path = source / "package.json"
    if not pkg_path.is_file():
        raise FileNotFoundError(f"未找到根 package.json: {pkg_path}")
    try:
        data = json.loads(pkg_path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        raise ValueError(f"package.json 非合法 JSON: {pkg_path}") from exc
    version = data.get("version")
    if not isinstance(version, str) or not version.strip():
        raise ValueError(f"package.json:version 字段无效: {version!r}")
    return version.strip()


# ==================== 打包 ====================
def build_tarball(source: Path, out_path: Path, files: list[Path]) -> None:
    """将 files 打成 ``out_path`` 的 tar.gz.

    所有条目以 ``ARCHIVE_TOP_LEVEL/`` 为顶层前缀, 远端 ``tar -xzf ... --strip-components=1``
    可剥掉, 直接落到 ``$SNOWLUMA_DIR/`` 下.

    Args:
        source: SnowLuma 仓库根.
        out_path: 目标 ``.tar.gz``; 父目录会自动创建.
        files: ``collect_files`` 的输出 (相对路径列表).
    """
    out_path.parent.mkdir(parents=True, exist_ok=True)
    if out_path.exists():
        out_path.unlink()

    with tarfile.open(out_path, "w:gz") as tf:
        for rel in files:
            full = source / rel
            arcname = f"{ARCHIVE_TOP_LEVEL}/{rel.as_posix()}"
            tf.add(full, arcname=arcname, recursive=False)


def write_version_sidecar(out_path: Path, version: str) -> Path:
    """在 ``out_path`` 同级写 ``<name>.version.txt``.

    Args:
        out_path: tar.gz 路径 (例 ``snowluma_framework_lite.tar.gz``).
        version: 版本字符串 (例 ``"0.1.0"``).

    Returns:
        version 文件路径.
    """
    sidecar = out_path.parent / out_path.name.replace(".tar.gz", ".version.txt")
    sidecar.write_text(version + "\n", encoding="utf-8")
    return sidecar


# ==================== 入口 ====================
def build(source: Path, out_path: Path) -> tuple[Path, Path, str, int]:
    """端到端构建 lite tarball + version sidecar.

    Args:
        source: SnowLuma 仓库根.
        out_path: 目标 ``.tar.gz`` 路径.

    Returns:
        ``(tarball_path, version_sidecar, version, file_count)``.

    Raises:
        FileNotFoundError: source 不存在或缺关键文件.
        ValueError: 版本号无效或产物大小越界.
    """
    version = read_source_version(source)
    files = collect_files(source)
    if not files:
        raise ValueError(f"白名单匹配为空, 检查 source 是否已 build: {source}")

    build_tarball(source, out_path, files)

    size = out_path.stat().st_size
    if size < MIN_SIZE_BYTES:
        raise ValueError(
            f"产物过小 ({size} B < {MIN_SIZE_BYTES} B); 白名单可能未命中预期文件"
        )
    if size > MAX_SIZE_BYTES:
        raise ValueError(
            f"产物过大 ({size} B > {MAX_SIZE_BYTES} B); 黑名单可能漏拦截"
        )

    sidecar = write_version_sidecar(out_path, version)
    return out_path, sidecar, version, len(files)


def _parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Build SnowLuma.Framework lite tarball for Desktop bundling.",
    )
    parser.add_argument(
        "--source",
        required=True,
        type=Path,
        help="SnowLuma 仓库根 (需已 npm run build)",
    )
    parser.add_argument(
        "--out",
        required=True,
        type=Path,
        help="输出 .tar.gz 路径; sibling .version.txt 同位置写入",
    )
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = _parse_args(argv)
    try:
        tarball, sidecar, version, count = build(args.source.resolve(), args.out.resolve())
    except (FileNotFoundError, ValueError) as exc:
        print(f"[build_snowluma_framework_lite] ERROR: {exc}", file=sys.stderr)
        return 1
    size_mb = tarball.stat().st_size / (1024 * 1024)
    print(f"[build_snowluma_framework_lite] OK: {tarball} ({size_mb:.2f} MB, {count} files)")
    print(f"[build_snowluma_framework_lite] version: {version} -> {sidecar}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
