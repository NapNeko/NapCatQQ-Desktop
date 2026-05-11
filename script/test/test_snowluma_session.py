# -*- coding: utf-8 -*-
"""SnowLuma session 单测 (W5 ``resolve_effective_password`` 签名重写).

W5 之前: ``resolve_effective_password(config)`` 依赖 :class:`BotConfig`, 这与
SnowLuma "全局密码" 语义矛盾 (一个 daemon 同时服务 N 个 Bot, 不能 per-Bot 各设密码).

W5 之后: ``resolve_effective_password(*, override="")`` 解耦 BotConfig, 只看
``override`` 字符串与 ``snowluma-session.json`` 的回退. App 级 override 由
:class:`SnowLumaDaemon` 从 ``cfg.snowluma_webui_password_override`` 读出后传入.

参见: ``docs/requirements/2026-05-11-snowluma-daemon-refactor.md`` §2.5,
``docs/plans/2026-05-11-snowluma-daemon-refactor-execution-plan.md`` §W5.
"""
from __future__ import annotations

import json
import os
from pathlib import Path

import pytest
from PySide6.QtWidgets import QApplication

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")


def ensure_qapp() -> QApplication:
    app = QApplication.instance()
    if app is None:
        app = QApplication([])
    return app


@pytest.fixture(autouse=True)
def _qapp() -> QApplication:
    return ensure_qapp()


@pytest.fixture
def isolated_session(tmp_path: Path, monkeypatch) -> Path:
    """把 ``snowluma_session.session_path`` 重定向到 tmp_path."""
    fake_session_path = tmp_path / "snowluma-session.json"
    monkeypatch.setattr(
        "src.core.runtime.snowluma_session.session_path",
        lambda: fake_session_path,
    )
    return fake_session_path


# ==================== resolve_effective_password ====================
class TestResolveEffectivePassword:
    """W5: ``resolve_effective_password(*, override="")`` 签名 + 行为."""

    def test_with_override_returns_override(
        self, isolated_session: Path
    ) -> None:
        """override 非空 → 直接返回, 不读 session.json."""
        from src.core.runtime.snowluma_session import resolve_effective_password

        assert not isolated_session.exists()
        result = resolve_effective_password(override="MyP@ssw0rd!")
        assert result == "MyP@ssw0rd!"
        # session.json 不应被创建 (override 路径不走 session 回退)
        assert not isolated_session.exists()

    def test_override_whitespace_treated_as_empty(
        self, isolated_session: Path
    ) -> None:
        """override 全空白 strip 后视为空, 走 session 回退."""
        from src.core.runtime.snowluma_session import resolve_effective_password

        assert not isolated_session.exists()
        result = resolve_effective_password(override="   ")
        # session.json 被现场创建, result == session.password
        assert isolated_session.exists()
        payload = json.loads(isolated_session.read_text(encoding="utf-8"))
        assert result == payload["password"]
        assert len(result) >= 10

    def test_empty_override_falls_back_to_session_create(
        self, isolated_session: Path
    ) -> None:
        """override="" + session.json 不存在 → 现场创建 session, 返回其密码."""
        from src.core.runtime.snowluma_session import resolve_effective_password

        assert not isolated_session.exists()
        result = resolve_effective_password(override="")
        assert isolated_session.exists()
        payload = json.loads(isolated_session.read_text(encoding="utf-8"))
        assert result == payload["password"]

    def test_empty_override_with_existing_session_returns_session_password(
        self, isolated_session: Path
    ) -> None:
        """override="" + session.json 已存在 → 返回其密码 (不重新生成)."""
        from src.core.runtime.snowluma_session import (
            create_session,
            resolve_effective_password,
        )

        # 先 create 一份 session
        existing = create_session()
        # 再 resolve 应返回同密码
        result = resolve_effective_password(override="")
        assert result == existing.password

    def test_no_arg_defaults_to_empty_override(
        self, isolated_session: Path
    ) -> None:
        """不传参 (默认 override="") 等价显式 ""; 走 session 路径."""
        from src.core.runtime.snowluma_session import resolve_effective_password

        result = resolve_effective_password()
        assert isolated_session.exists()
        payload = json.loads(isolated_session.read_text(encoding="utf-8"))
        assert result == payload["password"]

    def test_old_signature_with_config_no_longer_supported(self) -> None:
        """W5: 旧 ``resolve_effective_password(config)`` 位置参数不再支持."""
        from src.core.runtime.snowluma_session import resolve_effective_password

        # 旧用法: resolve_effective_password(some_bot_config) 应 TypeError
        # (现签名只接 keyword-only ``override``)
        with pytest.raises(TypeError):
            resolve_effective_password("some_positional_arg")  # type: ignore[arg-type]


# ==================== daemon 读 cfg ====================
class TestDaemonReadsCfg:
    """W5: ``SnowLumaDaemon.ensure_running(override=None)`` 默认从
    ``cfg.snowluma_webui_password_override`` 读取.
    """

    def test_read_cfg_helper_returns_empty_on_import_failure(
        self, monkeypatch
    ) -> None:
        """``_read_cfg_snowluma_override`` 在 cfg import 失败时静默返回 ""."""
        import src.core.runtime.snowluma_daemon as daemon_module

        # 模拟 import 失败
        def _raise_import_error(*args, **kwargs):
            raise ImportError("cfg unavailable")

        # 直接 monkeypatch 函数本体里 import cfg 的访问
        monkeypatch.setitem(
            __import__("sys").modules, "src.core.config", None
        )
        result = daemon_module._read_cfg_snowluma_override()
        assert result == ""

    def test_render_daemon_globals_with_override_does_not_touch_cfg(
        self, tmp_path: Path, isolated_session: Path
    ) -> None:
        """``render_daemon_globals(override="explicit")`` 应直接用 override, 不查 cfg."""
        from src.core.runtime.snowluma_daemon import render_daemon_globals

        snowluma_path = tmp_path / "SnowLuma"
        snowluma_path.mkdir()

        result = render_daemon_globals(snowluma_path, override="ExplicitP@ss")
        assert result == "ExplicitP@ss"
        # session 应**不**因 override 路径被创建
        assert not isolated_session.exists()
