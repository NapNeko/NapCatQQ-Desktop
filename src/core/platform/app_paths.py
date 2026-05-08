# -*- coding: utf-8 -*-
"""应用路径解析工具. """

# 标准库导入
import os
import sys
from pathlib import Path

APP_DATA_DIR_NAME = "NapCatQQ Desktop"


def resolve_app_base_path() -> Path:
    """解析应用基准目录. 

    冻结运行时使用可执行文件所在目录. 
    源码运行时使用**仓库根目录**, 让 ``runtime/`` / ``log/`` / ``config/``
    等运行时数据落在 Python 包之外, 便于 git 忽略与备份.

    深度计算 (从 v2 flatten 后):
    - 当前文件位于 ``src/core/platform/app_paths.py``
    - parents[0]=platform, parents[1]=core, parents[2]=src, parents[3]=仓库根
    """

    if getattr(sys, "frozen", False):
        return Path(sys.executable).resolve().parent

    return Path(__file__).resolve().parents[3]


def resolve_app_data_path() -> Path:
    """解析应用可写数据目录. 

    冻结运行时默认使用 ProgramData, 便于 MSI 安装到 Program Files 后仍能写入运行时数据. 
    源码运行时继续使用项目根目录, 避免影响本地开发和测试体验. 
    """

    if getattr(sys, "frozen", False):
        program_data = os.environ.get("ProgramData") or os.environ.get("PROGRAMDATA")
        if program_data:
            return Path(program_data).resolve() / APP_DATA_DIR_NAME

        return Path(r"C:\ProgramData") / APP_DATA_DIR_NAME

    return resolve_app_base_path()
