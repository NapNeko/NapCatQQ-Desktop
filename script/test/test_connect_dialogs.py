# -*- coding: utf-8 -*-

# 标准库导入
import os

# 第三方库导入
import pytest
from PySide6.QtWidgets import QApplication, QWidget

# 项目内模块导入
import src.desktop.ui.page.bot_page.widget.msg_box as msg_box_module
from src.desktop.ui.page.bot_page.widget.msg_box import (
    HttpServerConfigDialog,
    WebsocketClientConfigDialog,
    WebsocketServerConfigDialog,
)


def ensure_qapp() -> QApplication:
    """创建或复用测试用 QApplication。"""
    os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
    app = QApplication.instance()
    if app is None:
        app = QApplication([])
    return app


def create_dialog_parent() -> QWidget:
    """创建供 MessageBoxBase 使用的父级窗口。"""
    ensure_qapp()
    parent = QWidget()
    parent.resize(800, 600)
    return parent


def test_http_server_dialog_rejects_invalid_port() -> None:
    """HTTP Server 对话框不应吞掉无效端口。"""
    parent = create_dialog_parent()
    dialog = HttpServerConfigDialog(parent)
    dialog.port_card.fill_value("not-a-port")

    with pytest.raises(ValueError, match="Port"):
        dialog.get_config()


def test_websocket_server_dialog_rejects_invalid_heart_interval() -> None:
    """WebSocket Server 对话框应明确拒绝非法心跳间隔。"""
    parent = create_dialog_parent()
    dialog = WebsocketServerConfigDialog(parent)
    dialog.port_card.fill_value("3000")
    dialog.heart_interval_card.fill_value("oops")

    with pytest.raises(ValueError, match="心跳间隔"):
        dialog.get_config()


def test_websocket_client_dialog_blank_intervals_fall_back_to_defaults() -> None:
    """WebSocket Client 的空白间隔输入应回落到默认值。"""
    parent = create_dialog_parent()
    dialog = WebsocketClientConfigDialog(parent)
    dialog.url_card.fill_value("ws://localhost:8080")
    dialog.heart_interval_card.clear()
    dialog.reconnect_interval_card.clear()

    config = dialog.get_config()

    assert config.heartInterval == 30000
    assert config.reconnectInterval == 30000


def test_connect_dialog_accept_shows_error_bar_without_closing(monkeypatch) -> None:
    """字段校验失败时应弹出错误提示，并保留对话框内容。"""
    parent = create_dialog_parent()
    dialog = HttpServerConfigDialog(parent)
    captured: dict[str, str] = {}

    def fake_error_bar(content: str, *args, **kwargs) -> None:
        captured["error"] = content

    monkeypatch.setattr(msg_box_module, "error_bar", fake_error_bar)
    dialog.port_card.fill_value("bad-port")
    dialog.accept()

    assert "Port" in captured.get("error", "")
    assert dialog.result() == 0
    assert dialog.title_label.text() == "HTTP Server"


def test_connect_dialog_duplicate_name_shows_error_bar_without_closing(monkeypatch) -> None:
    """名称冲突应在对话框内提示，而不是关闭后才失败。"""
    parent = create_dialog_parent()
    dialog = HttpServerConfigDialog(parent)
    captured: dict[str, str] = {}

    def fake_error_bar(content: str, *args, **kwargs) -> None:
        captured["error"] = content

    monkeypatch.setattr(msg_box_module, "error_bar", fake_error_bar)
    dialog.set_name_conflict_validator(lambda _name: "连接配置名称不能重复")
    dialog.name_card.fill_value("duplicate")
    dialog.host_card.fill_value("127.0.0.1")
    dialog.port_card.fill_value("3000")
    dialog.accept()

    assert captured.get("error") == "连接配置名称不能重复"
    assert dialog.result() == 0
    assert dialog.name_card.get_value() == "duplicate"
