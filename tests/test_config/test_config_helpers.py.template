# -*- coding: utf-8 -*-
"""测试配置模型中的辅助函数"""
# 标准库导入
import sys
from pathlib import Path

# 添加项目路径
sys.path.insert(0, str(Path(__file__).parent.parent.parent))

# 直接导入模块文件，绕过 __init__.py
import importlib.util

spec = importlib.util.spec_from_file_location(
    "config_model",
    Path(__file__).parent.parent.parent / "src" / "core" / "config" / "config_model.py"
)
config_model = importlib.util.module_from_spec(spec)
spec.loader.exec_module(config_model)

# 获取函数
_coerce_interval_default = config_model._coerce_interval_default


class TestCoerceIntervalDefaultDirect:
    """直接测试 _coerce_interval_default 函数"""

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
