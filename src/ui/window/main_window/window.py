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
from src.core.runtime.bot_process_manager import ManagerNapCatQQLoginState, BotProcessManager
from src.ui.common.icon import StaticIcon
from src.ui.components.info_bar import error_bar, info_bar, success_bar, warning_bar
from src.ui.page import AgentChatPage, ApiDebugPage, BotPage, ComponentPage, HomeWidget, RemotePage, SetupWidget
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
        self._install_progress_info_bar_bridge()
        self._install_host_key_dialog_bridge()

        # 组件加载完成结束 SplashScreen
        self.splash_screen.finish()
        logger.trace("主窗口初始化完成", log_source=LogSource.UI)

    def _install_progress_info_bar_bridge(self) -> None:
        """挂载 [`ProgressInfoBarBridge`](src/ui/components/progress_info_bar_bridge.py).

        P3 perf: 让 [`BackgroundTaskCenter`](src/core/runtime/background_tasks.py) 的
        ``begin/end`` 自动在 MainWindow 右上角弹出 / 收尾 ``ProgressInfoBar``,
        BotPage Header 不再自维护状态条, BotCard 也不再嵌入进度环 - 全部走该桥.
        """
        # 项目内模块导入: 局部 import 避免主窗口顶层依赖 qfluentwidgets.ProgressInfoBar
        # (老版本 qfluentwidgets-qiao 没有该 symbol, 启动期保护性容错).
        from src.ui.components.progress_info_bar_bridge import ProgressInfoBarBridge

        self._progress_info_bar_bridge = ProgressInfoBarBridge(self)

    def _install_host_key_dialog_bridge(self) -> None:
        """启动期注册 [`HostKeyDialogBridge`](src/ui/components/host_key_confirm_dialog.py).

        P4 F5.1 缺失补齐: 把交互式主机指纹确认弹窗的回调挂上, 让
        ``host_key_policy="interactive"`` 的 SSH 连接能在首次未知指纹时弹窗,
        而不是无声兜底为 ``reject_all_callback`` 一律拒绝. 调用幂等, 多次无副作用.
        """
        # 局部 import: 避免主窗口顶层依赖 host_key_confirm_dialog (该模块本身在
        # 启动早期被引用时会触发 paramiko import, 而某些容器环境无 paramiko).
        from src.ui.components.host_key_confirm_dialog import bootstrap_host_key_dialog

        try:
            bootstrap_host_key_dialog()
        except Exception as exc:  # noqa: BLE001 - 任何注册失败都不应阻断主窗口启动
            logger.warning(
                f"HostKeyDialogBridge 注册失败 (远端 SSH 首次连接将兜底为拒绝): {exc!r}",
                log_source=LogSource.UI,
            )

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
            interface=it(AgentChatPage).initialize(self),
            icon=FluentIcon.CHAT,
            text=self.tr("Agent"),
            position=NavigationItemPosition.TOP,
        )
        self.addSubInterface(
            interface=it(RemotePage).initialize(self),
            icon=FluentIcon.GLOBE,
            text=self.tr("远程"),
            position=NavigationItemPosition.TOP,
        )
        self.addSubInterface(
            interface=it(ApiDebugPage).initialize(self),
            icon=FluentIcon.DEVELOPER_TOOLS,
            text=self.tr("接口文档"),
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

        process_manager = it(BotProcessManager)
        login_state_manager = it(ManagerNapCatQQLoginState)

        process_manager.notification_signal.connect(self._show_core_notification)
        login_state_manager.notification_signal.connect(self._show_core_notification)
        login_state_manager.qr_code_available_signal.connect(self._show_login_qr_code)
        login_state_manager.qr_code_removed_signal.connect(self._remove_login_qr_code)

        self._core_events_bound = True
        logger.trace("主窗口已完成 core 信号绑定", log_source=LogSource.UI)

    def _bind_crash_bundle_events(self) -> None:
        """绑定崩溃诊断包生成事件. """
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
        """提示用户崩溃诊断包已生成. """
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
        # 防重入: _graceful_shutdown_and_close 最后调 super().close() 时不再进入本方法
        if getattr(self, "_closing", False):
            return super().close()

        close_action = cfg.get(cfg.close_button_action)
        logger.info(f"主窗口收到关闭请求, action={close_action.name}", log_source=LogSource.UI)
        if close_action == CloseActionEnum.CLOSE:

            # 如果有机器人在线, 则提示用户关闭实例
            if it(BotProcessManager).has_running_bot():
                # 项目内模块导入
                from src.ui.components.message_box import AskBox

                logger.warning("检测到仍有机器人运行，拒绝关闭主窗口", log_source=LogSource.UI)
                msg_box = AskBox(self.tr("无法退出"), self.tr("有机器人正在运行, 请关闭它们后再退出程序"), self)
                msg_box.cancelButton.hide()
                msg_box.exec()
                return False

            else:
                logger.info("主窗口执行实际关闭 (进入 graceful shutdown)", log_source=LogSource.UI)
                self._closing = True
                self._graceful_shutdown_and_close()
                return True
        else:
            self.hide()
            logger.info("主窗口关闭行为切换为最小化到托盘", log_source=LogSource.UI)
            return False

    def _graceful_shutdown_and_close(self) -> None:
        """显示"正在关闭"提示框, 同步等待线程池排空, 然后退出.

        使用 MessageBoxBase.show() (非模态) + processEvents 保持 UI 刷新,
        避免嵌套事件循环带来的析构顺序问题.
        """
        import time

        from PySide6.QtCore import QThreadPool
        from qfluentwidgets import BodyLabel, IndeterminateProgressRing, MessageBoxBase, SubtitleLabel

        # ---- 主线程: 先停所有 QThread / 定时器 ----
        # 停 AgentWorker 的 asyncio 线程 (QThread 子类, 不停会触发 fatal)
        try:
            from src.ui.page.agent_page import AgentChatPage

            agent_page = self.findChild(AgentChatPage)
            if agent_page is not None and hasattr(agent_page, "_worker"):
                agent_page._worker.stop()
        except Exception:  # noqa: BLE001
            pass

        try:
            from src.core.runtime.snowluma_driver import SnowLumaDriver

            driver = it(SnowLumaDriver)
            for qq_id in list(driver._pollers.keys()):
                poller = driver._pollers.get(qq_id)
                if poller is not None:
                    try:
                        poller.stop()
                    except Exception:  # noqa: BLE001
                        pass
        except Exception:  # noqa: BLE001
            pass

        try:
            from src.core.runtime.bot_process_manager import ManagerNapCatQQLoginState

            login_mgr = it(ManagerNapCatQQLoginState)
            for login_state in list(login_mgr.napcat_login_state_dict.values()):
                try:
                    login_state.remove()
                except Exception:  # noqa: BLE001
                    pass
        except Exception:  # noqa: BLE001
            pass

        # ---- 提示框: spinner + 文字水平排列, 非模态 ----
        class _ShutdownDialog(MessageBoxBase):
            def __init__(self, parent):
                super().__init__(parent=parent)
                from PySide6.QtWidgets import QHBoxLayout

                self.title_label = SubtitleLabel(self.tr("正在关闭"), self)

                self.spinner = IndeterminateProgressRing(self)
                self.spinner.setFixedSize(24, 24)
                self.spinner.setStrokeWidth(3)

                self.content_label = BodyLabel(self.tr("正在等待后台任务结束, 请稍候..."), self)

                spinner_row = QHBoxLayout()
                spinner_row.setContentsMargins(0, 8, 0, 0)
                spinner_row.setSpacing(12)
                spinner_row.addWidget(self.spinner)
                spinner_row.addWidget(self.content_label, 1)

                self.viewLayout.addWidget(self.title_label)
                self.viewLayout.addSpacing(4)
                self.viewLayout.addLayout(spinner_row)
                self.widget.setMinimumWidth(360)

                self.yesButton.hide()
                self.cancelButton.hide()
                self.buttonGroup.hide()

        dialog = _ShutdownDialog(self)
        dialog.show()  # 非模态, 不开嵌套事件循环
        QApplication.processEvents()

        # ---- 同步等待线程池, 用 processEvents 保持 UI 刷新 ----
        try:
            from src.core.remote.thread_pool import shutdown_remote_ssh_pool

            shutdown_remote_ssh_pool(wait_ms=0)
        except Exception:  # noqa: BLE001
            pass

        pool = QThreadPool.globalInstance()
        deadline = time.monotonic() + 6.0  # 最多等 6 秒 (覆盖 httpx 5s timeout)

        # 用非阻塞的 activeThreadCount 轮询, 让 processEvents 持续驱动 spinner 动画
        while pool.activeThreadCount() > 0:
            QApplication.processEvents()
            time.sleep(0.016)  # ~60fps, 让出 CPU 但不阻塞主线程过久
            if time.monotonic() > deadline:
                logger.warning("Graceful shutdown 超时, 强制退出", log_source=LogSource.CORE)
                break

        dialog.accept()
        dialog.deleteLater()

        logger.info("Graceful shutdown 完成, 退出应用", log_source=LogSource.CORE)
        super().close()


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
