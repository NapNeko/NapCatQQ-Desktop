# -*- coding: utf-8 -*-

# 项目内模块导入
from src.core.utils.runtime_args import parse_runtime_launch_options


def test_parse_runtime_launch_options_enables_developer_mode() -> None:
    """开发者模式参数应被识别并从 Qt 参数列表剔除。"""
    options, filtered_argv = parse_runtime_launch_options(
        ["main.py", "--developer-mode", "--platform", "windows:darkmode=1"]
    )

    assert options.developer_mode is True
    assert filtered_argv == ["main.py", "--platform", "windows:darkmode=1"]


def test_parse_runtime_launch_options_preserves_non_app_arguments() -> None:
    """非应用自定义参数应完整保留给 Qt。"""
    options, filtered_argv = parse_runtime_launch_options(["main.py", "--platform", "windows:darkmode=1"])

    assert options.developer_mode is False
    assert filtered_argv == ["main.py", "--platform", "windows:darkmode=1"]
