# -*- coding: utf-8 -*-
# 标准库导入
import sys
import ctypes

# 第三方库导入
from loguru import logger

# 项目内模块导入
from src.Core.Utils.logger import Logger

NAPCATQQ_DESKTOP_LOGO = r"""

 +-+-+-+-+-+-+ +-+-+-+-+-+-+-+
 |N|a|p|C|a|t| |D|e|s|k|t|o|p|
 +-+-+-+-+-+-+ +-+-+-+-+-+-+-+
"""


if __name__ == "__main__":
    # 调整程序 log 输出
    Logger()
    # 检查是否以管理员模式启动, 非管理员模式尝试获取管理员权限
    # 获取管理员权限成功后退出原有进程
    if not ctypes.windll.shell32.IsUserAnAdmin():
        logger.warning("非管理员模式启动, 尝试获取管理员权限")
        ctypes.windll.shell32.ShellExecuteW(None, "runas", sys.executable, " ".join(sys.argv), None, 1)
        sys.exit()

    # 启动主程序
    # 第三方库导入
    from creart import it
    from qfluentwidgets import FluentTranslator
    from PySide6.QtCore import QLocale, QTranslator
    from PySide6.QtWidgets import QApplication

    # 项目内模块导入
    from src.Core.Config import cfg
    from src.Ui.MainWindow import MainWindow

    logger.opt(colors=True).info(f"<blue>{NAPCATQQ_DESKTOP_LOGO}</>")
    # 创建app实例
    app = QApplication(sys.argv)

    # 加载翻译文件
    locale: QLocale = cfg.get(cfg.language).value
    NCDTranslator = QTranslator()
    NCDTranslator.load(locale, f":i18n/i18n/translation.{locale.name()}.qm")
    app.installTranslator(FluentTranslator(locale))
    app.installTranslator(NCDTranslator)

    # 显示窗体
    it(MainWindow).initialize()

    # 进入循环
    sys.exit(app.exec())
