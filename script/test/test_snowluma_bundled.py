# -*- coding: utf-8 -*-
""":mod:`src.core.remote.snowluma.bundled` 单测 (W3).

覆盖:

- 开发态探测路径优先级 (仓库根 ``src/resource/runtime/``)
- 冻结态探测路径 (``base_path / resource/runtime/``)
- 文件不存在时 ``find_*`` 返回 ``None``, ``read_bundled_version`` 返回 ``None``
- 版本 sidecar 内容读取 + 空白裁剪
- 版本 sidecar 编码异常时降级 (返 ``None`` 不 raise)
"""

from __future__ import annotations

import sys
from pathlib import Path

import pytest

from src.core.remote.snowluma import bundled


# ==================== _candidate_dirs ====================
class TestCandidateDirs:
    def test_dev_mode_uses_src_resource_runtime(
        self, monkeypatch: pytest.MonkeyPatch, tmp_path: Path
    ) -> None:
        # 模拟开发态 (sys.frozen 不存在)
        monkeypatch.delattr(sys, "frozen", raising=False)
        monkeypatch.setattr(bundled, "resolve_app_base_path", lambda: tmp_path)

        dirs = bundled._candidate_dirs()
        assert tmp_path / "src" / "resource" / "runtime" in dirs

    def test_frozen_mode_uses_resource_runtime(
        self, monkeypatch: pytest.MonkeyPatch, tmp_path: Path
    ) -> None:
        monkeypatch.setattr(sys, "frozen", True, raising=False)
        monkeypatch.setattr(bundled, "resolve_app_base_path", lambda: tmp_path)
        # 不设 _MEIPASS 也应能拿到 base_path 候选
        monkeypatch.delattr(sys, "_MEIPASS", raising=False)

        dirs = bundled._candidate_dirs()
        assert tmp_path / "resource" / "runtime" in dirs

    def test_frozen_mode_includes_meipass_fallback(
        self, monkeypatch: pytest.MonkeyPatch, tmp_path: Path
    ) -> None:
        meipass = tmp_path / "_meipass"
        meipass.mkdir()
        monkeypatch.setattr(sys, "frozen", True, raising=False)
        monkeypatch.setattr(sys, "_MEIPASS", str(meipass), raising=False)
        monkeypatch.setattr(bundled, "resolve_app_base_path", lambda: tmp_path)

        dirs = bundled._candidate_dirs()
        assert meipass / "resource" / "runtime" in dirs


# ==================== find_bundled_lite_tarball ====================
class TestFindBundledLiteTarball:
    def test_returns_none_when_missing(
        self, monkeypatch: pytest.MonkeyPatch, tmp_path: Path
    ) -> None:
        monkeypatch.delattr(sys, "frozen", raising=False)
        monkeypatch.setattr(bundled, "resolve_app_base_path", lambda: tmp_path)
        assert bundled.find_bundled_lite_tarball() is None

    def test_returns_path_when_present(
        self, monkeypatch: pytest.MonkeyPatch, tmp_path: Path
    ) -> None:
        monkeypatch.delattr(sys, "frozen", raising=False)
        monkeypatch.setattr(bundled, "resolve_app_base_path", lambda: tmp_path)

        runtime_dir = tmp_path / "src" / "resource" / "runtime"
        runtime_dir.mkdir(parents=True)
        tarball = runtime_dir / bundled.LITE_TARBALL_FILENAME
        tarball.write_bytes(b"fake tarball")

        assert bundled.find_bundled_lite_tarball() == tarball

    def test_frozen_mode_finds_tarball(
        self, monkeypatch: pytest.MonkeyPatch, tmp_path: Path
    ) -> None:
        monkeypatch.setattr(sys, "frozen", True, raising=False)
        monkeypatch.delattr(sys, "_MEIPASS", raising=False)
        monkeypatch.setattr(bundled, "resolve_app_base_path", lambda: tmp_path)

        runtime_dir = tmp_path / "resource" / "runtime"
        runtime_dir.mkdir(parents=True)
        tarball = runtime_dir / bundled.LITE_TARBALL_FILENAME
        tarball.write_bytes(b"fake tarball")

        assert bundled.find_bundled_lite_tarball() == tarball


