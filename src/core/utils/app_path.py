# -*- coding: utf-8 -*-
"""应用路径解析工具。"""

# 标准库导入
import sys
from pathlib import Path


def resolve_app_base_path() -> Path:
    """解析应用基准目录。

    冻结运行时使用可执行文件所在目录。
    源码运行时使用项目根目录，而不是当前工作目录。
    """

    if getattr(sys, "frozen", False):
        return Path(sys.executable).resolve().parent

    return Path(__file__).resolve().parents[3]
