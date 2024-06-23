# -*- coding: utf-8 -*-
import sys

from PySide6.QtCore import QTranslator, QLocale
from creart import it
from loguru import logger
from PySide6.QtWidgets import QApplication
from qfluentwidgets import FluentTranslator

from src.Core.Config import cfg
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

    # 加载翻译文件
    locale: QLocale = cfg.get(cfg.language).value
    translator = FluentTranslator(locale)
    NCDTranslator = QTranslator()
    NCDTranslator.load(locale, f":i18n/i18n/translation.{locale.name()}.qm")
    app.installTranslator(translator)
    app.installTranslator(NCDTranslator)

    # 显示窗体
    it(MainWindow).initialize()
    # 进入循环
    app.exec()
