# -*- coding: utf-8 -*-

# 标准库导入
import os
import sys
from types import ModuleType, SimpleNamespace

# 第三方库导入
import pytest
from PySide6.QtWidgets import QApplication, QWidget

# 项目内模块导入
from src.ui.page.bot_page.widget import card as card_module


def ensure_qapp() -> QApplication:
    """创建或复用测试用 QApplication。"""
    os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
    app = QApplication.instance()
    if app is None:
        app = QApplication([])
    return app


class DummySignal:
    """最小可用信号替身。"""

    def connect(self, *_args, **_kwargs) -> None:
        return None


class DummyAvatarWidget(QWidget):
    """避免测试中触发真实头像下载。"""

    def __init__(self, _qq_id: str, parent: QWidget | None = None) -> None:
        super().__init__(parent)


class DummyInfoWidget(QWidget):
    """避免测试中接入真实运行态更新。"""

    def __init__(self, _config, parent: QWidget | None = None) -> None:
        super().__init__(parent)


class ConfirmAskBox:
    """始终确认的弹窗替身。"""

    def __init__(self, *_args, **_kwargs) -> None:
        pass

    def exec(self) -> bool:
        return True


def test_remove_button_skips_stop_when_process_missing(monkeypatch: pytest.MonkeyPatch, config_factory) -> None:
    """删除未运行 Bot 时不应误调用 stop_process，但仍需清理其余运行态。"""
    ensure_qapp()
    stop_calls: list[str] = []
    login_state_removals: list[str] = []
    auto_restart_removals: list[str] = []
    emitted: list[str] = []

    fake_process_manager = SimpleNamespace(
        process_changed_signal=DummySignal(),
        get_process=lambda qq_id: None,
        stop_process=lambda qq_id: stop_calls.append(qq_id),
    )
    fake_login_state_manager = SimpleNamespace(remove_login_state=lambda qq_id: login_state_removals.append(qq_id))
    fake_auto_restart_manager = SimpleNamespace(
        remove_auto_restart_timer=lambda qq_id: auto_restart_removals.append(qq_id)
    )

    fake_main_window_module = ModuleType("src.ui.window.main_window.window")
    fake_main_window_module.MainWindow = type("MainWindow", (), {})
    monkeypatch.setitem(sys.modules, "src.ui.window.main_window.window", fake_main_window_module)

    monkeypatch.setattr(card_module, "BotAvatarWidget", DummyAvatarWidget)
    monkeypatch.setattr(card_module, "BotInfoWidget", DummyInfoWidget)
    monkeypatch.setattr(card_module, "AskBox", ConfirmAskBox)
    monkeypatch.setattr(
        card_module,
        "it",
        lambda cls: {
            "ManagerNapCatQQProcess": fake_process_manager,
            "ManagerNapCatQQLoginState": fake_login_state_manager,
            "ManagerAutoRestartProcess": fake_auto_restart_manager,
            "MainWindow": object(),
        }[cls.__name__],
    )

    widget = card_module.BotCard(config_factory(2477817352))
    widget.remove_signal.connect(emitted.append)

    widget.slot_remove_button()

    assert stop_calls == []
    assert login_state_removals == ["2477817352"]
    assert auto_restart_removals == ["2477817352"]
    assert emitted == ["2477817352"]
