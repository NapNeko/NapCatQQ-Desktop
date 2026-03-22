# -*- coding: utf-8 -*-
# 标准库导入
from abc import ABC

# 第三方库导入
from creart import AbstractCreator, CreateTargetInfo, add_creator, exists_module, it
from qfluentwidgets.components.widgets.stacked_widget import PopUpAniStackedWidget
from qframelesswindow import FramelessWindow
from PySide6.QtCore import Qt
from PySide6.QtWidgets import QApplication, QVBoxLayout

# 项目内模块导入
from src.core.config import cfg
from src.core.logging import CrashBundleNotification, LogSource, crash_bundle_notification_center, logger
from src.ui.common.icon import StaticIcon
from src.ui.components.info_bar import info_bar, warning_bar
from src.ui.window.guide_window.ask_page import AskPage
from src.ui.window.guide_window.eula_page import EulaPage
from src.ui.window.guide_window.finsh_page import FinshPage
from src.ui.window.guide_window.install_page import InstallNapCatQQPage, InstallQQPage
from src.ui.window.guide_window.welcome_page import WelcomePage

"""引导用户执行初始化操作模块

Attributes:
    GuideWindow: 引导用户执行初始化操作窗体
"""


class GuideWindow(FramelessWindow):
    """引导用户执行初始化操作窗体

    Attributes:
        view (FadeEffectAniStackedWidget): 页面切换组件
        eula_page (EulaPage): 用户协议页面
        welcome_page (WelcomePage): 欢迎页面
        ask_page (AskPage): 询问页面
        install_qq_page (InstallQQPage): 安装 QQ 页面
        install_nc_page (InstallNapCatQQPage): 安装 NapCatQQ 页面
        finish_page (FinshPage): 完成页面
        vBoxLayout (QVBoxLayout): 布局管理器
        splashScreen (SplashScreen): 启动画面
    """

    def __init__(self) -> None:
        """初始化"""
        super().__init__()

    def initialize(self) -> None:
        """初始化方法

        调整窗体大小、位置、布局等
        """
        logger.trace("引导窗口初始化开始", log_source=LogSource.UI)
        self.show()

        # 设置窗体图标
        self.setWindowIcon(StaticIcon.NAPCAT.qicon())

        # 设置窗体大小以及设置打开时居中
        self.setFixedSize(600, 450)
        desktop = QApplication.screens()[0].availableGeometry()
        width, height = desktop.width(), desktop.height()
        self.move(width // 2 - self.width() // 2, height // 2 - self.height() // 2)

        # 隐藏无用的东西
        self.titleBar.hide()

        # 挂起
        QApplication.processEvents()

        # 调用方法
        self.create_page()
        self.create_layout()
        self.bind_crash_bundle_events()
        logger.trace("引导窗口初始化完成", log_source=LogSource.UI)

    def create_page(self) -> None:
        """创建页面

        页面顺序：欢迎页面 -> 询问页面 -> 安装 QQ 页面 -> 安装 NapCatQQ 页面 -> 完成页面
        通过 FadeEffectAniStackedWidget 实现页面切换时的淡入淡出效果
        通过 on_next_page 方法实现页面的切换
        """
        self.view = PopUpAniStackedWidget(self)
        self.elua_page = EulaPage(self)
        self.welcome_page = WelcomePage(self)
        self.ask_page = AskPage(self)
        self.install_qq_page = InstallQQPage(self)
        self.install_nc_page = InstallNapCatQQPage(self)
        self.finish_page = FinshPage(self)

        self.view.addWidget(self.elua_page)
        self.view.addWidget(self.welcome_page)
        self.view.addWidget(self.ask_page)
        self.view.addWidget(self.install_qq_page)
        self.view.addWidget(self.install_nc_page)
        self.view.addWidget(self.finish_page)

        self.view.setCurrentWidget(self.elua_page)
        logger.trace("引导窗口页面已创建，当前页=EulaPage", log_source=LogSource.UI)

    def create_layout(self) -> None:
        """创建布局

        使用 QVBoxLayout 布局管理器将 FadeEffectAniStackedWidget 添加到窗体中
        """
        self.vBoxLayout = QVBoxLayout()
        self.vBoxLayout.setContentsMargins(0, 0, 0, 0)
        self.vBoxLayout.setSpacing(0)
        self.vBoxLayout.addWidget(self.view)
        self.setLayout(self.vBoxLayout)
        logger.trace("引导窗口布局创建完成", log_source=LogSource.UI)

    def bind_crash_bundle_events(self) -> None:
        """绑定崩溃诊断包生成事件。"""
        if getattr(self, "_crash_bundle_events_bound", False):
            return

        crash_bundle_notification_center.crash_bundle_created.connect(self.show_crash_bundle_notification)
        self._crash_bundle_events_bound = True

        for notification in crash_bundle_notification_center.consume_pending():
            self.show_crash_bundle_notification(notification)

        logger.trace("引导窗口已完成崩溃诊断包通知绑定", log_source=LogSource.UI)

    def show_crash_bundle_notification(self, notification: CrashBundleNotification) -> None:
        """提示用户崩溃诊断包已生成。"""
        if not self.isVisible():
            return

        warning_bar(
            self.tr(
                f"检测到异常，已生成脱敏崩溃包 {notification.bundle_path.name}。"
                "如果问题可复现或功能异常，请携带该文件提交 Issue。"
            ),
            title=self.tr("已生成崩溃包"),
            duration=-1,
            parent=self,
        )
        info_bar(
            self.tr(f"输出位置: {notification.bundle_path}"),
            title=self.tr("诊断包位置"),
            duration=15000,
            parent=self,
        )

    def close(self) -> bool:
        """关闭窗体

        关闭窗体时将窗体设置为不可见，并打开主窗体
        """
        logger.info("引导窗口关闭，准备切换到主窗口", log_source=LogSource.UI)

        # 关闭时将窗体设置为不可见
        self.hide()

        # 项目内模块导入
        from src.ui.window.main_window import MainWindow

        it(MainWindow).initialize()
        cfg.set(cfg.main_window, True)
        logger.info("引导窗口已切换到主窗口并写入主窗口状态", log_source=LogSource.UI)

        return super().close()

    def mousePressEvent(self, event):
        """重写鼠标按下事件"""
        if event.button() == Qt.MouseButton.LeftButton:
            self._drag_pos = event.globalPosition().toPoint() - self.pos()
            event.accept()

    def mouseMoveEvent(self, event):
        """重写鼠标移动事件"""
        if event.buttons() == Qt.MouseButton.LeftButton and self._drag_pos is not None:
            self.move(event.globalPosition().toPoint() - self._drag_pos)
            event.accept()

    def mouseReleaseEvent(self, event):
        """重写鼠标释放事件"""
        if event.button() == Qt.MouseButton.LeftButton:
            self._drag_pos = None
            event.accept()

    def on_next_page(self) -> None:
        """切换到下一个页面

        页面顺序：欢迎页面 -> 询问页面 -> 安装 QQ 页面 -> 安装 NapCatQQ 页面 -> 完成页面
        """
        current_name = type(self.view.currentWidget()).__name__
        target_name = current_name
        if isinstance(self.view.currentWidget(), EulaPage):
            self.view.setCurrentWidget(self.welcome_page)
            target_name = type(self.welcome_page).__name__
        elif isinstance(self.view.currentWidget(), WelcomePage):
            self.view.setCurrentWidget(self.ask_page)
            target_name = type(self.ask_page).__name__
        elif isinstance(self.view.currentWidget(), AskPage):
            self.view.setCurrentWidget(self.install_qq_page)
            target_name = type(self.install_qq_page).__name__
        elif isinstance(self.view.currentWidget(), InstallQQPage):
            self.view.setCurrentWidget(self.install_nc_page)
            target_name = type(self.install_nc_page).__name__
        elif isinstance(self.view.currentWidget(), InstallNapCatQQPage):
            self.view.setCurrentWidget(self.finish_page)
            target_name = type(self.finish_page).__name__

        if target_name != current_name:
            logger.trace(f"引导窗口页面切换: {current_name} -> {target_name}", log_source=LogSource.UI)


class GuideWindowCreator(AbstractCreator, ABC):
    """引导窗体创建类"""

    targets = (
        CreateTargetInfo(
            module="src.ui.window.guide_window.guide_window",
            identify="GuideWindow",
            humanized_name="引导窗体",
            description="NapCatQQ Desktop 引导窗体",
        ),
    )

    @staticmethod
    def available() -> bool:
        """检查创建器是否可用"""
        return exists_module("src.ui.window.guide_window.guide_window")

    @staticmethod
    def create(create_type):
        """创建引导窗体实例"""
        return create_type()


add_creator(GuideWindowCreator)

