# -*- coding: utf-8 -*-

# 标准库导入
import os

# 第三方库导入
from PySide6.QtWidgets import QApplication

# 项目内模块导入
import src.ui.page.bot_page.sub_page.bot_config as bot_config_module
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

    def fake_update_config(_config) -> bool:
        captured["update_called"] = True
        return True

    def fake_error_bar(content: str, *args, **kwargs) -> None:
        captured["error"] = content

    monkeypatch.setattr(bot_config_module, "update_config", fake_update_config)
    monkeypatch.setattr(bot_config_module, "error_bar", fake_error_bar)
    monkeypatch.setattr(bot_config_module.logger, "warning", lambda *args, **kwargs: None)

    page.bot_widget.bot_qq_id_card.fill_value("not-a-number")
    page.slot_save_config_button()

    assert captured.get("update_called") is not True
    assert captured.get("error") == "配置校验失败，请检查输入内容"

    page.close()
