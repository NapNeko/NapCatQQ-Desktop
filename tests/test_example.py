# -*- coding: utf-8 -*-
"""简单的示例测试，用于验证 pytest 框架配置"""


def add(a: int, b: int) -> int:
    """简单的加法函数"""
    return a + b


def multiply(a: int, b: int) -> int:
    """简单的乘法函数"""
    return a * b


class TestBasicMath:
    """测试基本数学函数"""

    def test_add_positive_numbers(self):
        """测试正数相加"""
        assert add(2, 3) == 5
        assert add(10, 20) == 30

    def test_add_negative_numbers(self):
        """测试负数相加"""
        assert add(-5, -3) == -8
        assert add(-10, 5) == -5

    def test_add_zero(self):
        """测试加零"""
        assert add(0, 5) == 5
        assert add(5, 0) == 5
        assert add(0, 0) == 0

    def test_multiply_positive_numbers(self):
        """测试正数相乘"""
        assert multiply(2, 3) == 6
        assert multiply(5, 4) == 20

    def test_multiply_by_zero(self):
        """测试乘以零"""
        assert multiply(5, 0) == 0
        assert multiply(0, 5) == 0

    def test_multiply_by_one(self):
        """测试乘以一"""
        assert multiply(5, 1) == 5
        assert multiply(1, 10) == 10
