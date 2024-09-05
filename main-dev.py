# -*- coding: utf-8 -*-
import ctypes
import sys

from loguru import logger

NAPCATQQ_DESKTOP_LOGO = r"""

 +-+-+-+-+-+-+ +-+-+-+-+-+-+-+
 |N|a|p|C|a|t| |D|e|s|k|t|o|p|
 +-+-+-+-+-+-+ +-+-+-+-+-+-+-+
"""


if __name__ == "__main__":
    # 检查是否以管理员模式启动, 非管理员模式尝试获取管理员权限
    if not ctypes.windll.shell32.IsUserAnAdmin():
        logger.warning("非管理员模式启动, 尝试获取管理员权限")
        ctypes.windll.shell32.ShellExecuteW(None, "runas", sys.executable, " ".join(sys.argv), None, 1)

    # 启动主程序
    from src.Core.Config import cfg
    from src.Ui.MainWindow import MainWindow
    from qfluentwidgets import FluentTranslator
    from PySide6.QtCore import QTranslator, QLocale
    from PySide6.QtWidgets import QApplication
    from creart import it
    from loguru import logger

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
    sys.exit(app.exec())
