# -*- coding: utf-8 -*-
# 标准库导入
import sys

from PySide6.QtWidgets import QApplication

# 项目内模块导入
from src.Ui.MainWindow import MainWindow
from src.Core.Utils.mutex import SingleInstanceApplication

if __name__ == "__main__":
    # 实现单实例应用程序检查
    if SingleInstanceApplication().is_running():
        sys.exit()

    # 创建应用程序
    app = QApplication(sys.argv)

    # 初始化主窗口
    MainWindow().initialize()

    # 进入循环
    sys.exit(app.exec())
