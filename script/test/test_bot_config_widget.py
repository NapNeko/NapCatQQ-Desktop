# -*- coding: utf-8 -*-

# 标准库导入
import os

# 第三方库导入
from PySide6.QtWidgets import QApplication

# 项目内模块导入
import src.ui.page.bot_page.widget.config as config_widget_module
from src.core.config.config_model import AdvancedConfig, AutoRestartScheduleConfig, BypassConfig, BotConfig
from src.ui.page.bot_page.widget.config import AdvancedConfigWidget, BotConfigWidget


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


# ==================== P2.7: runtime_target 选择器 ====================
class _FakeServer:
    """ServerProfile 替身, 仅暴露 runtime_target 卡片读取的字段."""

    def __init__(self, server_id: str, name: str, host: str) -> None:
        self.id = server_id
        self.name = name

        class _Cred:
            def __init__(self, h: str) -> None:
                self.host = h

        self.credentials = _Cred(host)


class _FakeServerManager:
    def __init__(self, servers: list[_FakeServer]) -> None:
        self._servers = servers

    def list_servers(self) -> list[_FakeServer]:
        return list(self._servers)


def _patch_server_manager(monkeypatch, servers: list[_FakeServer]) -> None:
    fake = _FakeServerManager(servers)
    monkeypatch.setattr(
        config_widget_module,
        "it",
        lambda cls: fake,
    )


def test_runtime_target_card_defaults_to_local(monkeypatch) -> None:
    ensure_qapp()
    _patch_server_manager(monkeypatch, [])
    widget = config_widget_module.RuntimeTargetConfigCard(title="运行位置")
    assert widget.get_value() == "local"


def test_runtime_target_card_lists_servers(monkeypatch) -> None:
    ensure_qapp()
    servers = [
        _FakeServer("uuid-a", "线上A", "10.0.0.1"),
        _FakeServer("uuid-b", "测试B", "10.0.0.2"),
    ]
    _patch_server_manager(monkeypatch, servers)
    widget = config_widget_module.RuntimeTargetConfigCard(title="运行位置")
    assert widget.combo_box.count() == 3  # 本地 + 2 远端
    assert "uuid-a" in widget._target_ids
    assert "uuid-b" in widget._target_ids


def test_runtime_target_card_round_trip_remote(monkeypatch) -> None:
    ensure_qapp()
    servers = [_FakeServer("uuid-a", "线上A", "10.0.0.1")]
    _patch_server_manager(monkeypatch, servers)
    widget = config_widget_module.RuntimeTargetConfigCard(title="运行位置")

    widget.fill_value("uuid-a")
    assert widget.get_value() == "uuid-a"


def test_runtime_target_card_handles_deleted_server(monkeypatch) -> None:
    """配置引用的 server_id 已被删除时, 应在下拉中显示占位项, 而不是静默回退."""
    ensure_qapp()
    _patch_server_manager(monkeypatch, [])
    widget = config_widget_module.RuntimeTargetConfigCard(title="运行位置")

    widget.fill_value("ghost-id")
    # 仍能正确返回原值, 让保存时不丢失绑定
    assert widget.get_value() == "ghost-id"
    # 下拉项数量从 1(local) 增加到 2(local + 占位)
    assert widget.combo_box.count() == 2


def test_bot_config_widget_round_trips_runtime_target(monkeypatch) -> None:
    """``BotConfigWidget`` 应在 fill / get 之间保留 runtime_target."""
    ensure_qapp()
    servers = [_FakeServer("uuid-a", "线上A", "10.0.0.1")]
    _patch_server_manager(monkeypatch, servers)

    widget = BotConfigWidget()
    bot = BotConfig(
        name="RemoteBot",
        QQID=1145141919,
        musicSignUrl="",
        autoRestartSchedule=AutoRestartScheduleConfig(enable=False, time_unit="h", duration=1),
        offlineAutoRestart=False,
        runtime_target="uuid-a",
    )
    widget.fill_config(bot)
    restored = widget.get_config()
    assert restored.runtime_target == "uuid-a"


def test_bot_config_widget_clear_resets_runtime_target_to_local(monkeypatch) -> None:
    ensure_qapp()
    servers = [_FakeServer("uuid-a", "线上A", "10.0.0.1")]
    _patch_server_manager(monkeypatch, servers)

    widget = BotConfigWidget()
    widget.fill_config(
        BotConfig(
            name="x",
            QQID=1145141919,
            musicSignUrl="",
            autoRestartSchedule=AutoRestartScheduleConfig(enable=False, time_unit="h", duration=1),
            offlineAutoRestart=False,
            runtime_target="uuid-a",
        )
    )
    widget.clear_config()
    assert widget.runtime_target_card.get_value() == "local"
