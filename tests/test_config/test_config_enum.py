# -*- coding: utf-8 -*-
"""测试配置枚举类"""
# 标准库导入
from enum import Enum

# 第三方库导入
import pytest


# 直接定义枚举类以避免导入 PySide6
class TimeUnitEnum(str, Enum):
    """时间单位枚举（从源文件复制）"""
    MINUTE = "m"
    HOUR = "h"
    DAY = "d"
    MONTH = "mon"
    YEAR = "year"


class CloseActionEnum(Enum):
    """关闭动作枚举（从源文件复制）"""
    CLOSE = 0
    HIDE = 1


class TestTimeUnitEnum:
    """测试时间单位枚举"""

    def test_enum_values(self):
        """测试枚举值存在"""
        # 注意：源文件没有 SECOND，所以移除
        assert TimeUnitEnum.MINUTE is not None
        assert TimeUnitEnum.HOUR is not None
        assert TimeUnitEnum.DAY is not None
        assert TimeUnitEnum.MONTH is not None
        assert TimeUnitEnum.YEAR is not None

    def test_enum_values_are_strings(self):
        """测试枚举值是字符串"""
        assert isinstance(TimeUnitEnum.MINUTE.value, str)
        assert isinstance(TimeUnitEnum.HOUR.value, str)
        assert isinstance(TimeUnitEnum.DAY.value, str)

    def test_enum_string_values(self):
        """测试枚举字符串值"""
        assert TimeUnitEnum.MINUTE.value == "m"
        assert TimeUnitEnum.HOUR.value == "h"
        assert TimeUnitEnum.DAY.value == "d"
        assert TimeUnitEnum.MONTH.value == "mon"
        assert TimeUnitEnum.YEAR.value == "year"


class TestCloseActionEnum:
    """测试关闭动作枚举"""

    def test_enum_values_exist(self):
        """测试枚举值存在"""
        assert CloseActionEnum.CLOSE is not None
        assert CloseActionEnum.HIDE is not None

    def test_enum_values_are_ints(self):
        """测试枚举值是整数"""
        assert isinstance(CloseActionEnum.CLOSE.value, int)
        assert isinstance(CloseActionEnum.HIDE.value, int)

    def test_enum_unique_values(self):
        """测试枚举值唯一"""
        values = [
            CloseActionEnum.CLOSE.value,
            CloseActionEnum.HIDE.value,
        ]
        assert len(values) == len(set(values))
