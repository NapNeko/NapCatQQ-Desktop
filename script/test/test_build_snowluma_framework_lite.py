# -*- coding: utf-8 -*-
""":mod:`script.build_scripts.build_snowluma_framework_lite` 单测 (W3).

覆盖:

- ``read_source_version``: 解析 ``package.json:version``
- ``collect_files``: 白名单匹配 + 黑名单排除
- ``build_tarball``: 产物结构与 ARCHIVE_TOP_LEVEL 前缀
- ``build`` 端到端: 大小约束 / 错误路径
- 命令行入口: 参数缺失或 source 不存在
"""

from __future__ import annotations

import json
import sys
import tarfile
from pathlib import Path
from typing import Callable

import pytest

# 把 script/build_scripts 加入 path 以便 import (与 conftest.py 的 PROJECT_ROOT 协同)
PROJECT_ROOT = Path(__file__).resolve().parents[2]
BUILD_SCRIPTS_DIR = PROJECT_ROOT / "script" / "build_scripts"
if str(BUILD_SCRIPTS_DIR) not in sys.path:
    sys.path.insert(0, str(BUILD_SCRIPTS_DIR))

import build_snowluma_framework_lite as builder  # noqa: E402


# ==================== fixture: fake SL source tree ====================
@pytest.fixture
def fake_source(tmp_path: Path) -> Path:
    """构造一个迷你 SnowLuma source tree.

    包含白名单要求的全部文件 (内容随便, 仅用于 tar 结构测试) +
    若干黑名单文件 (验证排除).
    """
    root = tmp_path / "fake-sl"
    root.mkdir()

    # 根 manifests
    (root / "package.json").write_text(
        json.dumps({"name": "snowluma", "version": "0.1.0"}),
        encoding="utf-8",
    )
    (root / "LICENSE").write_text("MIT", encoding="utf-8")

    # vite 输出: dist/
    dist = root / "dist"
    dist.mkdir()
    (dist / "index.mjs").write_text("export default {};\n", encoding="utf-8")
    (dist / "index.mjs.map").write_text("{}\n", encoding="utf-8")  # 黑名单 *.map
    assets = dist / "assets"
    assets.mkdir()
    (assets / "main.js").write_text("//\n", encoding="utf-8")
    (assets / "style.css").write_text("body{}\n", encoding="utf-8")

    # packages/runtime/
    rt_native = root / "packages" / "runtime" / "native"
    rt_native.mkdir(parents=True)
    (root / "packages" / "runtime" / "launcher.sh").write_text("#!/bin/sh\n", encoding="utf-8")
    (root / "packages" / "runtime" / "package.json").write_text(
        '{"name":"@snowluma/runtime"}', encoding="utf-8"
    )
    # Linux native (白名单)
    for fname in (
        "snowluma-linux-x64.node",
        "snowluma-linux-x64.so",
        "snowluma-linux-arm64.node",
        "snowluma-linux-arm64.so",
        "websocket-linux-x64.node",
        "websocket-linux-arm64.node",
    ):
        (rt_native / fname).write_bytes(b"\x00" * 16)
    # Win/macOS native (黑名单)
    for fname in (
        "snowluma-win32-x64.dll",
        "snowluma-win32-x64.node",
        "snowluma-darwin-arm64.node",
    ):
        (rt_native / fname).write_bytes(b"\x00" * 16)

    # ffmpeg
    ffmpeg_dir = rt_native / "ffmpeg"
    ffmpeg_dir.mkdir()
    (ffmpeg_dir / "ffmpegAddon.linux.x64.node").write_bytes(b"\x00" * 16)
    (ffmpeg_dir / "ffmpegAddon.linux.arm64.node").write_bytes(b"\x00" * 16)
    (ffmpeg_dir / "ffmpegAddon.win32.x64.node").write_bytes(b"\x00" * 16)  # 黑名单
    (ffmpeg_dir / "ffmpegAddon.darwin.arm64.node").write_bytes(b"\x00" * 16)  # 黑名单

    # packages/sdk/ — 整包黑名单
    sdk = root / "packages" / "sdk" / "dist"
    sdk.mkdir(parents=True)
    (sdk / "index.js").write_text("export {};\n", encoding="utf-8")

    # packages/core/src/ — 黑名单 src
    core_src = root / "packages" / "core" / "src"
    core_src.mkdir(parents=True)
    (core_src / "index.ts").write_text("export {};\n", encoding="utf-8")

    # packages/webui/dist/ — 黑名单 (已合并到根 dist/)
    webui_dist = root / "packages" / "webui" / "dist"
    webui_dist.mkdir(parents=True)
    (webui_dist / "duplicate.js").write_text("//\n", encoding="utf-8")

    # node_modules — 黑名单
    nm = root / "node_modules" / "react"
    nm.mkdir(parents=True)
    (nm / "index.js").write_text("//\n", encoding="utf-8")

    return root


