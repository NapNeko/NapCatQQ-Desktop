# -*- coding: utf-8 -*-
# 标准库导入
import os
import sys

from PySide6.QtCore import Qt
from PySide6.QtWidgets import QApplication

# 项目内模块导入
from src.core.config import cfg
from src.core.utils.mutex import SingleInstanceApplication
from src.core.utils.path_func import PathFunc
from src.resource import resource
from src.ui.common.font import FontManager

if __name__ == "__main__":
    # 实现单实例应用程序检查
    if SingleInstanceApplication().is_running():
        sys.exit()

    # 执行路径验证
    PathFunc().path_validator()

    # 设置DPI缩放
    if cfg.get(cfg.dpi_scale) == "Auto":
        QApplication.setHighDpiScaleFactorRoundingPolicy(Qt.HighDpiScaleFactorRoundingPolicy.PassThrough)
    else:
        os.environ["QT_SCALE_FACTOR"] = str(cfg.get(cfg.dpi_scale))

    app = QApplication(sys.argv)

    # 初始化字体
    FontManager.initialize_fonts()

    if cfg.get(cfg.main_window):
        # 项目内模块导入
        from src.ui.window.main_window import MainWindow

        MainWindow().initialize()
    else:
        # 项目内模块导入
        from src.ui.window.guide_window import GuideWindow

        GuideWindow().initialize()
        cfg.set(cfg.main_window, True)

    sys.exit(app.exec())
