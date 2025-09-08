# -*- coding: utf-8 -*-
# 标准库导入
import sys
from typing import TYPE_CHECKING

# 第三方库导入
from qfluentwidgets import Action, AvatarWidget, BodyLabel, CaptionLabel
from qfluentwidgets import FluentIcon as FIF
from qfluentwidgets import SystemTrayMenu
from PySide6.QtCore import Slot
from PySide6.QtGui import QColor
from PySide6.QtWidgets import QSystemTrayIcon, QWidget

# 项目内模块导入
from src.ui.common.icon import StaticIcon
from src.ui.components.info_bar import success_bar
from src.ui.page.bot_list_page import BotListWidget

if TYPE_CHECKING:
    # 项目内模块导入
    from src.ui.window.main_window.window import MainWindow


"""系统托盘图标模块

创建一个系统托盘图标, 并添加一个右键菜单, 可快速操作

Attributes:
    SystemTrayIcon (QSystemTrayIcon): 系统托盘图标类
    NCDCard (QWidget): NapCatQQ Desktop 卡片展示类
"""


class SystemTrayIcon(QSystemTrayIcon):
    """系統托盤圖標

    创建一个系统托盘图标, 并添加一个右键菜单, 可快速操作

    Attributes:
        menu (SystemTrayMenu): 系统托盘图标的右键菜单
    """

    def __init__(self, parent: "MainWindow"):
        """初始化系统托盘图标, 创建右键菜单并添加相关选项

        Args:
            parent (MainWindow): 主窗体
        """
        super().__init__(parent=parent)

        # 创建控件
        self.menu = SystemTrayMenu(parent=parent)

        self.menu.addWidget(NCDCard(self), selectable=False)
        self.menu.addSeparator()
        self.menu.addAction(
            Action(
                icon=FIF.PLAY,
                text=self.tr("运行所有机器人"),
                triggered=lambda: (self.check_show_state(), BotListWidget().runAllBot()),
            )
        )
        self.menu.addAction(
            Action(
                icon=FIF.PAUSE,
                text=self.tr("停止所有机器人"),
                triggered=lambda: (
                    self.check_show_state(),
                    success_bar(self.tr("成功停止所有机器人运行")),
                    self.stopAllBotBotSlot(),
                ),
            )
        )
        self.menu.addSeparator()
        self.menu.addAction(
            Action(
                icon=FIF.CLOSE,
                text=self.tr("关闭程序"),
                triggered=lambda: (BotListWidget().stopAllBot(), sys.exit()),
            )
        )

        # 设置控件
        self.setIcon(parent.windowIcon())
        self.setToolTip("NapCatQQ Desktop")
        self.setContextMenu(self.menu)

        # 链接信号
        self.activated.connect(self.on_click)

    @Slot(QSystemTrayIcon)
    def on_click(self, reason: QSystemTrayIcon) -> None:
        """系统托盘图标点击事件

        通过点击系统托盘图标, 切换主窗口的显示状态

        Args:
            reason (QSystemTrayIcon): 系统托盘图标
        """
        if reason == QSystemTrayIcon.ActivationReason.Trigger:
            self.check_show_state()

    def check_show_state(self) -> None:
        """检查主窗口显示状态并切换

        根据主窗口当前的显示状态, 进行显示或隐藏操作
        """
        # 项目内模块导入
        from src.ui.window.main_window.window import MainWindow

        if MainWindow().isMinimized():
            MainWindow().showNormal()
        else:
            MainWindow().show()
        MainWindow().raise_()
        MainWindow().activateWindow()


class NCDCard(QWidget):
    """NapCatQQ Desktop 卡片, 显示软件信息

    Attributes:
        avatar (AvatarWidget): 软件头像
        nameLabel (BodyLabel): 软件名称标签
        licenseLabel (CaptionLabel): 许可证标签
    """

    def __init__(self, parent: SystemTrayIcon):
        """初始化 NapCatQQ Desktop 卡片

        Args:
            parent (SystemTrayIcon): 系统托盘图标
        """
        super().__init__()
        self.avatar = AvatarWidget(StaticIcon.LOGO.path(), self)
        self.nameLabel = BodyLabel("NapCatQQ Desktop", self)
        self.licenseLabel = CaptionLabel("License: GPL-v3", self)
        self.licenseLabel.setTextColor(QColor(96, 96, 96), QColor(206, 206, 206))

        self.setFixedSize(218, 60)
        self.avatar.setRadius(24)
        self.avatar.move(2, 6)
        self.nameLabel.move(64, 13)
        self.licenseLabel.move(64, 32)
