# -*- coding: utf-8 -*-

# 标准库导入
import importlib.util
import os
import sys
from pathlib import Path
from types import ModuleType, SimpleNamespace

# 第三方库导入
import pytest
from PySide6.QtWidgets import QApplication, QWidget

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
sys.modules.setdefault("qrcode", ModuleType("qrcode"))


def load_card_module():
    """按文件路径加载 card 模块，避免触发页面包的全量导入。"""
    project_root = Path(__file__).resolve().parents[2]
    module_name = "src.ui.page.bot_page.widget.card"

    page_package = ModuleType("src.ui.page")
    page_package.__path__ = [str(project_root / "src" / "ui" / "page")]
    sys.modules["src.ui.page"] = page_package

    bot_page_package = ModuleType("src.ui.page.bot_page")
    bot_page_package.__path__ = [str(project_root / "src" / "ui" / "page" / "bot_page")]
    sys.modules["src.ui.page.bot_page"] = bot_page_package

    widget_package = ModuleType("src.ui.page.bot_page.widget")
    widget_package.__path__ = [str(project_root / "src" / "ui" / "page" / "bot_page" / "widget")]
    sys.modules["src.ui.page.bot_page.widget"] = widget_package

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


card_module = load_card_module()


def ensure_qapp() -> QApplication:
    """创建或复用测试用 QApplication。"""
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


def test_remove_button_blocks_running_bot(monkeypatch: pytest.MonkeyPatch, config_factory) -> None:
    """运行中的 Bot 应先停止，不能直接删除。"""
    ensure_qapp()
    warning_messages: list[str] = []
    stop_calls: list[str] = []
    login_state_removals: list[str] = []
    auto_restart_removals: list[str] = []
    emitted: list[str] = []

    fake_process_manager = SimpleNamespace(
        process_changed_signal=DummySignal(),
        get_process=lambda qq_id: object(),
        stop_process=lambda qq_id: stop_calls.append(qq_id),
    )
    fake_login_state_manager = SimpleNamespace(
        qr_code_available_signal=DummySignal(),
        qr_code_removed_signal=DummySignal(),
        remove_login_state=lambda qq_id: login_state_removals.append(qq_id),
    )
    fake_auto_restart_manager = SimpleNamespace(
        remove_auto_restart_timer=lambda qq_id: auto_restart_removals.append(qq_id)
    )
    fake_qr_code_factory = SimpleNamespace(has_qr_code=lambda qq_id: False)

    fake_main_window_module = ModuleType("src.ui.window.main_window.window")
    fake_main_window_module.MainWindow = type("MainWindow", (), {})
    sys.modules["src.ui.window.main_window.window"] = fake_main_window_module

    monkeypatch.setattr(card_module, "BotAvatarWidget", DummyAvatarWidget)
    monkeypatch.setattr(card_module, "BotInfoWidget", DummyInfoWidget)
    monkeypatch.setattr(card_module, "AskBox", ConfirmAskBox)
    monkeypatch.setattr(card_module, "warning_bar", lambda content, *args, **kwargs: warning_messages.append(content))
    monkeypatch.setattr(
        card_module,
        "it",
        lambda cls: {
            "ManagerNapCatQQProcess": fake_process_manager,
            "ManagerNapCatQQLoginState": fake_login_state_manager,
            "ManagerAutoRestartProcess": fake_auto_restart_manager,
            "QRCodeDialogFactory": fake_qr_code_factory,
            "MainWindow": object(),
        }[cls.__name__],
    )

    widget = card_module.BotCard(config_factory(2477817352))
    widget.remove_signal.connect(emitted.append)

    widget.slot_remove_button()

    assert warning_messages == ["请先停止正在运行的 Bot，再执行移除"]
    assert stop_calls == []
    assert login_state_removals == []
    assert auto_restart_removals == []
    assert emitted == []
