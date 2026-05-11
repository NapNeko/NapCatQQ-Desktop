# -*- coding: utf-8 -*-
"""W6 (2026-05-11): 设置页 → 常规 tab 的 ``SnowLuma`` 卡片组测试.

行为约束:

- SnowLuma **已安装** (``PathFunc.get_snowluma_node_executable()`` 非 None) →
  ``General`` widget 构造 ``snowluma_group`` + ``snowluma_password_override_card``,
  挂到 ``expand_layout`` 末位.
- SnowLuma **未安装** → ``snowluma_group`` / ``snowluma_password_override_card`` 均为
  ``None``, ``expand_layout`` 不包含该组 (用户看不到无意义的卡).
- 用户在输入框打字 → ``_on_snowluma_password_override_changed`` 实时同步到
  ``cfg.snowluma_webui_password_override`` (strip 后写盘).

参见:

- ``src/ui/page/setup_page/sub_page/general.py``
- ``docs/requirements/2026-05-11-snowluma-daemon-refactor.md`` §2.6
- ``docs/plans/2026-05-11-snowluma-daemon-refactor-execution-plan.md`` §W6
"""
from __future__ import annotations

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


@pytest.fixture(autouse=True)
def _reset_cfg_override():
    """每个 case 前后清空 cfg.snowluma_webui_password_override, 防 cross-test 污染."""
    from src.core.config import cfg

    cfg.set(cfg.snowluma_webui_password_override, "", True)
    yield
    cfg.set(cfg.snowluma_webui_password_override, "", True)


# ==================== SnowLuma 已安装 → group 可见 ====================
class TestSnowLumaInstalled:
    """``PathFunc.get_snowluma_node_executable()`` 返回 Path → group 应创建."""

    def test_group_and_card_created_when_node_exe_exists(
        self, tmp_path: Path, monkeypatch
    ) -> None:
        # mock node.exe 存在
        fake_node_exe = tmp_path / "node.exe"
        fake_node_exe.write_bytes(b"")

        from creart import it
        from src.core.runtime.paths import PathFunc

        path_func = it(PathFunc)
        monkeypatch.setattr(
            path_func, "get_snowluma_node_executable", lambda: fake_node_exe
        )

        from src.ui.page.setup_page.sub_page.general import General
        from PySide6.QtWidgets import QWidget

        parent = QWidget()
        general = General(parent)

        assert general.snowluma_group is not None, "SnowLuma group 应已创建"
        assert general.snowluma_password_override_card is not None, "密码卡应已创建"
        assert general.snowluma_password_override_card.parent() is general.snowluma_group, (
            "密码卡 parent 应是 SnowLuma group (用 addSettingCard 加入)"
        )

    def test_card_pre_fills_current_cfg_value(
        self, tmp_path: Path, monkeypatch
    ) -> None:
        """卡片创建时应反填 ``cfg.snowluma_webui_password_override`` 当前值."""
        fake_node_exe = tmp_path / "node.exe"
        fake_node_exe.write_bytes(b"")

        from creart import it
        from src.core.config import cfg
        from src.core.runtime.paths import PathFunc

        cfg.set(cfg.snowluma_webui_password_override, "PreExisting!", True)
        monkeypatch.setattr(
            it(PathFunc), "get_snowluma_node_executable", lambda: fake_node_exe
        )

        from src.ui.page.setup_page.sub_page.general import General
        from PySide6.QtWidgets import QWidget

        # 持 parent 引用避免 Qt 树过早 GC, 不然 LineEdit 等 C++ 子对象会被销毁
        self._parent_holder = QWidget()
        general = General(self._parent_holder)
        assert general.snowluma_password_override_card.get_value() == "PreExisting!"

    def test_text_change_writes_to_cfg(
        self, tmp_path: Path, monkeypatch
    ) -> None:
        """``_on_snowluma_password_override_changed`` 应同步到 cfg (含 strip)."""
        fake_node_exe = tmp_path / "node.exe"
        fake_node_exe.write_bytes(b"")

        from creart import it
        from src.core.config import cfg
        from src.core.runtime.paths import PathFunc

        monkeypatch.setattr(
            it(PathFunc), "get_snowluma_node_executable", lambda: fake_node_exe
        )

        from src.ui.page.setup_page.sub_page.general import General
        from PySide6.QtWidgets import QWidget

        general = General(QWidget())

        # 直接调槽 (textChanged 信号也会同样调用, 但 offscreen 下不一定跑 event loop)
        general._on_snowluma_password_override_changed("NewSecret!")
        assert cfg.get(cfg.snowluma_webui_password_override) == "NewSecret!"

        # 空白 strip
        general._on_snowluma_password_override_changed("   ")
        assert cfg.get(cfg.snowluma_webui_password_override) == ""


# ==================== SnowLuma 未安装 → group 缺席 ====================
class TestSnowLumaNotInstalled:
    """``PathFunc.get_snowluma_node_executable()`` 返回 None → group 不应创建."""

    def test_group_is_none_when_node_exe_missing(self, monkeypatch) -> None:
        from creart import it
        from src.core.runtime.paths import PathFunc

        monkeypatch.setattr(
            it(PathFunc), "get_snowluma_node_executable", lambda: None
        )

        from src.ui.page.setup_page.sub_page.general import General
        from PySide6.QtWidgets import QWidget

        general = General(QWidget())

        assert general.snowluma_group is None, "未装 SnowLuma 时 group 应保持 None"
        assert general.snowluma_password_override_card is None, "未装 SnowLuma 时密码卡应保持 None"
