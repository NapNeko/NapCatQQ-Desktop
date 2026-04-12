# -*- coding: utf-8 -*-

# 标准库导入
import os

# 第三方库导入
from PySide6.QtWidgets import QApplication

# 项目内模块导入
import src.desktop.ui.page.bot_page.widget.config as config_widget_module
from src.desktop.core.config.config_model import AdvancedConfig, AutoRestartScheduleConfig, BypassConfig, BotConfig
from src.desktop.ui.page.bot_page.widget.config import AdvancedConfigWidget, BotConfigWidget


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


def test_advanced_config_widget_round_trips_grouped_and_dialog_fields() -> None:
    """高级配置页应保留分组主项和底层对话框项。"""
    ensure_qapp()
    widget = AdvancedConfigWidget()
    config = AdvancedConfig(
        autoStart=True,
        offlineNotice=True,
        parseMultMsg=True,
        packetServer="ws://127.0.0.1:3001",
        packetBackend="disable",
        enableLocalFile2Url=True,
        fileLog=True,
        consoleLog=False,
        fileLogLevel="info",
        consoleLogLevel="error",
        o3HookMode=0,
        bypass=BypassConfig(hook=True, module=True, js=True),
    )

    widget.fill_config(config)
    restored = widget.get_config()

    assert restored.autoStart is True
    assert restored.offlineNotice is True
    assert restored.parseMultMsg is True
    assert restored.enableLocalFile2Url is True
    assert restored.packetServer == "ws://127.0.0.1:3001"
    assert restored.packetBackend == "disable"
    assert restored.o3HookMode == 0
    assert restored.bypass.hook is True
    assert restored.bypass.module is True
    assert restored.bypass.js is True
    assert restored.bypass.window is False
    assert restored.fileLogLevel == "info"
    assert restored.consoleLogLevel == "error"
    assert widget.file_log_level_card.isEnabled() is True
    assert widget.console_level_card.isEnabled() is False


def test_advanced_config_widget_clear_resets_backend_and_log_state(monkeypatch) -> None:
    """清空高级配置时应恢复默认值并关闭低频开关。"""
    ensure_qapp()
    monkeypatch.setattr(config_widget_module.cfg, "get", lambda item: False)
    widget = AdvancedConfigWidget()

    widget.fill_config(
        AdvancedConfig(
            fileLog=True,
            consoleLog=True,
            packetBackend="disable",
            packetServer="ws://example.com",
            bypass=BypassConfig(hook=True, window=True),
        )
    )
    widget.clear_config()
    restored = widget.get_config()

    assert restored.packetBackend == "auto"
    assert restored.packetServer == ""
    assert restored.o3HookMode == 1
    assert restored.bypass == BypassConfig()
    assert widget.file_log_level_card.isEnabled() is False
    assert widget.console_level_card.isEnabled() is False
    assert restored.offlineNotice is False


def test_advanced_config_widget_clear_enables_offline_notice_when_global_notice_enabled(monkeypatch) -> None:
    """新增 Bot 时若全局邮件或 WebHook 通知已开启，应默认勾选单独掉线通知。"""
    ensure_qapp()
    monkeypatch.setattr(
        config_widget_module.cfg,
        "get",
        lambda item: item in {
            config_widget_module.cfg.bot_offline_email_notice,
            config_widget_module.cfg.bot_offline_web_hook_notice,
        },
    )
    widget = AdvancedConfigWidget()

    widget.clear_config()
    restored = widget.get_config()

    assert restored.offlineNotice is True
