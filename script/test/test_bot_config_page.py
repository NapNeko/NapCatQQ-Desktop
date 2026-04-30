# -*- coding: utf-8 -*-

# 标准库导入
import os
import sys
from types import ModuleType

# 第三方库导入
from PySide6.QtWidgets import QApplication

sys.modules.setdefault("qrcode", ModuleType("qrcode"))
markdown_module = ModuleType("markdown")
markdown_module.markdown = lambda text, *args, **kwargs: text
sys.modules.setdefault("markdown", markdown_module)

# 项目内模块导入
import src.ui.page.bot_page.sub_page.bot_config as bot_config_module
from src.core.config.config_model import (
    AdvancedConfig,
    BotConfig,
    Config,
    ConnectConfig,
    HttpClientsConfig,
    HttpServersConfig,
    WebsocketClientsConfig,
    WebsocketServersConfig,
)
from src.ui.page.bot_page.sub_page.bot_config import ConfigPage


def ensure_qapp() -> QApplication:
    """创建或复用测试用 QApplication。"""
    os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
    app = QApplication.instance()
    if app is None:
        app = QApplication([])
    return app


def test_save_invalid_config_shows_error_without_calling_update(monkeypatch) -> None:
    """顶层配置校验失败时应提示错误，而不是继续保存。"""
    ensure_qapp()
    page = ConfigPage()
    captured: dict[str, object] = {}

    def fake_update_config(_config, *args, **kwargs) -> bool:
        captured["update_called"] = True
        return True

    def fake_error_bar(content: str, *args, **kwargs) -> None:
        captured["error"] = content

    monkeypatch.setattr(bot_config_module, "update_config", fake_update_config)
    monkeypatch.setattr(bot_config_module, "merge_config_for_update", lambda config, base_config=None: config)
    monkeypatch.setattr(bot_config_module, "error_bar", fake_error_bar)
    monkeypatch.setattr(bot_config_module.logger, "warning", lambda *args, **kwargs: None)

    page.bot_widget.bot_qq_id_card.fill_value("not-a-number")
    page.slot_save_config_button()

    assert captured.get("update_called") is not True
    assert captured.get("error") == "配置校验失败，请检查输入内容"

    page.close()


def test_save_duplicate_connect_names_shows_error_without_calling_update(monkeypatch) -> None:
    """连接配置名称重复时应阻止保存。"""
    ensure_qapp()
    page = ConfigPage()
    captured: dict[str, object] = {}

    def fake_update_config(_config, *args, **kwargs) -> bool:
        captured["update_called"] = True
        return True

    def fake_error_bar(content: str, *args, **kwargs) -> None:
        captured["error"] = content

    monkeypatch.setattr(bot_config_module, "update_config", fake_update_config)
    monkeypatch.setattr(bot_config_module, "merge_config_for_update", lambda config, base_config=None: config)
    monkeypatch.setattr(bot_config_module, "error_bar", fake_error_bar)
    monkeypatch.setattr(bot_config_module.logger, "warning", lambda *args, **kwargs: None)

    page.connect_widget.add_card(HttpServersConfig(name="shared", host="127.0.0.1", port=3000))
    page.connect_widget.add_card(HttpClientsConfig(name="shared", url="https://127.0.0.1:3001"))
    page.slot_save_config_button()

    assert captured.get("update_called") is not True
    assert captured.get("error") == "配置校验失败，请检查输入内容"

    page.close()


def test_fill_and_save_config_preserves_websocket_entries(monkeypatch) -> None:
    """整页填充已有配置后直接保存，不应丢失 WebSocket 连接项。"""
    ensure_qapp()
    page = ConfigPage()
    captured: dict[str, object] = {}

    config = Config(
        bot=BotConfig(name="WebsocketBot", QQID=22334455, musicSignUrl=""),
        connect=ConnectConfig(
            httpServers=[],
            httpSseServers=[],
            httpClients=[],
            websocketServers=[
                WebsocketServersConfig(
                    name="wss-main",
                    host="127.0.0.1",
                    port=3001,
                    reportSelfMessage=True,
                    enableForcePushEvent=True,
                    heartInterval=45000,
                )
            ],
            websocketClients=[
                WebsocketClientsConfig(
                    name="wsc-main",
                    url="ws://127.0.0.1:3002/ws",
                    reportSelfMessage=True,
                    heartInterval=46000,
                    reconnectInterval=47000,
                )
            ],
            plugins=[],
        ),
        advanced=AdvancedConfig(),
    )

    def fake_update_config(saved_config, *args, **kwargs) -> bool:
        captured["config"] = saved_config
        return True

    monkeypatch.setattr(bot_config_module, "update_config", fake_update_config)
    monkeypatch.setattr(bot_config_module, "merge_config_for_update", lambda config, base_config=None: config)
    monkeypatch.setattr(
        bot_config_module,
        "it",
        lambda cls: type("FakeBotPage", (), {"bot_list_page": type("FakeListPage", (), {"update_bot_list": lambda self: None})()})(),
    )
    monkeypatch.setattr(bot_config_module, "success_bar", lambda *args, **kwargs: None)
    monkeypatch.setattr(bot_config_module.logger, "info", lambda *args, **kwargs: None)

    page.fill_config(config)
    page.slot_save_config_button()

    saved_config = captured["config"]
    assert len(saved_config.connect.websocketServers) == 1
    assert len(saved_config.connect.websocketClients) == 1
    assert saved_config.connect.websocketServers[0].name == "wss-main"
    assert saved_config.connect.websocketServers[0].port == 3001
    assert saved_config.connect.websocketClients[0].name == "wsc-main"
    assert saved_config.connect.websocketClients[0].url.unicode_string() == "ws://127.0.0.1:3002/ws"

    page.close()