# ==================== find_bundled_version_sidecar / read_bundled_version ====================
class TestReadBundledVersion:
    def test_returns_none_when_missing(
        self, monkeypatch: pytest.MonkeyPatch, tmp_path: Path
    ) -> None:
        monkeypatch.delattr(sys, "frozen", raising=False)
        monkeypatch.setattr(bundled, "resolve_app_base_path", lambda: tmp_path)
        assert bundled.find_bundled_version_sidecar() is None
        assert bundled.read_bundled_version() is None

    def test_reads_version_string(
        self, monkeypatch: pytest.MonkeyPatch, tmp_path: Path
    ) -> None:
        monkeypatch.delattr(sys, "frozen", raising=False)
        monkeypatch.setattr(bundled, "resolve_app_base_path", lambda: tmp_path)

        runtime_dir = tmp_path / "src" / "resource" / "runtime"
        runtime_dir.mkdir(parents=True)
        sidecar = runtime_dir / bundled.LITE_VERSION_FILENAME
        sidecar.write_text("0.1.0\n", encoding="utf-8")

        assert bundled.find_bundled_version_sidecar() == sidecar
        assert bundled.read_bundled_version() == "0.1.0"

    def test_strips_whitespace(
        self, monkeypatch: pytest.MonkeyPatch, tmp_path: Path
    ) -> None:
        monkeypatch.delattr(sys, "frozen", raising=False)
        monkeypatch.setattr(bundled, "resolve_app_base_path", lambda: tmp_path)

        runtime_dir = tmp_path / "src" / "resource" / "runtime"
        runtime_dir.mkdir(parents=True)
        (runtime_dir / bundled.LITE_VERSION_FILENAME).write_text(
            "  v1.2.3-beta1  \n\r\n", encoding="utf-8"
        )

        assert bundled.read_bundled_version() == "v1.2.3-beta1"

    def test_empty_file_returns_none(
        self, monkeypatch: pytest.MonkeyPatch, tmp_path: Path
    ) -> None:
        monkeypatch.delattr(sys, "frozen", raising=False)
        monkeypatch.setattr(bundled, "resolve_app_base_path", lambda: tmp_path)

        runtime_dir = tmp_path / "src" / "resource" / "runtime"
        runtime_dir.mkdir(parents=True)
        (runtime_dir / bundled.LITE_VERSION_FILENAME).write_text(
            "   \n", encoding="utf-8"
        )

        assert bundled.read_bundled_version() is None

    def test_decode_error_returns_none(
        self, monkeypatch: pytest.MonkeyPatch, tmp_path: Path
    ) -> None:
        """非 UTF-8 文件 ``read_bundled_version`` 优雅降级返 None."""
        monkeypatch.delattr(sys, "frozen", raising=False)
        monkeypatch.setattr(bundled, "resolve_app_base_path", lambda: tmp_path)

        runtime_dir = tmp_path / "src" / "resource" / "runtime"
        runtime_dir.mkdir(parents=True)
        # 写入纯 0x80-0xFF 二进制, 在 UTF-8 解码时必失败
        (runtime_dir / bundled.LITE_VERSION_FILENAME).write_bytes(b"\x80\x81\x82\xff\xfe")

        assert bundled.read_bundled_version() is None


# ==================== 公开 API 集成 (lazy import) ====================
class TestPackageReExports:
    """从 :mod:`src.core.remote.snowluma` 顶层 import 也应可用 (lazy)."""

    def test_top_level_imports(self) -> None:
        from src.core.remote.snowluma import (
            find_bundled_lite_tarball,
            find_bundled_version_sidecar,
            read_bundled_version,
        )

        assert callable(find_bundled_lite_tarball)
        assert callable(find_bundled_version_sidecar)
        assert callable(read_bundled_version)
