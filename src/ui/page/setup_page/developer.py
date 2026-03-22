# -*- coding: utf-8 -*-
# 标准库导入
import os
import subprocess
import threading
from collections.abc import Callable
from typing import TYPE_CHECKING

# 第三方库导入
from creart import it
from PySide6.QtCore import QTimer, Qt
from PySide6.QtWidgets import QMessageBox, QWidget
from qfluentwidgets import ExpandLayout, PushButton, ScrollArea, SettingCard, SettingCardGroup
from qfluentwidgets import FluentIcon as FI

# 项目内模块导入
from src.core.home import home_notice_debug_center
from src.core.desktop_update import MsiUpdateStrategy, inject_target_pid
from src.core.installation.install_type import detect_install_type
from src.core.logging import LogSource, logger
from src.core.runtime.paths import PathFunc
from src.core.runtime.napcat import ManagerNapCatQQLoginState, ManagerNapCatQQProcess
from src.core.desktop_update.templates import load_msi_update_script, load_portable_update_script
from src.ui.page.component_page.presentation import build_install_type_details, build_update_confirmation_message
from src.ui.components.info_bar import error_bar, info_bar, success_bar, warning_bar
from src.ui.components.input_card.generic_card import SwitchConfigCard
from src.ui.components.message_box import AskBox

