# -*- coding: utf-8 -*-
# 标准库导入
import argparse
from dataclasses import dataclass


@dataclass(frozen=True, slots=True)
class RuntimeLaunchOptions:
    """运行时启动选项。"""

    developer_mode: bool = False


_runtime_launch_options = RuntimeLaunchOptions()


def parse_runtime_launch_options(argv: list[str]) -> tuple[RuntimeLaunchOptions, list[str]]:
    """解析应用自定义启动参数，并返回保留给 Qt 的参数列表。"""
    parser = argparse.ArgumentParser(add_help=False)
    parser.add_argument("--developer-mode", "--dev", action="store_true", dest="developer_mode")

    raw_args = argv[1:] if argv else []
    namespace, remaining_args = parser.parse_known_args(raw_args)
    filtered_argv = [argv[0], *remaining_args] if argv else remaining_args
    return RuntimeLaunchOptions(developer_mode=bool(namespace.developer_mode)), filtered_argv


def apply_runtime_launch_options(options: RuntimeLaunchOptions) -> None:
    """应用解析后的运行时选项到当前进程。"""
    global _runtime_launch_options
    _runtime_launch_options = options


def get_runtime_launch_options() -> RuntimeLaunchOptions:
    """获取当前进程的运行时选项。"""
    return _runtime_launch_options


def is_developer_mode_enabled() -> bool:
    """判断当前进程是否启用了开发者模式。"""
    return _runtime_launch_options.developer_mode
