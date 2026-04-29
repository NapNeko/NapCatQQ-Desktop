# -*- coding: utf-8 -*-
"""[`LocalBackend`](src/desktop/core/operation/local_backend.py) 单元测试。

覆盖 P0 阶段已实现的方法：文件 8 op + 检测类 (``detect_napcat_version`` / ``detect_qq_path`` / ``detect_installation``)。
进程 / 安装写入 / 日志 / WebUI 这些 NotImplementedError 也单独验证。
"""
from __future__ import annotations

from pathlib import Path

import pytest

from src.desktop.core.operation import (
    FileEntry,
    InstallationInfo,
    LocalBackend,
    OperationBackend,
)


@pytest.fixture
def backend() -> LocalBackend:
    return LocalBackend()


# ==================== 文件类 ====================
class TestFileOperations:
    def test_write_then_read(self, backend: LocalBackend, tmp_path: Path) -> None:
        target = tmp_path / "sub" / "config.json"
        backend.write_file(str(target), '{"key": "值"}')

        assert target.exists()
        assert backend.read_file(str(target)) == '{"key": "值"}'

    def test_write_file_creates_parent_dirs(self, backend: LocalBackend, tmp_path: Path) -> None:
        target = tmp_path / "a" / "b" / "c.txt"
        backend.write_file(str(target), "hi")
        assert target.parent.is_dir()

    def test_file_exists(self, backend: LocalBackend, tmp_path: Path) -> None:
        target = tmp_path / "x.txt"
        assert backend.file_exists(str(target)) is False
        target.write_text("v", encoding="utf-8")
        assert backend.file_exists(str(target)) is True
        # 目录也算存在
        assert backend.file_exists(str(tmp_path)) is True

    def test_list_dir(self, backend: LocalBackend, tmp_path: Path) -> None:
        (tmp_path / "a.txt").write_text("aa", encoding="utf-8")
        (tmp_path / "b").mkdir()
        (tmp_path / "b" / "c.txt").write_text("ccc", encoding="utf-8")

        entries = backend.list_dir(str(tmp_path))
        names = {entry.name: entry for entry in entries}

        assert "a.txt" in names and "b" in names
        assert names["a.txt"].is_dir is False
        assert names["a.txt"].size == 2
        assert names["b"].is_dir is True
        # 子目录大小固定 0(接口约定)
        assert names["b"].size == 0
        # 返回类型
        assert all(isinstance(e, FileEntry) for e in entries)

    def test_list_dir_missing(self, backend: LocalBackend, tmp_path: Path) -> None:
        with pytest.raises(FileNotFoundError):
            backend.list_dir(str(tmp_path / "no_such"))

    def test_list_dir_not_directory(self, backend: LocalBackend, tmp_path: Path) -> None:
        f = tmp_path / "f.txt"
        f.write_text("x", encoding="utf-8")
        with pytest.raises(NotADirectoryError):
            backend.list_dir(str(f))

    def test_mkdir(self, backend: LocalBackend, tmp_path: Path) -> None:
        target = tmp_path / "deep" / "nested"
        backend.mkdir(str(target))
        assert target.is_dir()
        # 已存在: 默认 exist_ok=True 不抛
        backend.mkdir(str(target))

    def test_mkdir_no_parents_no_existok(self, backend: LocalBackend, tmp_path: Path) -> None:
        # 不存在父目录时 parents=False 会抛 FileNotFoundError
        with pytest.raises(FileNotFoundError):
            backend.mkdir(str(tmp_path / "a" / "b"), parents=False, exist_ok=False)

    def test_remove_file(self, backend: LocalBackend, tmp_path: Path) -> None:
        f = tmp_path / "x.txt"
        f.write_text("v", encoding="utf-8")
        backend.remove(str(f))
        assert not f.exists()
        # 幂等: 再次删除不抛
        backend.remove(str(f))

    def test_remove_dir_requires_recursive(self, backend: LocalBackend, tmp_path: Path) -> None:
        d = tmp_path / "d"
        d.mkdir()
        (d / "x.txt").write_text("v", encoding="utf-8")

        with pytest.raises(IsADirectoryError):
            backend.remove(str(d))

        backend.remove(str(d), recursive=True)
        assert not d.exists()

    def test_upload_download_are_local_copy(self, backend: LocalBackend, tmp_path: Path) -> None:
        source = tmp_path / "src.bin"
        source.write_bytes(b"\x00\x01\x02hello\xff")

        # upload = local copy
        target = tmp_path / "out" / "dst.bin"
        backend.upload(source, str(target))
        assert target.read_bytes() == source.read_bytes()

        # download = 反向 copy
        echo = tmp_path / "echo.bin"
        backend.download(str(target), echo)
        assert echo.read_bytes() == source.read_bytes()