if TYPE_CHECKING:
    # 避免循环导入
    from src.ui.window.main_window import MainWindow


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
        self.update_test_group = SettingCardGroup(title=self.tr("更新测试"), parent=self.view)

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

        # 更新测试按钮
        self.test_update_confirm_card = ActionButtonCard(
            icon=FI.UPDATE,
            title=self.tr("测试更新确认弹窗"),
            content=self.tr("模拟显示更新前的确认弹窗，测试 MSI/便携版的不同提示"),
            button_text=self.tr("显示确认弹窗"),
            callback=self._test_update_confirmation,
            parent=self.update_test_group,
        )
        self.test_install_type_card = ActionButtonCard(
            icon=FI.SEARCH,
            title=self.tr("检测安装类型"),
            content=self.tr("检测当前运行实例的安装类型（MSI/便携版/未知）"),
            button_text=self.tr("检测类型"),
            callback=self._test_detect_install_type,
            parent=self.update_test_group,
        )
        self.test_msi_script_card = ActionButtonCard(
            icon=FI.DOCUMENT,
            title=self.tr("测试 MSI 更新脚本"),
            content=self.tr("生成并打开 update_msi.bat 脚本供检查（不会执行）"),
            button_text=self.tr("生成脚本"),
            callback=self._test_generate_msi_script,
            parent=self.update_test_group,
        )
        self.test_portable_script_card = ActionButtonCard(
            icon=FI.FOLDER,
            title=self.tr("测试便携版更新脚本"),
            content=self.tr("生成并打开 update.bat 脚本供检查（不会执行）"),
            button_text=self.tr("生成脚本"),
            callback=self._test_generate_portable_script,
            parent=self.update_test_group,
        )

    def _set_layout(self) -> None:
        """控件布局。"""
        self.log_group.addSettingCard(self.trace_logging_card)
        self.crash_group.addSettingCard(self.export_bundle_card)
        self.crash_group.addSettingCard(self.thread_exception_card)
        self.crash_group.addSettingCard(self.main_exception_card)
        self.notice_group.addSettingCard(self.notice_batch_card)
        self.notice_group.addSettingCard(self.notice_clear_card)
        self.update_test_group.addSettingCard(self.test_update_confirm_card)
        self.update_test_group.addSettingCard(self.test_install_type_card)
        self.update_test_group.addSettingCard(self.test_msi_script_card)
        self.update_test_group.addSettingCard(self.test_portable_script_card)

        self.expand_layout.addWidget(self.log_group)
        self.expand_layout.addWidget(self.crash_group)
        self.expand_layout.addWidget(self.notice_group)
        self.expand_layout.addWidget(self.update_test_group)
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

    def _test_update_confirmation(self) -> None:
        """测试更新确认弹窗。

        模拟显示更新前的确认弹窗，展示 MSI 和便携版的不同提示文本，
        以及 Bot 状态检测结果的集成效果。
        """
        logger.info("开发者模式测试更新确认弹窗", log_source=LogSource.UI)

        # 检测当前安装类型
        install_type = detect_install_type()
        has_bot = it(ManagerNapCatQQProcess).has_running_bot()

        full_message = build_update_confirmation_message(
            translate=self.tr,
            install_type=install_type,
            local_version=None,
            remote_version=None,
            has_running_bot=has_bot,
            test_mode=True,
        )

        # 运行时动态导入避免循环导入
        from src.ui.window.main_window import MainWindow

        box = AskBox(self.tr("确认更新 [测试]"), full_message, it(MainWindow))
        box.yesButton.setText(self.tr("开始更新"))
        box.cancelButton.setText(self.tr("取消"))

        result = box.exec()
        if result:  # AskBox.exec() 返回 bool
            success_bar(self.tr('用户点击了"开始更新"'), parent=self)
            logger.info("用户确认更新（测试）", log_source=LogSource.UI)
        else:
            info_bar(self.tr('用户点击了"取消"'), parent=self)
            logger.info("用户取消更新（测试）", log_source=LogSource.UI)

    def _test_detect_install_type(self) -> None:
        """检测安装类型并显示详细信息。

        检测当前运行实例的安装类型（MSI/便携版/未知），
        并显示详细的诊断信息，包括注册表路径、_internal 目录等。
        """
        logger.info("开发者模式检测安装类型", log_source=LogSource.UI)

        install_type = detect_install_type()
        base_path = it(PathFunc).base_path

        details = build_install_type_details(self.tr, install_type, base_path)

        info_text = "\n".join(details)
        logger.info(f"安装类型检测结果:\n{info_text}", log_source=LogSource.UI)

        # 运行时动态导入避免循环导入
        from src.ui.window.main_window import MainWindow

        # 显示信息弹窗
        msg = QMessageBox(it(MainWindow))
        msg.setWindowTitle(self.tr("安装类型检测结果"))
        msg.setText(self.tr(f"检测结果: {install_type.value}"))
        msg.setInformativeText(info_text)
        msg.setIcon(QMessageBox.Icon.Information)
        msg.exec()

    def _test_generate_msi_script(self) -> None:
        """生成并显示 MSI 更新脚本（仅用于检查，不会执行）。

        从资源加载 update_msi.bat 模板，注入当前进程 PID，
        保存到临时目录并打开文件位置供检查。
        """
        logger.info("开发者模式生成 MSI 更新脚本", log_source=LogSource.UI)

        try:
            MsiUpdateStrategy()  # 保留实例化验证
            script_content = inject_target_pid(load_msi_update_script(), os.getpid())

            # 保存到临时目录
            tmp_path = it(PathFunc).tmp_path / "update_msi_test.bat"
            tmp_path.write_text(script_content, encoding="utf-8")

            success_bar(self.tr(f"MSI 脚本已生成: {tmp_path.name}"), parent=self)
            info_bar(self.tr(f"位置: {tmp_path}"), parent=self)
            logger.info(f"MSI 测试脚本已生成: {tmp_path}", log_source=LogSource.UI)

            # 打开文件位置
            subprocess.Popen(f'explorer /select,"{tmp_path}"', shell=True)

        except Exception as e:
            logger.error(f"生成 MSI 脚本失败: {e}", log_source=LogSource.UI)
            error_bar(self.tr(f"生成脚本失败: {e}"), parent=self)

    def _test_generate_portable_script(self) -> None:
        """生成并显示便携版更新脚本（仅用于检查，不会执行）。

        从 Qt 资源或文件系统加载 update.bat 模板，注入当前进程 PID，
        保存到临时目录并打开文件位置供检查。
        """
        logger.info("开发者模式生成便携版更新脚本", log_source=LogSource.UI)

        try:
            script_content = inject_target_pid(load_portable_update_script(), os.getpid())

            # 保存到临时目录
            tmp_path = it(PathFunc).tmp_path / "update_test.bat"
            tmp_path.write_text(script_content, encoding="utf-8")

            success_bar(self.tr(f"便携版脚本已生成: {tmp_path.name}"), parent=self)
            info_bar(self.tr(f"位置: {tmp_path}"), parent=self)
            logger.info(f"便携版测试脚本已生成: {tmp_path}", log_source=LogSource.UI)

            # 打开文件位置
            subprocess.Popen(f'explorer /select,"{tmp_path}"', shell=True)

        except Exception as e:
            logger.error(f"生成便携版脚本失败: {e}", log_source=LogSource.UI)
            error_bar(self.tr(f"生成脚本失败: {e}"), parent=self)

