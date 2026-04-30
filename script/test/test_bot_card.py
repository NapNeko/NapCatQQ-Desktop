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

# 项目内模块导入
sys.modules.setdefault("qrcode", ModuleType("qrcode"))
from src.core.config.config_model import HttpServersConfig, WebsocketClientsConfig


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
            "QRCodeDialogFactory": fake_qr_code_factory,
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


def test_tag_widget_renders_enabled_connect_types(config_factory) -> None:
    """网络标签应仅展示已配置的连接类型。"""
    ensure_qapp()
    config = config_factory(114514)
    config.connect.httpServers.append(
        HttpServersConfig(name="http", host="127.0.0.1", port=3000)
    )
    config.connect.websocketClients.append(
        WebsocketClientsConfig(name="wsc", url="ws://127.0.0.1:3001/ws")
    )

    parent = QWidget()
    widget = card_module.BotInfoWidget.TagWidget(config.connect, parent)

    assert widget.flow_layout.count() == 2
    assert widget.flow_layout.itemAt(0).widget().text() == "HTTPS"
    assert widget.flow_layout.itemAt(1).widget().text() == "WSC"


def test_qr_code_button_shows_for_pending_qr_and_opens_target_dialog(
    monkeypatch: pytest.MonkeyPatch, config_factory
) -> None:
    """卡片应仅在当前 Bot 存在二维码时展示入口，并打开对应 QQ 的弹窗。"""
    ensure_qapp()
    shown_qq_ids: list[str] = []

    fake_process_manager = SimpleNamespace(process_changed_signal=DummySignal())
    fake_login_state_manager = SimpleNamespace(
        qr_code_available_signal=DummySignal(),
        qr_code_removed_signal=DummySignal(),
    )
    fake_qr_code_factory = SimpleNamespace(
        has_qr_code=lambda qq_id: False,
        show=lambda qq_id=None: shown_qq_ids.append(qq_id),
    )

    monkeypatch.setattr(card_module, "BotAvatarWidget", DummyAvatarWidget)
    monkeypatch.setattr(card_module, "BotInfoWidget", DummyInfoWidget)
    monkeypatch.setattr(
        card_module,
        "it",
        lambda cls: {
            "ManagerNapCatQQProcess": fake_process_manager,
            "ManagerNapCatQQLoginState": fake_login_state_manager,
            "QRCodeDialogFactory": fake_qr_code_factory,
        }[cls.__name__],
    )

    widget = card_module.BotCard(config_factory(2477817352))

    assert widget.qr_code_button.isHidden() is True

    widget.slot_qr_code_available("2477817352", "https://example.com/qr")
    assert widget.qr_code_button.isHidden() is False

    widget.slot_qr_code_button()
    assert shown_qq_ids == ["2477817352"]

    widget.slot_qr_code_removed("2477817352")
    assert widget.qr_code_button.isHidden() is True
