# -*- coding: utf-8 -*-
# 标准库导入
import os
import sys

from PySide6.QtCore import Qt
from PySide6.QtWidgets import QApplication

# 项目内模块导入
from src.core.config import cfg
from src.ui.main_window import MainWindow
from src.core.utils.mutex import SingleInstanceApplication

if __name__ == "__main__":
    # 实现单实例应用程序检查
    if SingleInstanceApplication().is_running():
        sys.exit()

    # 设置DPI缩放
    if cfg.get(cfg.dpiScale) == "Auto":
        QApplication.setHighDpiScaleFactorRoundingPolicy(Qt.HighDpiScaleFactorRoundingPolicy.PassThrough)
        QApplication.setAttribute(Qt.AA_EnableHighDpiScaling)
    else:
        os.environ["QT_ENABLE_HIGHDPI_SCALING"] = "0"
        os.environ["QT_SCALE_FACTOR"] = str(cfg.get(cfg.dpiScale))

    # 创建应用程序
    app = QApplication(sys.argv)

    # 初始化主窗口
    MainWindow()

    # 进入循环
    sys.exit(app.exec())
