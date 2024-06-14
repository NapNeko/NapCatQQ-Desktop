# -*- coding: utf-8 -*-
import sys
from pathlib import Path

from creart import it
from loguru import logger
from PySide6.QtWidgets import QApplication, QWidget
from PySide6.QtGui import QFontDatabase, QFont

from src.Ui.MainWindow import MainWindow

NAPCATQQ_DESKTOP_LOGO = r"""

   _  __                _____         __    ____     ____        ___                __     __              
  / |/ / ___ _  ___    / ___/ ___ _  / /_  / __ \   / __ \      / _ \  ___   ___   / /__  / /_  ___    ___ 
 /    / / _ `/ / _ \  / /__  / _ `/ / __/ / /_/ /  / /_/ /     / // / / -_) (_-<  /  '_/ / __/ / _ \  / _ \
/_/|_/  \_,_/ / .__/  \___/  \_,_/  \__/  \___\_\  \___\_\    /____/  \__/ /___/ /_/\_\  \__/  \___/ / .__/
             /_/                                                                                    /_/    

"""


if __name__ == "__main__":
    logger.opt(colors=True).info(f"<blue>{NAPCATQQ_DESKTOP_LOGO}</>")
    # 创建app实例
    app = QApplication(sys.argv)
    # 显示窗体
    it(MainWindow)
    # 进入循环
    app.exec()
