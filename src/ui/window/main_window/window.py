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
from src.core.logging import CrashBundleNotification, LogSource, crash_bundle_notification_center, logger
from src.core.runtime.napcat import ManagerNapCatQQLoginState, ManagerNapCatQQProcess
from src.ui.common.icon import StaticIcon
from src.ui.components.info_bar import error_bar, info_bar, success_bar, warning_bar
from src.ui.page import BotPage, ComponentPage, HomeWidget, SetupWidget
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
        logger.trace("主窗口初始化开始", log_source=LogSource.UI)
        # 调用方法
        self._set_window()
        self._bind_core_events()
        self._bind_crash_bundle_events()
        self._set_item()
        self._set_tray_icon()

        # 组件加载完成结束 SplashScreen
        self.splash_screen.finish()
        logger.trace("主窗口初始化完成", log_source=LogSource.UI)

    def _set_window(self) -> None:
        """
        设置窗体
        """
        logger.trace("开始配置主窗口基础属性", log_source=LogSource.UI)
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
        logger.trace("主窗口已显示并完成初始绘制", log_source=LogSource.UI)

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
            interface=it(ComponentPage).initialize(self),
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
        logger.trace("主窗口托盘图标初始化完成", log_source=LogSource.UI)

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
        logger.trace("主窗口已完成 core 信号绑定", log_source=LogSource.UI)

    def _bind_crash_bundle_events(self) -> None:
        """绑定崩溃诊断包生成事件。"""
        if getattr(self, "_crash_bundle_events_bound", False):
            return

        crash_bundle_notification_center.crash_bundle_created.connect(self._show_crash_bundle_notification)
        self._crash_bundle_events_bound = True

        for notification in crash_bundle_notification_center.consume_pending():
            self._show_crash_bundle_notification(notification)

        logger.trace("主窗口已完成崩溃诊断包通知绑定", log_source=LogSource.UI)

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

    def _show_crash_bundle_notification(self, notification: CrashBundleNotification) -> None:
        """提示用户崩溃诊断包已生成。"""
        if not self.isVisible():
            return

        warning_bar(
            self.tr(
                f"检测到异常，已生成脱敏崩溃包\n{notification.bundle_path.name}\n"
                "如问题可复现，请携带该文件提交 Issue。"
            ),
            title=self.tr("已生成崩溃包"),
            duration=-1,
            parent=self,
        )
        info_bar(
            self.tr(f"输出位置:\n{notification.bundle_path}"),
            title=self.tr("诊断包位置"),
            duration=15000,
            parent=self,
        )

    def close(self) -> bool:
        """重写关闭事件"""
        close_action = cfg.get(cfg.close_button_action)
        logger.info(f"主窗口收到关闭请求, action={close_action.name}", log_source=LogSource.UI)
        if close_action == CloseActionEnum.CLOSE:

            # 如果有机器人在线, 则提示用户关闭实例
            if it(ManagerNapCatQQProcess).has_running_bot():
                # 项目内模块导入
                from src.ui.components.message_box import AskBox

                logger.warning("检测到仍有机器人运行，拒绝关闭主窗口", log_source=LogSource.UI)
                msg_box = AskBox(self.tr("无法退出"), self.tr("有机器人正在运行, 请关闭它们后再退出程序"), self)
                msg_box.cancelButton.hide()
                msg_box.exec()
                return False

            else:
                logger.info("主窗口执行实际关闭", log_source=LogSource.UI)
                return super().close()
        else:
            self.hide()
            logger.info("主窗口关闭行为切换为最小化到托盘", log_source=LogSource.UI)
            return False


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

