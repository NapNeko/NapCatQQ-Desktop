# -*- coding: utf-8 -*-
"""测试日志数据模型"""
# 标准库导入
from datetime import datetime
from enum import Enum


# 复制日志枚举
class LogLevel(Enum):
    """日志等级"""
    DBUG = 0
    INFO = 1
    WARN = 2
    EROR = 3
    CRIT = 4
    ALL_ = 5


class LogType(Enum):
    """日志类型"""
    FILE_FUNC = 0
    NETWORK = 1
    NONE_TYPE = 2


class LogSource(Enum):
    """日志来源"""
    UI = 0
    CORE = 1
    NONE = 2


class TestLogDataModel:
    """测试日志数据模型"""

    def test_log_entry_structure(self):
        """测试日志条目结构"""
        log_entry = {
            "timestamp": datetime.now().isoformat(),
            "level": LogLevel.INFO,
            "type": LogType.FILE_FUNC,
            "source": LogSource.CORE,
            "message": "Test message",
        }
        
        assert "timestamp" in log_entry
        assert "level" in log_entry
        assert "message" in log_entry

    def test_log_level_comparison(self):
        """测试日志等级比较"""
        assert LogLevel.DBUG.value < LogLevel.INFO.value
        assert LogLevel.INFO.value < LogLevel.WARN.value
        assert LogLevel.WARN.value < LogLevel.EROR.value
        assert LogLevel.EROR.value < LogLevel.CRIT.value

    def test_log_message_format(self):
        """测试日志消息格式"""
        timestamp = "2024-01-01 12:00:00"
        level = "INFO"
        message = "Application started"
        
        formatted = f"[{timestamp}] [{level}] {message}"
        
        assert timestamp in formatted
        assert level in formatted
        assert message in formatted

    def test_log_type_categorization(self):
        """测试日志类型分类"""
        types = {
            LogType.FILE_FUNC: "文件操作",
            LogType.NETWORK: "网络操作",
            LogType.NONE_TYPE: "无类型",
        }
        
        assert len(types) == 3
        assert LogType.FILE_FUNC in types


class TestLogFormatting:
    """测试日志格式化"""

    def test_timestamp_formatting(self):
        """测试时间戳格式化"""
        now = datetime.now()
        formatted = now.strftime("%Y-%m-%d %H:%M:%S")
        
        assert len(formatted) == 19
        assert "-" in formatted
        assert ":" in formatted

    def test_level_string_formatting(self):
        """测试等级字符串格式化"""
        level = LogLevel.INFO
        formatted = f"[{level.name}]"
        
        assert formatted == "[INFO]"

    def test_multiline_message(self):
        """测试多行消息"""
        message = """Line 1
Line 2
Line 3"""
        
        lines = message.split("\n")
        assert len(lines) == 3


class TestLogFiltering:
    """测试日志过滤"""

    def test_filter_by_level(self):
        """测试按等级过滤"""
        logs = [
            {"level": LogLevel.INFO, "msg": "info"},
            {"level": LogLevel.WARN, "msg": "warn"},
            {"level": LogLevel.EROR, "msg": "error"},
        ]
        
        errors = [log for log in logs if log["level"] == LogLevel.EROR]
        
        assert len(errors) == 1
        assert errors[0]["msg"] == "error"

    def test_filter_by_source(self):
        """测试按来源过滤"""
        logs = [
            {"source": LogSource.UI, "msg": "ui log"},
            {"source": LogSource.CORE, "msg": "core log"},
        ]
        
        core_logs = [log for log in logs if log["source"] == LogSource.CORE]
        
        assert len(core_logs) == 1
        assert core_logs[0]["msg"] == "core log"
