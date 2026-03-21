# -*- coding: utf-8 -*-
# 标准库导入
import threading
from collections.abc import Callable

# 第三方库导入
from creart import it
from qfluentwidgets import ExpandLayout, PushButton, ScrollArea, SettingCard, SettingCardGroup
from qfluentwidgets import FluentIcon as FI
from PySide6.QtCore import QTimer, Qt
from PySide6.QtWidgets import QWidget

# 项目内模块导入
from src.core.utils.logger import LogSource, logger
from src.core.home import home_notice_debug_center
from src.core.utils.run_napcat import ManagerNapCatQQLoginState, ManagerNapCatQQProcess
from src.ui.components.info_bar import error_bar, info_bar, success_bar, warning_bar
from src.ui.components.input_card.generic_card import SwitchConfigCard


class ActionButtonCard(SettingCard):
    """带单个操作按钮的设置卡片。"""

    def __init__(
        self,
        icon,
        title: str,
        button_text: str,
        callback: Callable[[], None],
        content: str | None = None,
        parent: QWidget | None = None,
    ) -> None:
        super().__init__(icon, title, content, parent)
        self._callback = callback
        self.button = PushButton(text=button_text, parent=self)
        self.button.clicked.connect(self._callback)
        self.hBoxLayout.addWidget(self.button, 0, Qt.AlignmentFlag.AlignRight)
        self.hBoxLayout.addSpacing(16)


