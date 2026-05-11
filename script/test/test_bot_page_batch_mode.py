# -*- coding: utf-8 -*-
"""[`BotCard.set_batch_mode`](src/ui/page/bot_page/widget/card.py) 批量模式 UI 测试 (P4 W1·F2).

仅测试 BotCard 单卡 + 批量复选框态; BotListPage 工具条逻辑通过
[`BatchDispatcher`](src/core/operation/batch_dispatcher.py) 单测覆盖.

照搬 [`test_bot_card_starting_state.py`](script/test/test_bot_card_starting_state.py)
的旁路加载技巧, 避免触发 BotPage 全量 creart 链路.
"""
from __future__ import annotations

# 标准库导入
import importlib.util
import os
import sys
from pathlib import Path
from types import ModuleType, SimpleNamespace

# 第三方库导入
import pytest
from PySide6.QtWidgets import QApplication, QWidget


sys.modules.setdefault("qrcode", ModuleType("qrcode"))


def load_card_module():
    """旁路加载 card 模块, 避免触发 BotPage / creart 链.

    幂等约定: 已存在的真实 ``src.ui.page`` 模块保留不动 (避免破坏 MainWindow
    的 ``from src.ui.page import ApiDebugPage`` 路径).
    """
    project_root = Path(__file__).resolve().parents[2]
    module_name = "src.ui.page.bot_page.widget.card"

    def _ensure_namespace(name: str, path: Path) -> None:
        existing = sys.modules.get(name)
        if existing is not None and getattr(existing, "__file__", None):
            return
        package = ModuleType(name)
        package.__path__ = [str(path)]
        sys.modules[name] = package

    _ensure_namespace("src.ui.page", project_root / "src" / "ui" / "page")
    _ensure_namespace(
        "src.ui.page.bot_page", project_root / "src" / "ui" / "page" / "bot_page"
    )
    _ensure_namespace(
        "src.ui.page.bot_page.widget",
        project_root / "src" / "ui" / "page" / "bot_page" / "widget",
    )

    sys.modules.pop(module_name, None)
    spec = importlib.util.spec_from_file_location(
        module_name,
        project_root / "src" / "ui" / "page" / "bot_page" / "widget" / "card.py",
    )
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    sys.modules[module_name] = module
    spec.loader.exec_module(module)
    return module


# P4 W1: 懒加载, 避免 collection 阶段污染 src.ui.page namespace
_card_module_cache = None


def _get_card_module():
    global _card_module_cache
    if _card_module_cache is None:
        _card_module_cache = load_card_module()
    return _card_module_cache


def ensure_qapp() -> QApplication:
    os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
    app = QApplication.instance()
    if app is None:
        app = QApplication([])
    return app


class DummySignal:
    def connect(self, *_a, **_k) -> None: ...


class DummyAvatarWidget(QWidget):
    def __init__(self, _qq_id: str, parent: QWidget | None = None) -> None:
        super().__init__(parent)


class DummyInfoWidget(QWidget):
    def __init__(self, _config, parent: QWidget | None = None) -> None:  # noqa: ANN001
        super().__init__(parent)


class _FakeQRCodeDialogFactory:
    """模拟 ``QRCodeDialogFactory``; 用真实类避免 SimpleNamespace 丢失 ``__name__``."""

    @staticmethod
    def has_qr_code(qq_id: str) -> bool:
        del qq_id
        return False


def _make_card(monkeypatch: pytest.MonkeyPatch, config) -> object:
    ensure_qapp()
    card_module = _get_card_module()
    fake_process_manager = SimpleNamespace(process_changed_signal=DummySignal())
    fake_login_state_manager = SimpleNamespace(
        qr_code_available_signal=DummySignal(),
        qr_code_removed_signal=DummySignal(),
    )
    fake_qr_code_factory = _FakeQRCodeDialogFactory()

    monkeypatch.setattr(card_module, "BotAvatarWidget", DummyAvatarWidget)
    monkeypatch.setattr(card_module, "BotInfoWidget", DummyInfoWidget)
    monkeypatch.setattr(card_module, "QRCodeDialogFactory", _FakeQRCodeDialogFactory)

    def fake_it(target):
        name = getattr(target, "__name__", "")
        if name == "BotProcessManager":
            return fake_process_manager
        if name == "ManagerNapCatQQLoginState":
            return fake_login_state_manager
        if name == "_FakeQRCodeDialogFactory":
            return fake_qr_code_factory
        return SimpleNamespace()

    monkeypatch.setattr(card_module, "it", fake_it)

    return card_module.BotCard(config)


def _fake_config(qq_id: str = "12345") -> SimpleNamespace:
    """构造一个最小够用的 Config 形状."""
    return SimpleNamespace(
        bot=SimpleNamespace(QQID=qq_id, name=f"Bot-{qq_id}"),
        connect=SimpleNamespace(
            httpClients=[], httpServers=[], httpSseServers=[],
            websocketClients=[], websocketServers=[],
        ),
        advanced=SimpleNamespace(autoStart=False),
    )


# ==================== 测试 ====================
def test_batch_check_hidden_by_default(monkeypatch: pytest.MonkeyPatch) -> None:
    card = _make_card(monkeypatch, _fake_config())
    assert card.batch_check.isVisible() is False
    assert card.is_batch_mode() is False
    assert card.is_batch_selected() is False


def test_set_batch_mode_shows_checkbox(monkeypatch: pytest.MonkeyPatch) -> None:
    card = _make_card(monkeypatch, _fake_config())
    card.show()
    card.set_batch_mode(True)
    assert card.is_batch_mode() is True
    assert card.batch_check.isVisible() is True
    assert card.batch_check.isChecked() is False


def test_set_batch_mode_with_initial_check(monkeypatch: pytest.MonkeyPatch) -> None:
    card = _make_card(monkeypatch, _fake_config())
    card.show()
    card.set_batch_mode(True, checked=True)
    assert card.is_batch_selected() is True


def test_set_batch_mode_off_clears_check(monkeypatch: pytest.MonkeyPatch) -> None:
    card = _make_card(monkeypatch, _fake_config())
    card.show()
    card.set_batch_mode(True, checked=True)
    assert card.is_batch_selected() is True
    card.set_batch_mode(False)
    assert card.is_batch_mode() is False
    assert card.batch_check.isChecked() is False
    assert card.batch_check.isVisible() is False


def test_batch_check_toggle_emits_signal(monkeypatch: pytest.MonkeyPatch) -> None:
    card = _make_card(monkeypatch, _fake_config(qq_id="99999"))
    card.show()
    card.set_batch_mode(True)

    captured: list[tuple[str, bool]] = []
    card.batch_check_changed_signal.connect(
        lambda qq_id, checked: captured.append((qq_id, checked))
    )

    card.batch_check.setChecked(True)
    card.batch_check.setChecked(False)

    assert captured == [("99999", True), ("99999", False)]


def test_set_batch_mode_initial_check_does_not_emit_signal(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """``set_batch_mode(True, checked=True)`` 触发的 setChecked 不应回声."""
    card = _make_card(monkeypatch, _fake_config())
    card.show()

    captured: list[tuple[str, bool]] = []
    card.batch_check_changed_signal.connect(
        lambda qq_id, checked: captured.append((qq_id, checked))
    )

    card.set_batch_mode(True, checked=True)
    assert captured == []
