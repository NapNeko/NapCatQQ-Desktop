# -*- coding: utf-8 -*-
"""测试配置模型中的辅助函数（独立实现版本）"""
# 标准库导入
import random
import string
from enum import Enum

# 第三方库导入
import pytest
from pydantic import BaseModel, Field, field_validator


# 复制 TimeUnitEnum
class TimeUnitEnum(str, Enum):
    """时间单位枚举"""
    MINUTE = "m"
    HOUR = "h"
    DAY = "d"
    MONTH = "mon"
    YEAR = "year"


# 复制辅助函数
def _coerce_interval_default(v, default: int = 30000) -> int:
    """将可能来自配置的间隔值规范化为整数，无法解析时返回默认值。"""
    if v is None:
        return default
    if isinstance(v, str):
        val = v.strip()
        if val == "":
            return default
        try:
            return int(val)
        except ValueError:
            return default
    try:
        return int(v)
    except (TypeError, ValueError):
        return default


# 复制配置模型
class AutoRestartScheduleConfig(BaseModel):
    """自动重启计划配置"""
    enable: bool = Field(default=False, description="是否启用自动重启计划任务")
    time_unit: TimeUnitEnum = Field(default=TimeUnitEnum.HOUR, description="时间单位")
    duration: int = Field(default=6, description="时间长度, 仅包含数字, 不包含单位")


class BotConfig(BaseModel):
    """机器人配置"""
    name: str
    QQID: str | int
    musicSignUrl: str = ""
    autoRestartSchedule: AutoRestartScheduleConfig
    offlineAutoRestart: bool = False

    @field_validator("name")
    @staticmethod
    def validate_name(value: str) -> str:
        """验证机器人名称"""
        if not value:
            return "".join(random.choices(string.ascii_letters, k=8))
        return value

    @field_validator("QQID")
    @staticmethod
    def validate_qqid(value: str | int) -> int:
        """验证QQID"""
        if isinstance(value, int):
            return value
        if isinstance(value, str):
            try:
                return int(value)
            except ValueError:
                raise ValueError(f"QQ号 '{value}' 无法转换为整数")
        raise TypeError("QQ号必须是字符串或整数")


# 测试类
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

    def test_serialization(self):
        """测试序列化"""
        config = AutoRestartScheduleConfig(enable=True, time_unit=TimeUnitEnum.MINUTE, duration=30)
        data = config.model_dump()
        assert data["enable"] is True
        assert data["time_unit"] == "m"
        assert data["duration"] == 30


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
        assert config.QQID == 123456789
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
        with pytest.raises(ValueError, match="无法转换为整数"):
            BotConfig(
                name="TestBot",
                QQID="invalid_qq",
                autoRestartSchedule=AutoRestartScheduleConfig()
            )

    def test_nested_config(self):
        """测试嵌套配置"""
        config = BotConfig(
            name="TestBot",
            QQID=123456,
            autoRestartSchedule=AutoRestartScheduleConfig(
                enable=True,
                time_unit=TimeUnitEnum.DAY,
                duration=7
            )
        )
        assert config.autoRestartSchedule.enable is True
        assert config.autoRestartSchedule.time_unit == TimeUnitEnum.DAY
        assert config.autoRestartSchedule.duration == 7

    def test_model_dump(self):
        """测试模型序列化"""
        config = BotConfig(
            name="TestBot",
            QQID=123456,
            musicSignUrl="https://example.com",
            autoRestartSchedule=AutoRestartScheduleConfig(),
            offlineAutoRestart=True
        )
        data = config.model_dump()
        assert data["name"] == "TestBot"
        assert data["QQID"] == 123456
        assert data["musicSignUrl"] == "https://example.com"
        assert data["offlineAutoRestart"] is True