# ==================== read_source_version ====================
class TestReadSourceVersion:
    def test_valid(self, fake_source: Path) -> None:
        assert builder.read_source_version(fake_source) == "0.1.0"

    def test_missing_package_json(self, tmp_path: Path) -> None:
        with pytest.raises(FileNotFoundError):
            builder.read_source_version(tmp_path)

    def test_invalid_json(self, tmp_path: Path) -> None:
        (tmp_path / "package.json").write_text("not json", encoding="utf-8")
        with pytest.raises(ValueError, match="非合法 JSON"):
            builder.read_source_version(tmp_path)

    def test_empty_version(self, tmp_path: Path) -> None:
        (tmp_path / "package.json").write_text(
            json.dumps({"name": "x", "version": ""}), encoding="utf-8"
        )
        with pytest.raises(ValueError, match="version 字段无效"):
            builder.read_source_version(tmp_path)

    def test_missing_version(self, tmp_path: Path) -> None:
        (tmp_path / "package.json").write_text(json.dumps({"name": "x"}), encoding="utf-8")
        with pytest.raises(ValueError, match="version 字段无效"):
            builder.read_source_version(tmp_path)


# ==================== collect_files ====================
class TestCollectFiles:
    def test_includes_whitelisted(self, fake_source: Path) -> None:
        files = {p.as_posix() for p in builder.collect_files(fake_source)}
        assert "package.json" in files
        assert "LICENSE" in files
        assert "dist/index.mjs" in files
        assert "dist/assets/main.js" in files
        assert "dist/assets/style.css" in files
        assert "packages/runtime/launcher.sh" in files
        assert "packages/runtime/package.json" in files
        assert "packages/runtime/native/snowluma-linux-x64.node" in files
        assert "packages/runtime/native/snowluma-linux-x64.so" in files
        assert "packages/runtime/native/snowluma-linux-arm64.node" in files
        assert "packages/runtime/native/websocket-linux-x64.node" in files
        assert "packages/runtime/native/ffmpeg/ffmpegAddon.linux.x64.node" in files
        assert "packages/runtime/native/ffmpeg/ffmpegAddon.linux.arm64.node" in files

    def test_excludes_blacklisted(self, fake_source: Path) -> None:
        files = {p.as_posix() for p in builder.collect_files(fake_source)}
        # *.map
        assert not any(p.endswith(".map") for p in files)
        # win32 / darwin native
        assert "packages/runtime/native/snowluma-win32-x64.dll" not in files
        assert "packages/runtime/native/snowluma-win32-x64.node" not in files
        assert "packages/runtime/native/snowluma-darwin-arm64.node" not in files
        assert "packages/runtime/native/ffmpeg/ffmpegAddon.win32.x64.node" not in files
        assert "packages/runtime/native/ffmpeg/ffmpegAddon.darwin.arm64.node" not in files
        # SDK
        assert "packages/sdk/dist/index.js" not in files
        # 源码
        assert "packages/core/src/index.ts" not in files
        # webui/dist 重复
        assert "packages/webui/dist/duplicate.js" not in files
        # node_modules
        assert "node_modules/react/index.js" not in files

    def test_source_not_directory_raises(self, tmp_path: Path) -> None:
        with pytest.raises(FileNotFoundError, match="不是目录"):
            builder.collect_files(tmp_path / "no_such_dir")

    def test_returns_sorted_paths(self, fake_source: Path) -> None:
        files = builder.collect_files(fake_source)
        sorted_posix = sorted(p.as_posix() for p in files)
        assert [p.as_posix() for p in files] == sorted_posix


# ==================== build_tarball ====================
class TestBuildTarball:
    def test_archive_has_top_level_prefix(
        self, fake_source: Path, tmp_path: Path
    ) -> None:
        out = tmp_path / "sl.tar.gz"
        files = builder.collect_files(fake_source)
        builder.build_tarball(fake_source, out, files)

        with tarfile.open(out, "r:gz") as tf:
            members = tf.getnames()

        assert all(m.startswith("snowluma-framework/") for m in members)
        assert "snowluma-framework/dist/index.mjs" in members
        assert "snowluma-framework/package.json" in members

    def test_archive_overwrites_existing(
        self, fake_source: Path, tmp_path: Path
    ) -> None:
        out = tmp_path / "sl.tar.gz"
        out.write_bytes(b"old garbage")
        files = builder.collect_files(fake_source)
        builder.build_tarball(fake_source, out, files)

        with tarfile.open(out, "r:gz") as tf:
            assert tf.getnames()  # 不为空, 即新打的 tar


