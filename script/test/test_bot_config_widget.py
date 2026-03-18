# -*- coding: utf-8 -*-

# 标准库导入
import os

# 第三方库导入
from PySide6.QtWidgets import QApplication

# 项目内模块导入
from src.core.config.config_model import AutoRestartScheduleConfig, BotConfig
from src.ui.page.bot_page.widget.config import BotConfigWidget


def ensure_qapp() -> QApplication:
    """创建或复用测试用 QApplication。"""
    os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
    app = QApplication.instance()
    if app is None:
        app = QApplication([])
    return app


def make_bot_config() -> BotConfig:
    """构造测试用 Bot 配置。"""
    return BotConfig(
        name="ExistingBot",
        QQID=2477817352,
        musicSignUrl="https://example.com/music",
        autoRestartSchedule=AutoRestartScheduleConfig(enable=True, time_unit="h", duration=3),
        offlineAutoRestart=False,
    )


def test_bot_qqid_card_is_enabled_after_clear() -> None:
    """新建模式下 QQID 输入框应可编辑。"""
    ensure_qapp()
    widget = BotConfigWidget()

    widget.clear_config()

    assert widget.bot_qq_id_card.isEnabled() is True
    assert widget.bot_qq_id_card.lineEdit.isEnabled() is True


def test_bot_qqid_card_is_disabled_when_editing_existing_bot() -> None:
    """编辑已有 Bot 时 QQID 输入框应被锁定。"""
    ensure_qapp()
    widget = BotConfigWidget()

    widget.fill_config(make_bot_config())

    assert widget.bot_qq_id_card.isEnabled() is False
    assert widget.bot_qq_id_card.lineEdit.isEnabled() is False


def test_bot_qqid_card_is_reenabled_after_returning_to_new_mode() -> None:
    """从编辑模式切回新建模式后 QQID 输入框应重新可编辑。"""
    ensure_qapp()
    widget = BotConfigWidget()

    widget.fill_config(make_bot_config())
    widget.clear_config()

    assert widget.bot_qq_id_card.isEnabled() is True
    assert widget.bot_qq_id_card.lineEdit.isEnabled() is True
