# -*- coding: utf-8 -*-

# 标准库导入
import random

# 第三方库导入
import pytest

# 项目内模块导入
from src.core.config.config_enum import TimeUnitEnum
from src.core.config.config_model import (
    AutoRestartScheduleConfig,
    BotConfig,
    WebsocketClientsConfig,
    WebsocketServersConfig,
    _coerce_interval_default,
)


def test_coerce_interval_default_handles_none_blank_and_invalid_values() -> None:
    """间隔值规范化应为 None、空白和非法输入回退默认值。"""
    assert _coerce_interval_default(None, 123) == 123
    assert _coerce_interval_default("   ", 456) == 456
    assert _coerce_interval_default("abc", 789) == 789
    assert _coerce_interval_default("42", 100) == 42
    assert _coerce_interval_default(18, 100) == 18


def test_auto_restart_schedule_normalizes_legacy_interval_payload() -> None:
    """旧版 interval 调度配置应在模型层被转换为当前结构。"""
    schedule = AutoRestartScheduleConfig.model_validate({"taskType": "interval", "interval": "15m", "jitter": 0})

    assert schedule.enable is True
    assert schedule.time_unit == TimeUnitEnum.MINUTE
    assert schedule.duration == 15


def test_auto_restart_schedule_disables_legacy_crontab_payload() -> None:
    """旧版 crontab 调度配置应安全降级为禁用。"""
    schedule = AutoRestartScheduleConfig.model_validate(
        {"taskType": "crontab", "interval": "6h", "crontab": "0 4 * * *", "jitter": 0}
    )

    assert schedule.enable is False
    assert schedule.time_unit == TimeUnitEnum.HOUR
    assert schedule.duration == 6


def test_bot_config_generates_name_when_value_is_empty(monkeypatch: pytest.MonkeyPatch) -> None:
    """空名称应退回为随机生成的 8 位字母串。"""
    monkeypatch.setattr(random, "choices", lambda population, k: list("AbCdEfGh"))

    bot = BotConfig(name="", QQID="123456", autoRestartSchedule=AutoRestartScheduleConfig())

    assert bot.name == "AbCdEfGh"
    assert bot.QQID == 123456


def test_bot_config_rejects_invalid_qqid() -> None:
    """非法 QQID 应在模型层直接报错。"""
    with pytest.raises(ValueError, match="无法转换为整数"):
        BotConfig(name="demo", QQID="not-a-number", autoRestartSchedule=AutoRestartScheduleConfig())


def test_websocket_configs_coerce_blank_intervals_to_defaults() -> None:
    """WebSocket 相关配置中的空白间隔应回退为默认值。"""
    server = WebsocketServersConfig(name="server", host="127.0.0.1", port=8080, heartInterval=" ")
    client = WebsocketClientsConfig(
        name="client",
        url="ws://127.0.0.1:3000/ws",
        heartInterval="",
        reconnectInterval="invalid",
    )

    assert server.heartInterval == 30000
    assert client.heartInterval == 30000
    assert client.reconnectInterval == 30000