# ==================== build (端到端) ====================
class TestBuildEndToEnd:
    def test_size_too_small_raises(
        self,
        fake_source: Path,
        tmp_path: Path,
        monkeypatch: pytest.MonkeyPatch,
    ) -> None:
        # 提高 MIN_SIZE_BYTES 让 fake source 必触发下限
        monkeypatch.setattr(builder, "MIN_SIZE_BYTES", 10 * 1024 * 1024)
        with pytest.raises(ValueError, match="产物过小"):
            builder.build(fake_source, tmp_path / "sl.tar.gz")

    def test_size_too_large_raises(
        self,
        fake_source: Path,
        tmp_path: Path,
        monkeypatch: pytest.MonkeyPatch,
    ) -> None:
        # 把上限调到 0, fake source 必触发上限
        monkeypatch.setattr(builder, "MIN_SIZE_BYTES", 0)
        monkeypatch.setattr(builder, "MAX_SIZE_BYTES", 100)
        with pytest.raises(ValueError, match="产物过大"):
            builder.build(fake_source, tmp_path / "sl.tar.gz")

    def test_success_writes_sidecar(
        self,
        fake_source: Path,
        tmp_path: Path,
        monkeypatch: pytest.MonkeyPatch,
    ) -> None:
        # 关掉大小约束, 验证端到端流程
        monkeypatch.setattr(builder, "MIN_SIZE_BYTES", 0)
        monkeypatch.setattr(builder, "MAX_SIZE_BYTES", 100 * 1024 * 1024)
        out = tmp_path / "sl.tar.gz"
        tarball, sidecar, version, count = builder.build(fake_source, out)

        assert tarball == out
        assert tarball.is_file()
        assert sidecar == tmp_path / "sl.version.txt"
        assert sidecar.is_file()
        assert sidecar.read_text(encoding="utf-8").strip() == "0.1.0"
        assert version == "0.1.0"
        assert count > 5  # 至少包含若干文件

    def test_completely_empty_source_raises(self, tmp_path: Path) -> None:
        """完全空目录 (无 package.json) 必报 FileNotFoundError."""
        empty = tmp_path / "empty"
        empty.mkdir()
        with pytest.raises(FileNotFoundError, match="package.json"):
            builder.build(empty, tmp_path / "out.tar.gz")

    def test_only_blacklist_files_raises_empty_whitelist(
        self,
        tmp_path: Path,
        monkeypatch: pytest.MonkeyPatch,
    ) -> None:
        """source 含 package.json + 只有黑名单文件 → 白名单匹配为空 ValueError."""
        # 临时把 package.json 加入黑名单, 模拟 "白名单全被黑名单覆盖" 情形
        # 这是触发 ``白名单匹配为空`` 分支的最直接方式
        only_bad = tmp_path / "only_blacklist"
        only_bad.mkdir()
        (only_bad / "package.json").write_text(
            json.dumps({"version": "9.9.9"}), encoding="utf-8"
        )
        node_modules = only_bad / "node_modules" / "react"
        node_modules.mkdir(parents=True)
        (node_modules / "index.js").write_text("//", encoding="utf-8")

        monkeypatch.setattr(
            builder,
            "BLACKLIST_GLOBS",
            (*builder.BLACKLIST_GLOBS, "package.json", "LICENSE"),
        )
        monkeypatch.setattr(builder, "MIN_SIZE_BYTES", 0)
        with pytest.raises(ValueError, match="白名单匹配为空"):
            builder.build(only_bad, tmp_path / "out.tar.gz")


# ==================== CLI ====================
class TestCLI:
    def test_missing_args_exits(self) -> None:
        with pytest.raises(SystemExit):
            builder._parse_args([])

    def test_main_returns_1_on_missing_source(
        self, tmp_path: Path, capsys: pytest.CaptureFixture
    ) -> None:
        rc = builder.main(
            ["--source", str(tmp_path / "no_such"), "--out", str(tmp_path / "x.tar.gz")]
        )
        assert rc == 1
        captured = capsys.readouterr()
        assert "ERROR" in captured.err
