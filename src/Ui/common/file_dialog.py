# -*- coding: utf-8 -*-
"""
NCD 的文件夹对话框相关包装
"""

# 标准库导入
from pathlib import Path

from PySide6.QtCore import QStandardPaths
from PySide6.QtWidgets import QFileDialog


def getFilePath(title: str, file_name: str | None, filter: str) -> Path:
    """调用 QFileDialog.getOpenFileName 获取文件路径"""

    # 项目内模块导入
    from src.ui.main_window import MainWindow

    # 参数处理
    file_name = file_name or ""
    dufalt_path = str(Path(QStandardPaths.writableLocation(QStandardPaths.StandardLocation.HomeLocation)) / file_name)

    # 获取路径
    if path := QFileDialog.getOpenFileName(MainWindow(), title, dufalt_path, filter)[0]:
        return Path(path)


def saveFilePath(title: str, file_name: str | None, filter: str) -> Path:
    """调用 QFileDialog.getSaveFileName 获取文件路径"""

    # 项目内模块导入
    from src.ui.main_window import MainWindow

    # 参数处理
    file_name = file_name or ""
    dufalt_path = str(Path(QStandardPaths.writableLocation(QStandardPaths.StandardLocation.HomeLocation)) / file_name)

    # 获取路径
    if path := QFileDialog.getSaveFileName(MainWindow(), title, dufalt_path, filter)[0]:
        return Path(path)
