# -*- coding: utf-8 -*-
"""SnowLuma 适配 P7.5: VersionSnapshot + RemoteVersionTask + LocalVersionTask 单测.

参见: ``docs/requirements/2026-05-10-snowluma-backend-adapter.md`` §4.2
"""

from __future__ import annotations

from pathlib import Path

import pytest
from creart import it

from src.core.runtime.paths import PathFunc
from src.core.versioning.service import (
    LocalVersionTask,
    RemoteVersionTask,
    VersionSnapshot,
)


# ==================== VersionSnapshot ====================
class TestVersionSnapshot:
    def test_legacy_kwargs_compat(self) -> None:
        """旧调用 (不传 snowluma_*) 应能成功构造, 默认 None."""
        vs = VersionSnapshot.model_validate(
            {"napcat_version": None, "qq_version": None, "ncd_version": None}
        )
        assert vs.snowluma_version is None
        assert vs.snowluma_update_log is None

    def test_full_kwargs(self) -> None:
        vs = VersionSnapshot(
            napcat_version="v4.0.0",
            qq_version="9.9.0",
            ncd_version="v1.7.0",
            snowluma_version="v1.7.5",
            qq_download_url="https://qq.example/dl",
            napcat_update_log="napcat changelog",
            ncd_update_log="ncd changelog",
            snowluma_update_log="snowluma changelog",
        )
        assert vs.snowluma_version == "v1.7.5"
        assert vs.snowluma_update_log == "snowluma changelog"


# ==================== LocalVersionTask.get_snowluma_version ====================
class TestLocalVersionTaskSnowLuma:
    def _patch_snowluma_path(self, tmp_path: Path, monkeypatch) -> Path:
        fake_root = tmp_path / "SnowLuma"
        fake_root.mkdir()
        path_func = it(PathFunc)
        monkeypatch.setattr(path_func, "snowluma_path", fake_root)
        return fake_root

    def test_returns_none_when_no_installed_tag(self, tmp_path: Path, monkeypatch) -> None:
        self._patch_snowluma_path(tmp_path, monkeypatch)
        assert LocalVersionTask().get_snowluma_version() is None

    def test_returns_tag_string_when_present(self, tmp_path: Path, monkeypatch) -> None:
        root = self._patch_snowluma_path(tmp_path, monkeypatch)
        (root / ".installed_tag").write_text("v1.7.5", encoding="utf-8")
        assert LocalVersionTask().get_snowluma_version() == "v1.7.5"

    def test_strips_whitespace(self, tmp_path: Path, monkeypatch) -> None:
        root = self._patch_snowluma_path(tmp_path, monkeypatch)
        (root / ".installed_tag").write_text("  v2.0.0 \n", encoding="utf-8")
        assert LocalVersionTask().get_snowluma_version() == "v2.0.0"

    def test_empty_file_returns_none(self, tmp_path: Path, monkeypatch) -> None:
        root = self._patch_snowluma_path(tmp_path, monkeypatch)
        (root / ".installed_tag").write_text("   \n", encoding="utf-8")
        assert LocalVersionTask().get_snowluma_version() is None

    def test_local_execute_includes_snowluma(self, tmp_path: Path, monkeypatch) -> None:
        """LocalVersionTask.execute() 把 snowluma_version 写入 VersionSnapshot."""
        root = self._patch_snowluma_path(tmp_path, monkeypatch)
        (root / ".installed_tag").write_text("v3.0.0", encoding="utf-8")

        task = LocalVersionTask()
        # napcat / qq / ncd 走真实 PathFunc / 配置, 测试环境下大概率返回 None;
        # 我们关心的只是 snowluma 字段被正确填入.
        snapshot = task.execute()
        assert snapshot.snowluma_version == "v3.0.0"


# ==================== RemoteVersionTask (mock httpx) ====================
class TestRemoteVersionTaskSnowLuma:
    def test_execute_pulls_snowluma_via_fallback_chain(self, monkeypatch) -> None:
        """RemoteVersionTask.execute() 多拉一份 SnowLuma; mock 全部网络返回模拟成功响应."""

        captured_urls: list[str] = []

        def fake_request(self, url, name, use_mirrors: bool = False, emit_error: bool = True):
            url_str = url.url() if hasattr(url, "url") else str(url)
            captured_urls.append(f"{name}:{url_str}")
            if name == "QQ":
                return {"Windows": {"version": "9.9.0", "ntDownloadX64Url": "https://qq.example/dl"}}
            return {"tag_name": f"v-{name}-stub", "body": f"# release notes for {name}"}

        monkeypatch.setattr(RemoteVersionTask, "request", fake_request)

        task = RemoteVersionTask()
        snapshot = task.execute()

        assert snapshot.snowluma_version == "v-SnowLuma-stub"
        assert snapshot.snowluma_update_log == "# release notes for SnowLuma"
        # 同时确认未污染其他后端
        assert snapshot.napcat_version == "v-NapCat-stub"
        assert snapshot.ncd_version == "v-NapCatQQ Desktop-stub"
        # SnowLuma 至少被请求一次 (主或备)
        assert any(u.startswith("SnowLuma:") for u in captured_urls)

    def test_execute_handles_snowluma_network_failure(self, monkeypatch) -> None:
        """SnowLuma 主+备都失败时, snowluma_version 应为 None 且不影响其他后端."""

        def fake_request(self, url, name, use_mirrors: bool = False, emit_error: bool = True):
            if name == "QQ":
                return {"Windows": {"version": "9.9.0", "ntDownloadX64Url": "https://qq.example/dl"}}
            if name == "SnowLuma":
                return None  # 模拟拨号失败
            return {"tag_name": f"v-{name}-stub", "body": f"notes-{name}"}

        monkeypatch.setattr(RemoteVersionTask, "request", fake_request)

        task = RemoteVersionTask()
        snapshot = task.execute()

        assert snapshot.snowluma_version is None
        assert snapshot.snowluma_update_log is None
        # NapCat / NCD 仍正常
        assert snapshot.napcat_version == "v-NapCat-stub"
        assert snapshot.ncd_version == "v-NapCatQQ Desktop-stub"