class Developer(ScrollArea):
    """开发者工具页面。"""

    _TEST_QQ_ID = "114514"

    def __init__(self, parent) -> None:
        super().__init__(parent=parent)
        self.view = QWidget()
        self.expand_layout = ExpandLayout(self.view)

        self.setWidget(self.view)
        self.setWidgetResizable(True)
        self.view.setObjectName("SetupView")
        self.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        self.setObjectName("SetupDeveloperWidget")

        self._create_config_cards()
        self._set_layout()

    def _create_config_cards(self) -> None:
        """创建开发者配置卡片。"""
        self.log_group = SettingCardGroup(title=self.tr("日志"), parent=self.view)
        self.trace_logging_card = SwitchConfigCard(
            FI.DEVELOPER_TOOLS,
            self.tr("启用 TRACE 日志"),
            self.tr("仅对当前会话生效，用于输出更详细的开发者日志"),
            logger.is_trace_logging_enabled(),
            self.log_group,
        )
        self.crash_group = SettingCardGroup(title=self.tr("崩溃诊断"), parent=self.view)
        self.notice_group = SettingCardGroup(title=self.tr("首页通知"), parent=self.view)
        self.export_bundle_card = ActionButtonCard(
            icon=FI.DEVELOPER_TOOLS,
            title=self.tr("生成脱敏崩溃包"),
            content=self.tr("立即导出一个脱敏后的诊断包，不会让程序退出"),
            button_text=self.tr("立即导出"),
            callback=self._export_test_crash_bundle,
            parent=self.crash_group,
        )
        self.thread_exception_card = ActionButtonCard(
            icon=FI.CODE,
            title=self.tr("触发线程未处理异常"),
            content=self.tr("测试 threading.excepthook，程序通常不会退出"),
            button_text=self.tr("触发线程异常"),
            callback=self._trigger_thread_exception,
            parent=self.crash_group,
        )
        self.main_exception_card = ActionButtonCard(
            icon=FI.CLOSE,
            title=self.tr("触发主线程未处理异常"),
            content=self.tr("测试主线程异常钩子，程序会在当前会话退出"),
            button_text=self.tr("触发主线程异常"),
            callback=self._trigger_main_thread_exception,
            parent=self.crash_group,
        )
        self.notice_batch_card = ActionButtonCard(
            icon=FI.RINGER,
            title=self.tr("触发首页通知样例"),
            content=self.tr("向首页通知时间线和右上角提示同时注入一组测试通知"),
            button_text=self.tr("触发通知"),
            callback=self._trigger_home_notice_test,
            parent=self.notice_group,
        )
        self.notice_clear_card = ActionButtonCard(
            icon=FI.CANCEL,
            title=self.tr("清除测试扫码通知"),
            content=self.tr("移除开发者模式注入的测试扫码提醒"),
            button_text=self.tr("清除通知"),
            callback=self._clear_home_notice_test,
            parent=self.notice_group,
        )

    def _set_layout(self) -> None:
        """控件布局。"""
        self.log_group.addSettingCard(self.trace_logging_card)
        self.crash_group.addSettingCard(self.export_bundle_card)
        self.crash_group.addSettingCard(self.thread_exception_card)
        self.crash_group.addSettingCard(self.main_exception_card)
        self.notice_group.addSettingCard(self.notice_batch_card)
        self.notice_group.addSettingCard(self.notice_clear_card)

        self.expand_layout.addWidget(self.log_group)
        self.expand_layout.addWidget(self.crash_group)
        self.expand_layout.addWidget(self.notice_group)
        self.expand_layout.setContentsMargins(0, 0, 0, 0)
        self.view.setLayout(self.expand_layout)

        self.trace_logging_card.switchButton.checkedChanged.connect(self._on_trace_logging_changed)

    def _on_trace_logging_changed(self, enabled: bool) -> None:
        """切换开发者 TRACE 日志开关。"""
        logger.set_trace_logging_enabled(enabled)
        logger.info(f"开发者模式切换 TRACE 日志: enabled={enabled}", log_source=LogSource.UI)
        if enabled:
            info_bar(self.tr("TRACE 日志已启用"), parent=self)
            logger.trace("开发者模式 TRACE 日志已启用，后续将输出更详细上下文", log_source=LogSource.UI)
        else:
            info_bar(self.tr("TRACE 日志已关闭"), parent=self)

    def _export_test_crash_bundle(self) -> None:
        """手动生成一份测试用诊断包。"""
        logger.info("开发者模式触发手动崩溃包导出", log_source=LogSource.UI)
        bundle_path = logger.emit_test_crash_bundle()
        if bundle_path is None:
            error_bar(self.tr("生成诊断包失败，请查看日志"), parent=self)
            return

        success_bar(self.tr(f"已生成脱敏诊断包: {bundle_path.name}"), parent=self)
        info_bar(self.tr(f"输出位置: {bundle_path.parent}"), parent=self)

    def _trigger_thread_exception(self) -> None:
        """触发线程未处理异常测试。"""
        logger.warning("开发者模式触发线程未处理异常测试", log_source=LogSource.UI)
        warning_bar(self.tr("已触发线程异常测试，请检查日志和桌面诊断包"), parent=self)

        def raise_unhandled_exception() -> None:
            raise RuntimeError("Developer mode thread exception test")

        threading.Thread(target=raise_unhandled_exception, name="developer-thread-crash-test", daemon=True).start()

    def _trigger_main_thread_exception(self) -> None:
        """触发主线程未处理异常测试。"""
        logger.warning("开发者模式触发主线程未处理异常测试", log_source=LogSource.UI)
        warning_bar(self.tr("即将触发主线程未处理异常，当前进程会退出"), parent=self)
        QTimer.singleShot(150, self._raise_main_thread_exception)

    def _trigger_home_notice_test(self) -> None:
        """触发首页通知测试。"""
        logger.info("开发者模式触发首页通知测试", log_source=LogSource.UI)

        process_manager = it(ManagerNapCatQQProcess)
        login_state_manager = it(ManagerNapCatQQLoginState)

        process_manager.notification_signal.emit("info", "开发者模式: 检测到 NapCat 可更新到 v9.9.99。")
        process_manager.notification_signal.emit("warning", "开发者模式: 测试 Bot 当前处于离线状态。")
        process_manager.notification_signal.emit("error", "开发者模式: 测试启动失败，请检查 QQ 安装路径。")
        login_state_manager.notification_signal.emit("success", "开发者模式: 已发送测试离线通知到配置的邮箱地址。")
        login_state_manager.qr_code_available_signal.emit(self._TEST_QQ_ID, "developer://home-notice-test")
        home_notice_debug_center.sampleRequested.emit()

        success_bar(self.tr("已注入首页通知测试数据"), parent=self)
        info_bar(self.tr("可切换到主页检查提醒、更新和扫码通知展示"), parent=self)

    def _clear_home_notice_test(self) -> None:
        """清除首页通知测试。"""
        logger.info("开发者模式清除首页通知测试", log_source=LogSource.UI)
        it(ManagerNapCatQQLoginState).qr_code_removed_signal.emit(self._TEST_QQ_ID)
        home_notice_debug_center.clearRequested.emit()
        success_bar(self.tr("已清除测试扫码通知"), parent=self)

    @staticmethod
    def _raise_main_thread_exception() -> None:
        """在 Qt 主线程中抛出未处理异常。"""
        raise RuntimeError("Developer mode main thread exception test")
