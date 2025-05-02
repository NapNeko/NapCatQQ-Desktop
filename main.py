# -*- coding: utf-8 -*-
# 标准库导入
import sys
from pathlib import Path

from PySide6.QtWidgets import QApplication

# 项目内模块导入
from src.Core.Utils.mutex import SingleInstanceApplication

if __name__ == "__main__":
    # 实现单实例应用程序检查
    if SingleInstanceApplication().is_running():
        sys.exit()

    app = QApplication(sys.argv)

    if (Path.cwd() / "config" / "config.json").exists():
        # 项目内模块导入
        from src.Ui.MainWindow import MainWindow

        MainWindow().initialize()
    else:
        # 项目内模块导入
        from src.Ui.GuideWindow.guide_window import GuideWindow

        GuideWindow().initialize()

    sys.exit(app.exec())
