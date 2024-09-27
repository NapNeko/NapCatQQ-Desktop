# -*- coding: utf-8 -*-
# 标准库导入
from typing import TYPE_CHECKING

# 第三方库导入
from qfluentwidgets import Action
from qfluentwidgets import FluentIcon as FIF
from qfluentwidgets import SystemTrayMenu
from PySide6.QtWidgets import QApplication, QSystemTrayIcon

if TYPE_CHECKING:
    # 项目内模块导入
    from src.Ui.MainWindow.Window import MainWindow


class SystemTrayIcon(QSystemTrayIcon):
    """
    ## 系统托盘功能
    """

    def __init__(self, parent: "MainWindow" = None):
        super().__init__(parent=parent)

        # 创建控件
        self.menu = SystemTrayMenu(parent=parent)
        self.menu.addAction(Action(FIF.CLOSE, self.tr("Close NapCat Desktop"), triggered=QApplication.quit))

        # 设置控件
        self.setIcon(parent.windowIcon())
        self.setToolTip("NapCat Desktop")
        self.setContextMenu(self.menu)
