# -*- coding: utf-8 -*-
# 第三方库导入
from qfluentwidgets import FluentIcon, MSFluentWindow, NavigationItemPosition, SplashScreen, Theme
from PySide6.QtCore import QSize
from PySide6.QtGui import QIcon
from PySide6.QtWidgets import QApplication

# 项目内模块导入
from src.core.config import cfg
from src.core.config.config_enum import CloseActionEnum
from src.core.utils.singleton import singleton
from src.ui.common.icon import NapCatDesktopIcon
from src.ui.page.add_page import AddWidget
from src.ui.page.bot_list_page import BotListWidget
from src.ui.page.home_page import HomeWidget
from src.ui.page.setup_page import SetupWidget
from src.ui.page.unit_page import UnitWidget
from src.ui.window.main_window.system_try_icon import SystemTrayIcon
from src.ui.window.main_window.title_bar import CustomTitleBar

"""NapCatQQ Desktop 主窗口模块

该模块定义了主窗口类 MainWindow, 继承自 MSFluentWindow

Attributes:
    MainWindow (MSFluentWindow): 主窗口类
"""


@singleton
class MainWindow(MSFluentWindow):
    """程序的主窗体"""

    trayIcon: SystemTrayIcon
    title_bar: CustomTitleBar
    splash_screen: SplashScreen

    def __init__(self) -> None:
        """构造函数"""
        super().__init__()

    def initialize(self) -> None:
        """初始化"""
        # 调用方法
        self._set_window()
        self._set_item()
        self._set_tray_icon()

        # 组件加载完成结束 SplashScreen
        self.splash_screen.finish()

    def _set_window(self) -> None:
        """
        设置窗体
        """
        # 标题栏部分
        self.title_bar = CustomTitleBar(self)
        self.setTitleBar(self.title_bar)
        self.setWindowIcon(QIcon(NapCatDesktopIcon.LOGO.path(Theme.LIGHT)))
        # 窗体大小以及设置打开时居中
        self.setMinimumSize(1148, 720)
        desktop = QApplication.screens()[0].availableGeometry()
        width, height = desktop.width(), desktop.height()
        self.move(width // 2 - self.width() // 2, height // 2 - self.height() // 2)
        # 调整窗体透明度
        self.setWindowOpacity(cfg.get(cfg.window_opacity) / 100)
        # 创建 Splash Screen
        self.splash_screen = SplashScreen(":Global/image/Global/napcat.png", self, True)
        self.splash_screen.setIconSize(QSize(360, 260))
        self.splash_screen.raise_()
        # 显示窗体
        self.show()
        # 挂起
        QApplication.processEvents()

    def _set_item(self) -> None:
        """
        设置侧边栏
        """

        # 添加子页面
        self.addSubInterface(
            interface=HomeWidget().initialize(self),
            icon=FluentIcon.HOME,
            text=self.tr("主页"),
            position=NavigationItemPosition.TOP,
        )

        self.addSubInterface(
            interface=AddWidget().initialize(self),
            icon=FluentIcon.ADD_TO,
            text=self.tr("添加"),
            position=NavigationItemPosition.TOP,
        )
        self.addSubInterface(
            interface=BotListWidget().initialize(self),
            icon=FluentIcon.MENU,
            text=self.tr("列表"),
            position=NavigationItemPosition.TOP,
        )
        self.addSubInterface(
            interface=UnitWidget()._setup_ui(self),
            icon=FluentIcon.EMOJI_TAB_SYMBOLS,
            text=self.tr("组件"),
            position=NavigationItemPosition.BOTTOM,
        )
        self.addSubInterface(
            interface=SetupWidget().initialize(self),
            icon=FluentIcon.SETTING,
            text=self.tr("设置"),
            position=NavigationItemPosition.BOTTOM,
        )

    def _set_tray_icon(self):
        """
        ## 设置托盘图标
        """
        self.trayIcon = SystemTrayIcon(self)
        self.trayIcon.show()

    def close(self) -> None:
        """重写关闭事件"""
        if cfg.get(cfg.close_button_action) == CloseActionEnum.CLOSE:
            # 如果有机器人在线, 则提示用户关闭实例
            if BotListWidget().get_bot_is_run():
                # 项目内模块导入
                from src.ui.components.message_box import AskBox

                msg_box = AskBox(self.tr("无法退出"), self.tr("有机器人正在运行, 请关闭它们后再退出程序"), self)
                msg_box.cancelButton.hide()
                msg_box.exec()
            else:
                super().close()
        else:
            self.hide()
