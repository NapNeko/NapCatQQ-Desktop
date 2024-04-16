# -*- coding: utf-8 -*-
import sys

from PySide6.QtWidgets import QApplication
from creart import it
from loguru import logger

from ui.main_window import MainWindow

NAPCAT_DESKTOP_LOGO = r"""

   _  __               _____        __         ___              __    __             
  / |/ / ___ _   ___  / ___/ ___ _ / /_       / _ \ ___   ___  / /__ / /_ ___    ___ 
 /    / / _ `/  / _ \/ /__  / _ `// __/      / // // -_) (_-< /  '_// __// _ \  / _ \
/_/|_/  \_,_/  / .__/\___/  \_,_/ \__/      /____/ \__/ /___//_/\_\ \__/ \___/ / .__/
              /_/                                                             /_/    

"""

if __name__ == "__main__":
    logger.opt(colors=True).info(f"<blue>{NAPCAT_DESKTOP_LOGO}</>")
    # 创建app实例
    app = QApplication(sys.argv)
    # 显示窗体
    it(MainWindow)
    # 进入循环
    app.exec()
