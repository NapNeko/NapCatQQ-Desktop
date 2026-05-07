# -*- coding: utf-8 -*-
"""[`RemoteSummaryCard`](src/ui/components/remote_summary_card.py) 纯函数单测 (P4 W2·F4).

仅覆盖文案格式化辅助 ``_format_breach``, 不实例化 QWidget.
"""
from __future__ import annotations

# 标准库导入
import time

# 第三方库导入
import pytest

# 项目内模块导入
from src.ui.components.remote_summary_card import _format_breach


def test_format_breach_just_now() -> None:
    now = time.time()
    text = _format_breach("us-east-1", "cpu", 95.4, now)
    assert "us-east-1" in text
    assert "CPU" in text
    assert "95%" in text
    assert "刚刚" in text


def test_format_breach_minutes_ago() -> None:
    text = _format_breach("HK-A", "mem", 92.0, time.time() - 300)  # 5min 前
    assert "HK-A" in text
    assert "内存" in text
    assert "92%" in text
    # 文案包含 "X 分钟前", X 可能是 4 或 5, 取决于时序
    assert "分钟前" in text


def test_format_breach_disk_label() -> None:
    text = _format_breach("EU-1", "disk", 91.0, time.time())
    assert "磁盘" in text
    assert "91%" in text


def test_format_breach_unknown_metric_falls_back_to_uppercase() -> None:
    text = _format_breach("srv", "swap", 80.0, time.time())
    # 未知 metric 直接走 .upper()
    assert "SWAP" in text


@pytest.mark.parametrize("value,expected", [(0.4, "0%"), (50.6, "51%"), (99.9, "100%")])
def test_format_breach_value_rounding(value: float, expected: str) -> None:
    text = _format_breach("srv", "cpu", value, time.time())
    assert expected in text
