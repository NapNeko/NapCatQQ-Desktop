# -*- coding: utf-8 -*-
# 标准库导入
from abc import ABC

# 第三方库导入
from creart import AbstractCreator, CreateTargetInfo, add_creator, exists_module, it
from qfluentwidgets import FluentIcon, MSFluentWindow, NavigationItemPosition, SplashScreen, Theme
from PySide6.QtCore import QSize
from PySide6.QtGui import QIcon
from PySide6.QtWidgets import QApplication

# 项目内模块导入
from src.core.config import cfg
from src.core.config.config_enum import CloseActionEnum
from src.core.utils.run_napcat import ManagerNapCatQQLoginState, ManagerNapCatQQProcess
from src.ui.components.info_bar import error_bar, info_bar, success_bar, warning_bar
from src.ui.common.icon import StaticIcon
from src.ui.page import BotPage, HomeWidget, SetupWidget, UnitWidget
from src.ui.page.bot_page.widget.msg_box import QRCodeDialogFactory
from src.ui.window.main_window.system_try_icon import SystemTrayIcon
from src.ui.window.main_window.title_bar import CustomTitleBar

"""NapCatQQ Desktop 主窗口模块

该模块定义了主窗口类 MainWindow, 继承自 MSFluentWindow

Attributes:
    MainWindow (MSFluentWindow): 主窗口类
"""


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
        self._bind_core_events()
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
        self.setWindowIcon(StaticIcon.LOGO.qicon())
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
            interface=it(HomeWidget).initialize(self),
            icon=FluentIcon.HOME,
            text=self.tr("主页"),
            position=NavigationItemPosition.TOP,
        )
        self.addSubInterface(
            interface=it(BotPage).initialize(self),
            icon=FluentIcon.ROBOT,
            text=self.tr("BOT"),
            position=NavigationItemPosition.TOP,
        )
        self.addSubInterface(
            interface=it(UnitWidget)._setup_ui(self),
            icon=FluentIcon.EMOJI_TAB_SYMBOLS,
            text=self.tr("组件"),
            position=NavigationItemPosition.BOTTOM,
        )
        self.addSubInterface(
            interface=it(SetupWidget).initialize(self),
            icon=FluentIcon.SETTING,
            text=self.tr("设置"),
            position=NavigationItemPosition.BOTTOM,
        )

    def _set_tray_icon(self):
        """设置托盘图标"""
        self.trayIcon = SystemTrayIcon(self)
        self.trayIcon.show()

    def _bind_core_events(self) -> None:
        """将 core 层信号桥接到 UI 表现层"""
        if getattr(self, "_core_events_bound", False):
            return

        process_manager = it(ManagerNapCatQQProcess)
        login_state_manager = it(ManagerNapCatQQLoginState)

        process_manager.notification_signal.connect(self._show_core_notification)
        login_state_manager.notification_signal.connect(self._show_core_notification)
        login_state_manager.qr_code_available_signal.connect(self._show_login_qr_code)
        login_state_manager.qr_code_removed_signal.connect(self._remove_login_qr_code)

        self._core_events_bound = True

    def _show_core_notification(self, level: str, message: str) -> None:
        """根据 core 层通知级别选择对应的 UI 提示方式"""
        mapping = {
            "info": info_bar,
            "success": success_bar,
            "warning": warning_bar,
            "error": error_bar,
        }
        mapping.get(level, info_bar)(message, parent=self)

    def _show_login_qr_code(self, qq_id: str, qr_code: str) -> None:
        """展示登录二维码"""
        it(QRCodeDialogFactory).add_qr_code(qq_id, qr_code)

    def _remove_login_qr_code(self, qq_id: str) -> None:
        """移除已失效的登录二维码"""
        it(QRCodeDialogFactory).remove_qr_code(qq_id)

    def close(self) -> None:
        """重写关闭事件"""
        if cfg.get(cfg.close_button_action) == CloseActionEnum.CLOSE:

            # 如果有机器人在线, 则提示用户关闭实例
            if it(ManagerNapCatQQProcess).has_running_bot():
                # 项目内模块导入
                from src.ui.components.message_box import AskBox

                msg_box = AskBox(self.tr("无法退出"), self.tr("有机器人正在运行, 请关闭它们后再退出程序"), self)
                msg_box.cancelButton.hide()
                msg_box.exec()

            else:
                super().close()
        else:
            self.hide()


class MainWindowCreator(AbstractCreator, ABC):
    """MainWindow 创建器"""

    targets = (
        CreateTargetInfo(
            module="src.ui.window.main_window.window",
            identify="MainWindow",
            humanized_name="主窗口",
            description="主窗口的创建器",
        ),
    )

    @staticmethod
    def available() -> bool:
        """判断 MainWindow 模块是否可用"""
        return exists_module("src.ui.window.main_window")

    @staticmethod
    def create(create_type):
        """创建 MainWindow 实例"""
        return create_type()


add_creator(MainWindowCreator)
