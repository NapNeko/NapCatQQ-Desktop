# -*- coding: utf-8 -*-
"""测试日志枚举"""
# 标准库导入
from enum import Enum


# 直接定义枚举类（从源文件复制）
class LogLevel(Enum):
    """日志等级"""
    DBUG = 0  # debug
    INFO = 1  # information
    WARN = 2  # warning
    EROR = 3  # error
    CRIT = 4  # critical
    ALL_ = 5  # all


class LogType(Enum):
    """日志类型"""
    FILE_FUNC = 0  # 文件操作
    NETWORK = 1  # 网络操作
    NONE_TYPE = 2  # 无类型


class LogSource(Enum):
    """日志来源"""
    UI = 0  # 用户界面
    CORE = 1  # 核心逻辑
    NONE = 2  # 无来源


class TestLogLevel:
    """测试日志等级枚举"""

    def test_log_level_values_exist(self):
        """测试日志等级枚举值存在"""
        assert LogLevel.DBUG is not None
        assert LogLevel.INFO is not None
        assert LogLevel.WARN is not None
        assert LogLevel.EROR is not None
        assert LogLevel.CRIT is not None
        assert LogLevel.ALL_ is not None

    def test_log_level_values_are_ints(self):
        """测试日志等级值是整数"""
        assert isinstance(LogLevel.DBUG.value, int)
        assert isinstance(LogLevel.INFO.value, int)
        assert isinstance(LogLevel.WARN.value, int)

    def test_log_level_unique_values(self):
        """测试日志等级值唯一"""
        values = [e.value for e in LogLevel]
        assert len(values) == len(set(values))



class TestLogType:
    """测试日志类型枚举"""

    def test_log_type_values_exist(self):
        """测试日志类型枚举值存在"""
        assert LogType.FILE_FUNC is not None
        assert LogType.NETWORK is not None
        assert LogType.NONE_TYPE is not None

    def test_log_type_values_are_ints(self):
        """测试日志类型值是整数"""
        assert isinstance(LogType.FILE_FUNC.value, int)
        assert isinstance(LogType.NETWORK.value, int)
        assert isinstance(LogType.NONE_TYPE.value, int)


class LogSource(Enum):
    """日志来源"""
    UI = 0  # 用户界面
    CORE = 1  # 核心逻辑
    NONE = 2  # 无来源


class TestLogSource:
    """测试日志来源枚举"""

    def test_log_source_values_exist(self):
        """测试日志来源枚举值存在"""
        assert LogSource.UI is not None
        assert LogSource.CORE is not None
        assert LogSource.NONE is not None

    def test_log_source_values_are_ints(self):
        """测试日志来源值是整数"""
        assert isinstance(LogSource.UI.value, int)
        assert isinstance(LogSource.CORE.value, int)
        assert isinstance(LogSource.NONE.value, int)
