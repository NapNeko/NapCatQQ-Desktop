# -*- coding: utf-8 -*-
# 标准库导入
import sys
from typing import TYPE_CHECKING

# 第三方库导入
from creart import it
from qfluentwidgets import Action, BodyLabel
from qfluentwidgets import FluentIcon as FIF
from qfluentwidgets import AvatarWidget, CaptionLabel, SystemTrayMenu
from PySide6.QtGui import QColor
from PySide6.QtCore import Slot
from PySide6.QtWidgets import QWidget, QSystemTrayIcon

# 项目内模块导入
from src.Ui.BotListPage import BotListWidget
from src.Ui.common.info_bar import success_bar

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

        self.menu.addWidget(NCDCard(), selectable=False)
        self.menu.addSeparator()
        self.menu.addAction(Action(FIF.ADD, self.tr("Add Bot"), triggered=self.addBotSlot))
        self.menu.addAction(Action(FIF.MENU, self.tr("Bot List"), triggered=self.botListSlot))
        self.menu.addSeparator()
        self.menu.addAction(Action(FIF.PLAY, self.tr("Run All Bots"), triggered=self.runAllBotSlot))
        self.menu.addAction(Action(FIF.PAUSE, self.tr("Stop All Bots"), triggered=self.stopAllBotBotSlot))
        self.menu.addSeparator()
        self.menu.addAction(Action(FIF.CLOSE, self.tr("Close"), triggered=self.closeSlot))

        # 设置控件
        self.setIcon(parent.windowIcon())
        self.setToolTip("NapCat Desktop")
        self.setContextMenu(self.menu)

        # 链接信号
        self.activated.connect(self.clickSlot)

    @Slot()
    def addBotSlot(self) -> None:
        self.checkShow()
        self.parent().add_widget_button.click()

    @Slot()
    def botListSlot(self) -> None:
        self.checkShow()
        self.parent().bot_list_widget_button.click()

    @Slot()
    def runAllBotSlot(self) -> None:
        self.checkShow()
        it(BotListWidget).runAllBot()

    @Slot()
    def stopAllBotBotSlot(self) -> None:
        self.checkShow()
        success_bar(self.tr("成功停止所有机器人运行"))
        it(BotListWidget).stopAllBot()

    @Slot()
    def closeSlot(self) -> None:
        """
        ## 关闭槽函数
        """
        # 循环判断是否机器人已经关闭
        it(BotListWidget).stopAllBot()
        sys.exit()

    @Slot(QSystemTrayIcon)
    def clickSlot(self, reason: QSystemTrayIcon) -> None:
        """
        ## 托盘图标被点击事件
        """
        if reason == QSystemTrayIcon.ActivationReason.Trigger:
            self.showNormal() if self.isMinimized() else None
            self.show() if self.isHidden() else None

    def checkShow(self) -> None:
        """
        ## 判断主窗体显示状态
        """
        if self.parent().isMinimized():
            self.parent().showNormal()
        else:
            self.parent().show()
        self.parent().raise_()
        self.parent().activateWindow()


class NCDCard(QWidget):

    def __init__(self):
        super().__init__()
        self.avatar = AvatarWidget(":Global/logo.png", self)
        self.nameLabel = BodyLabel("NapCat Desktop", self)
        self.licenseLabel = CaptionLabel("License: GPL-v3", self)
        self.licenseLabel.setTextColor(QColor(96, 96, 96), QColor(206, 206, 206))

        self.setFixedSize(200, 60)
        self.avatar.setRadius(24)
        self.avatar.move(2, 6)
        self.nameLabel.move(64, 13)
        self.licenseLabel.move(64, 32)