# ==================== 检测类 ====================
class TestDetection:
    def test_detect_napcat_version_when_path_unavailable(
        self, backend: LocalBackend, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        # 强制 _resolve_napcat_path 返回 None
        monkeypatch.setattr(LocalBackend, "_resolve_napcat_path", staticmethod(lambda: None))
        assert backend.detect_napcat_version() is None

    def test_detect_napcat_version_parses_const(
        self, backend: LocalBackend, monkeypatch: pytest.MonkeyPatch, tmp_path: Path
    ) -> None:
        napcat_dir = tmp_path / "napcat"
        napcat_dir.mkdir()
        (napcat_dir / "napcat.mjs").write_text(
            'export const version = "1.2.3";\nconsole.log("ok");',
            encoding="utf-8",
        )
        monkeypatch.setattr(LocalBackend, "_resolve_napcat_path", staticmethod(lambda: napcat_dir))
        assert backend.detect_napcat_version() == "1.2.3"

    def test_detect_napcat_version_returns_none_when_mjs_missing(
        self, backend: LocalBackend, monkeypatch: pytest.MonkeyPatch, tmp_path: Path
    ) -> None:
        napcat_dir = tmp_path / "napcat"
        napcat_dir.mkdir()  # 没有 napcat.mjs
        monkeypatch.setattr(LocalBackend, "_resolve_napcat_path", staticmethod(lambda: napcat_dir))
        assert backend.detect_napcat_version() is None

    def test_detect_qq_path_returns_none_when_missing(
        self, backend: LocalBackend, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        from src.desktop.core.runtime.paths import PathFunc

        monkeypatch.setattr(PathFunc, "get_qq_path", staticmethod(lambda: None))
        assert backend.detect_qq_path() is None

    def test_detect_installation_aggregate(
        self, backend: LocalBackend, monkeypatch: pytest.MonkeyPatch, tmp_path: Path
    ) -> None:
        # 准备 fake QQ 目录及 package.json
        qq_path = tmp_path / "QQ"
        (qq_path / "resources" / "app").mkdir(parents=True)
        (qq_path / "resources" / "app" / "package.json").write_text(
            '{"version": "9.9.9-beta", "name": "qq"}',
            encoding="utf-8",
        )
        # 准备 fake NapCat 目录
        napcat_dir = tmp_path / "napcat"
        napcat_dir.mkdir()
        (napcat_dir / "napcat.mjs").write_text('const version = "0.5.0"', encoding="utf-8")

        from src.desktop.core.runtime.paths import PathFunc

        monkeypatch.setattr(PathFunc, "get_qq_path", staticmethod(lambda: qq_path))
        monkeypatch.setattr(LocalBackend, "_resolve_napcat_path", staticmethod(lambda: napcat_dir))

        info = backend.detect_installation()
        assert isinstance(info, InstallationInfo)
        assert info.napcat_version == "0.5.0"
        assert info.qq_version == "9.9.9-beta"
        assert info.qq_install_path == str(qq_path)


# ==================== 接口契约 ====================
class TestInterfaceContract:
    def test_local_backend_is_operation_backend(self, backend: LocalBackend) -> None:
        assert isinstance(backend, OperationBackend)

    def test_lifecycle_default_noop(self, backend: LocalBackend) -> None:
        # 本地后端默认 connect/close 为 no-op, is_connected 恒 True
        assert backend.is_connected is True
        backend.connect()
        backend.close()
        assert backend.is_connected is True

    def test_context_manager(self, backend: LocalBackend) -> None:
        with backend as b:
            assert b is backend


# ==================== P2 延迟实现 ====================
class TestDeferredMethods:
    """文档级保障: 这些方法在 P0 阶段必须 raise NotImplementedError, 不能默默返回错误数据。"""

    def test_start_napcat_deferred(self, backend: LocalBackend, config_factory) -> None:
        with pytest.raises(NotImplementedError):
            backend.start_napcat("114514", config_factory())

    def test_stop_napcat_deferred(self, backend: LocalBackend) -> None:
        with pytest.raises(NotImplementedError):
            backend.stop_napcat("114514")

    def test_get_process_status_deferred(self, backend: LocalBackend) -> None:
        with pytest.raises(NotImplementedError):
            backend.get_process_status("114514")

    def test_install_napcat_deferred(self, backend: LocalBackend) -> None:
        with pytest.raises(NotImplementedError):
            backend.install_napcat()

    def test_install_qq_deferred(self, backend: LocalBackend) -> None:
        with pytest.raises(NotImplementedError):
            backend.install_qq()

    def test_read_log_deferred(self, backend: LocalBackend) -> None:
        with pytest.raises(NotImplementedError):
            backend.read_log("114514")

    def test_get_webui_endpoint_deferred(self, backend: LocalBackend) -> None:
        with pytest.raises(NotImplementedError):
            backend.get_webui_endpoint("114514")
