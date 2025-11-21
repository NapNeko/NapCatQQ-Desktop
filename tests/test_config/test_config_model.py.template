# -*- coding: utf-8 -*-
"""测试配置模型 (BotConfig, NetworkBaseConfig 等)"""
# 标准库导入
import pytest
from pydantic import ValidationError

# 项目内模块导入
from src.core.config.config_enum import TimeUnitEnum
from src.core.config.config_model import (
    AutoRestartScheduleConfig,
    BotConfig,
    HttpServersConfig,
    NetworkBaseConfig,
    WebsocketServersConfig,
    _coerce_interval_default,
)


class TestCoerceIntervalDefault:
    """测试 _coerce_interval_default 辅助函数"""

    def test_none_returns_default(self):
        """测试 None 返回默认值"""
        assert _coerce_interval_default(None, 30000) == 30000
        assert _coerce_interval_default(None, 5000) == 5000

    def test_empty_string_returns_default(self):
        """测试空字符串返回默认值"""
        assert _coerce_interval_default("", 30000) == 30000
        assert _coerce_interval_default("   ", 30000) == 30000

    def test_numeric_string_converts_to_int(self):
        """测试数字字符串转换为整数"""
        assert _coerce_interval_default("12345", 30000) == 12345
        assert _coerce_interval_default(" 999 ", 30000) == 999

    def test_integer_returns_as_is(self):
        """测试整数直接返回"""
        assert _coerce_interval_default(42000, 30000) == 42000
        assert _coerce_interval_default(0, 30000) == 0

    def test_invalid_value_returns_default(self):
        """测试无效值返回默认值"""
        assert _coerce_interval_default("abc", 30000) == 30000
        assert _coerce_interval_default([], 30000) == 30000


class TestAutoRestartScheduleConfig:
    """测试自动重启计划配置"""

    def test_default_values(self):
        """测试默认值"""
        config = AutoRestartScheduleConfig()
        assert config.enable is False
        assert config.time_unit == TimeUnitEnum.HOUR
        assert config.duration == 6

    def test_custom_values(self):
        """测试自定义值"""
        config = AutoRestartScheduleConfig(
            enable=True,
            time_unit=TimeUnitEnum.DAY,
            duration=3
        )
        assert config.enable is True
        assert config.time_unit == TimeUnitEnum.DAY
        assert config.duration == 3


class TestBotConfig:
    """测试机器人配置模型"""

    def test_valid_bot_config(self):
        """测试有效的机器人配置"""
        config = BotConfig(
            name="TestBot",
            QQID="123456789",
            autoRestartSchedule=AutoRestartScheduleConfig()
        )
        assert config.name == "TestBot"
        assert config.QQID == 123456789  # 验证器应该转换为 int
        assert config.musicSignUrl == ""
        assert config.offlineAutoRestart is False

    def test_empty_name_generates_random(self):
        """测试空名称生成随机名称"""
        config = BotConfig(
            name="",
            QQID=123456789,
            autoRestartSchedule=AutoRestartScheduleConfig()
        )
        assert len(config.name) == 8
        assert config.name.isalpha()

    def test_qqid_string_to_int_conversion(self):
        """测试 QQID 字符串转整数"""
        config = BotConfig(
            name="TestBot",
            QQID="987654321",
            autoRestartSchedule=AutoRestartScheduleConfig()
        )
        assert config.QQID == 987654321
        assert isinstance(config.QQID, int)

    def test_qqid_int_passes_through(self):
        """测试 QQID 整数直接通过"""
        config = BotConfig(
            name="TestBot",
            QQID=123456789,
            autoRestartSchedule=AutoRestartScheduleConfig()
        )
        assert config.QQID == 123456789
        assert isinstance(config.QQID, int)

    def test_invalid_qqid_raises_error(self):
        """测试无效的 QQID 抛出错误"""
        with pytest.raises(ValidationError) as exc_info:
            BotConfig(
                name="TestBot",
                QQID="invalid_qq",
                autoRestartSchedule=AutoRestartScheduleConfig()
            )
        assert "无法转换为整数" in str(exc_info.value)


class TestNetworkBaseConfig:
    """测试网络基础配置"""

    def test_default_values(self):
        """测试默认值"""
        config = NetworkBaseConfig(name="test")
        assert config.enable is True
        assert config.name == "test"
        assert config.messagePostFormat == "array"
        assert config.token == ""
        assert config.debug is False

    def test_custom_values(self):
        """测试自定义值"""
        config = NetworkBaseConfig(
            name="custom",
            enable=False,
            messagePostFormat="string",
            token="secret123",
            debug=True
        )
        assert config.enable is False
        assert config.messagePostFormat == "string"
        assert config.token == "secret123"
        assert config.debug is True


class TestHttpServersConfig:
    """测试 HTTP 服务器配置"""

    def test_valid_http_server_config(self):
        """测试有效的 HTTP 服务器配置"""
        config = HttpServersConfig(
            name="http-server",
            host="127.0.0.1",
            port=3000
        )
        assert config.name == "http-server"
        assert config.host == "127.0.0.1"
        assert config.port == 3000
        assert config.enableCors is False
        assert config.enableWebsocket is False

    def test_with_cors_and_websocket(self):
        """测试启用 CORS 和 WebSocket"""
        config = HttpServersConfig(
            name="http-server",
            host="0.0.0.0",
            port=8080,
            enableCors=True,
            enableWebsocket=True
        )
        assert config.enableCors is True
        assert config.enableWebsocket is True


class TestWebsocketServersConfig:
    """测试 WebSocket 服务器配置"""

    def test_default_intervals(self):
        """测试默认心跳间隔"""
        config = WebsocketServersConfig(
            name="ws-server",
            host="127.0.0.1",
            port=8080
        )
        assert config.heartInterval == 30000

    def test_heart_interval_coercion_from_string(self):
        """测试心跳间隔从字符串转换"""
        config = WebsocketServersConfig(
            name="ws-server",
            host="127.0.0.1",
            port=8080,
            heartInterval="60000"
        )
        assert config.heartInterval == 60000

    def test_heart_interval_empty_string_uses_default(self):
        """测试空字符串使用默认值"""
        config = WebsocketServersConfig(
            name="ws-server",
            host="127.0.0.1",
            port=8080,
            heartInterval=""
        )
        assert config.heartInterval == 30000

    def test_heart_interval_none_uses_default(self):
        """测试 None 使用默认值"""
        config = WebsocketServersConfig(
            name="ws-server",
            host="127.0.0.1",
            port=8080,
            heartInterval=None
        )
        assert config.heartInterval == 30000
