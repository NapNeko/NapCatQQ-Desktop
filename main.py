# -*- coding: utf-8 -*-
# 标准库导入
import sys

from PySide6.QtWidgets import QApplication

# 项目内模块导入
from src.Core.Config import cfg
from src.Core.Utils.mutex import SingleInstanceApplication
from src.Core.Utils.PathFunc import PathFunc

if __name__ == "__main__":
    # 实现单实例应用程序检查
    if SingleInstanceApplication().is_running():
        sys.exit()

    # 执行路径验证
    PathFunc().path_validator()

    app = QApplication(sys.argv)

    if cfg.get(cfg.MainWindow):
        # 项目内模块导入
        from src.Ui.MainWindow import MainWindow

        MainWindow().initialize()
    else:
        # 项目内模块导入
        from src.Ui.GuideWindow.guide_window import GuideWindow

        GuideWindow().initialize()
        cfg.set(cfg.MainWindow, False)

    sys.exit(app.exec())
