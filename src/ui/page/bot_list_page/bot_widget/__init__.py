# -*- coding: utf-8 -*-
# 标准库导入
from enum import Enum
from typing import Any, Dict, Optional

# 第三方库导入
import psutil
from qfluentwidgets import PushButton, ToolTipFilter, TransparentToolButton
from qfluentwidgets.common import FluentIcon
from qfluentwidgets.components import PrimaryPushButton, SegmentedWidget, ToolButton
from PySide6.QtCore import QProcess, QRegularExpression, Qt, QTimer, Signal, Slot
from PySide6.QtGui import QTextCursor
from PySide6.QtWidgets import QHBoxLayout, QStackedWidget, QVBoxLayout, QWidget

# 项目内模块导入
from src.core.config import cfg
from src.core.config.config_model import Config
from src.core.config.operate_config import delete_config, update_config
from src.core.network.email import offline_email
from src.core.network.webhook import offline_webhook
from src.core.utils.run_napcat import create_napcat_process
from src.ui.common.style_sheet import StyleSheet
from src.ui.components.code_editor.editor import CodeEditor
from src.ui.components.code_editor.highlight import LogHighlighter
from src.ui.components.info_bar import error_bar, info_bar, success_bar, warning_bar
from src.ui.components.message_box import AskBox, ImageBox
from src.ui.page.add_page.add_page_enum import ConnectType
from src.ui.page.add_page.msg_box import (
    HttpClientConfigDialog,
    HttpServerConfigDialog,
    HttpSSEServerConfigDialog,
    WebsocketClientConfigDialog,
    WebsocketServerConfigDialog,
)
from src.ui.page.bot_list_page.bot_widget.bot_setup_page import BotSetupPage
from src.ui.page.bot_list_page.bot_widget.msg_box import ChooseConfigTypeDialog
from src.ui.page.bot_list_page.signal_bus import bot_list_page_signal_bus


class BotWidget(QWidget):
    """机器人配置卡片控件，包含设置、日志等标签页"""

    choose_connect_type_signal = Signal(Enum)

    def __init__(self, config: Config) -> None:
        """初始化机器人控件

        Args:
            config: 机器人配置对象
        """
        super().__init__()
        self.config = config
        self.is_run = False  # 标记机器人是否在运行
        self.napcat_process: Optional[QProcess] = None  # 存储 QProcess 实例
        self.restart_timer = QTimer(self)

        # 创建控件
        self._create_view()
        self._create_pivot()
        self._create_button()
        self.v_box_layout = QVBoxLayout()
        self.h_box_layout = QHBoxLayout()
        self.button_layout = QHBoxLayout()

        # 调用方法
        self._set_layout()
        self._add_tool_tips()

        StyleSheet.BOT_WIDGET.apply(self)

    def _create_view(self) -> None:
        """创建并配置堆叠视图页面"""
        # 创建 view 和页面
        self.view = QStackedWidget()
        self.bot_info_page = QWidget(self)
        self.bot_info_page.setObjectName(f"{self.config.bot.QQID}_BotWidgetPivot_BotInfo")

        self.bot_setup_page = BotSetupPage(self.config, self)
        self.bot_setup_page.setObjectName(f"{self.config.bot.QQID}_BotWidgetPivot_BotSetup")

        self.bot_log_page = CodeEditor(self)
        self.bot_log_page.setObjectName(f"{self.config.bot.QQID}_BotWidgetPivot_BotLog")

        # 将页面添加到 view
        self.view.addWidget(self.bot_info_page)
        self.view.addWidget(self.bot_setup_page)
        self.view.addWidget(self.bot_log_page)
        self.view.setObjectName("BotView")
        self.view.setCurrentWidget(self.bot_setup_page)
        self.view.currentChanged.connect(self._on_pivot)

    def _create_pivot(self) -> None:
        """创建顶部导航栏"""
        self.pivot = SegmentedWidget(self)

        self.pivot.addItem(
            routeKey=self.bot_setup_page.objectName(),
            text=self.tr("设置"),
            onClick=lambda: self.view.setCurrentWidget(self.bot_setup_page),
        )
        self.pivot.addItem(
            routeKey=self.bot_log_page.objectName(),
            text=self.tr("NapCat 日志"),
            onClick=lambda: self.view.setCurrentWidget(self.bot_log_page),
        )

        self.pivot.setCurrentItem(self.bot_setup_page.objectName())
        self.pivot.setMaximumWidth(300)

    def _create_button(self) -> None:
        """创建并配置所有功能按钮"""
        # 创建按钮
        self.run_button = PrimaryPushButton(FluentIcon.POWER_BUTTON, self.tr("启动"))
        self.stop_button = PushButton(FluentIcon.POWER_BUTTON, self.tr("停止"))
        self.reboot_button = PrimaryPushButton(FluentIcon.UPDATE, self.tr("重启"))
        self.update_config_button = PrimaryPushButton(FluentIcon.UPDATE, self.tr("更新配置"))
        self.delete_config_button = ToolButton(FluentIcon.DELETE, self)
        self.return_button = TransparentToolButton(FluentIcon.RETURN, self)
        self.add_connect_config_button = PrimaryPushButton(FluentIcon.ADD, self.tr("添加连接配置"), self)

        # 连接槽函数
        self.run_button.clicked.connect(self.on_run_button)
        self.stop_button.clicked.connect(self.on_stop_button)
        self.reboot_button.clicked.connect(self.on_reboot_button)
        self.update_config_button.clicked.connect(self._on_update_button)
        self.delete_config_button.clicked.connect(self._on_delete_button)
        self.return_button.clicked.connect(self._on_return_button)
        self.add_connect_config_button.clicked.connect(self._on_add_connect_config_button_clicked)
        self.choose_connect_type_signal.connect(self._on_show_type_dialog)

        # 隐藏按钮
        self.stop_button.hide()
        self.reboot_button.hide()

    def _set_layout(self) -> None:
        """设置控件布局"""
        self.button_layout.setSpacing(8)
        self.button_layout.setContentsMargins(0, 0, 0, 0)
        self.button_layout.addWidget(self.run_button)
        self.button_layout.addWidget(self.stop_button)
        self.button_layout.addWidget(self.reboot_button)
        self.button_layout.addWidget(self.update_config_button)
        self.button_layout.addWidget(self.add_connect_config_button)
        self.button_layout.addWidget(self.delete_config_button)
        self.button_layout.addWidget(self.return_button)
        self.button_layout.setAlignment(Qt.AlignmentFlag.AlignRight)

        self.h_box_layout.setSpacing(0)
        self.h_box_layout.setContentsMargins(0, 0, 0, 0)
        self.h_box_layout.addWidget(self.pivot)
        self.h_box_layout.addLayout(self.button_layout)

        self.v_box_layout.setSpacing(0)
        self.v_box_layout.setContentsMargins(0, 0, 0, 0)
        self.v_box_layout.addLayout(self.h_box_layout)
        self.v_box_layout.addSpacing(10)
        self.v_box_layout.addWidget(self.view)

        self.setLayout(self.v_box_layout)

    def _add_tool_tips(self) -> None:
        """为按钮添加悬停提示"""
        # 返回按钮提示
        self.return_button.setToolTip(self.tr("返回"))
        self.return_button.installEventFilter(ToolTipFilter(self.return_button))

        # 删除按钮提示
        self.delete_config_button.setToolTip(self.tr("点击删除配置"))
        self.delete_config_button.installEventFilter(ToolTipFilter(self.delete_config_button))

        # 更新按钮提示
        self.update_config_button.setToolTip(self.tr("点击更新配置"))
        self.update_config_button.installEventFilter(ToolTipFilter(self.update_config_button))

        # 启动按钮提示
        self.run_button.setToolTip(self.tr("点击启动机器人"))
        self.run_button.installEventFilter(ToolTipFilter(self.run_button))

        # 停止按钮提示
        self.stop_button.setToolTip(self.tr("点击停止机器人"))
        self.stop_button.installEventFilter(ToolTipFilter(self.stop_button))

        # 重启按钮提示
        self.reboot_button.setToolTip(self.tr("点击重启机器人"))
        self.reboot_button.installEventFilter(ToolTipFilter(self.reboot_button))

        # 添加连接配置按钮提示
        self.add_connect_config_button.setToolTip(self.tr("添加连接配置"))
        self.add_connect_config_button.installEventFilter(ToolTipFilter(self.add_connect_config_button))

    # ==================== 公共方法 ====================
    # 注：此类暂无对外公开方法

    # ==================== 槽函数 ====================
    def on_run_button(self) -> None:
        """启动机器人按钮点击槽函数"""
        # 判断是否存在旧实例，如果不存在则创建新实例，存在则销毁后创建新实例
        if self.napcat_process is None:
            self.napcat_process = create_napcat_process(self.config)
        elif isinstance(self.napcat_process, QProcess):
            self.napcat_process.deleteLater()
            self.napcat_process = create_napcat_process(self.config)

        # NapCat 启动
        self.highlighter = LogHighlighter(self.bot_log_page.document())
        self.bot_log_page.clear()
        self.napcat_process.setParent(self)
        self.napcat_process.setProcessChannelMode(QProcess.ProcessChannelMode.MergedChannels)
        self.napcat_process.readyReadStandardOutput.connect(self._napcat_handle_stdout)
        self.napcat_process.finished.connect(self._on_napcat_process_finished)
        self.napcat_process.start()
        self.napcat_process.waitForStarted()

        # 参数调整
        self.is_run = True
        self.view.setCurrentWidget(self.bot_log_page)
        self.run_button.setVisible(False)
        self.stop_button.setVisible(True)
        self.reboot_button.setVisible(True)

        # 配置自动重启
        self._auto_restart()

        # 显示提示
        info_bar(self.tr("已执行启动命令，如果长时间没有输出，请查看日志"))

    def on_stop_button(self) -> None:
        """停止机器人按钮点击槽函数"""
        if not self.is_run:
            return

        if (parent := psutil.Process(self.napcat_process.processId())).pid != 0:
            [child.kill() for child in parent.children(recursive=True)]
            parent.kill()
            self.napcat_process.kill()
            self.napcat_process.waitForFinished()
            self.napcat_process.deleteLater()
            self.napcat_process = None

        self.is_run = False
        self.view.setCurrentWidget(self.bot_setup_page)

        # 停止自动重启
        self._stop_auto_restart()

    def on_reboot_button(self) -> None:
        """重启机器人按钮点击槽函数"""
        self._on_stop()
        self.on_run_button()

    def _on_update_button(self) -> None:
        """更新配置按钮点击槽函数"""
        self.config = Config(**self.bot_setup_page.get_value())
        if update_config(self.config):
            success_bar(self.tr("更新配置成功"))
        else:
            error_bar(self.tr("更新配置文件时引发错误，请前往 设置 > log 中查看详细错误"))

    def _on_delete_button(self) -> None:
        """删除配置按钮点击槽函数"""
        # 项目内模块导入
        from src.ui.window.main_window.window import MainWindow

        if self.is_run:
            warning_bar(self.tr("NapCat 还在运行，请停止运行再进行操作"))
            return

        if AskBox(
            self.tr("确认删除"),
            self.tr(f"你确定要删除 {self.config.bot.QQID} 吗? \n\n此操作无法撤消，请谨慎操作"),
            MainWindow(),
        ).exec():
            # 项目内模块导入
            from src.ui.page.bot_list_page.bot_list_widget import BotListWidget

            if delete_config(self.config):
                # 删除成功后的操作
                self.return_button.clicked.emit()
                BotListWidget().bot_list.update_list()
                success_bar(self.tr(f"成功删除配置 {self.config.bot.QQID}({self.config.bot.name})"))

                self.return_button.click()
                self.close()

                # 处理 TabBar
                # 项目内模块导入
                from src.ui.window.main_window.window import MainWindow

                MainWindow().title_bar.tab_bar.removeTabByKey(f"{self.config.bot.QQID}")
            else:
                error_bar(self.tr("删除配置文件时引发错误，请前往 设置 > log 查看错误原因"))

    def _on_return_button(self) -> None:
        """返回按钮点击槽函数"""
        if self.view.currentWidget() in [self.bot_info_page, self.bot_setup_page, self.bot_log_page]:
            # 项目内模块导入
            from src.ui.page.bot_list_page.bot_list_widget import BotListWidget

            BotListWidget().view.setCurrentIndex(0)
            BotListWidget().top_card.breadcrumb_bar.setCurrentIndex(0)
            BotListWidget().top_card.update_list_button.show()
        else:
            self.view.setCurrentWidget(self.bot_setup_page)

    def _on_add_connect_config_button_clicked(self) -> None:
        """添加连接配置按钮点击槽函数"""
        # 项目内模块导入
        from src.ui.window.main_window import MainWindow

        ChooseConfigTypeDialog(MainWindow(), self.choose_connect_type_signal).exec()

    def _on_show_type_dialog(self, connect_type: ConnectType) -> None:
        """显示连接类型选择对话框槽函数

        Args:
            connect_type: 连接类型枚举值
        """
        # 项目内模块导入
        from src.ui.window.main_window import MainWindow

        dialog_map = {
            ConnectType.HTTP_SERVER: HttpServerConfigDialog,
            ConnectType.HTTP_SSE_SERVER: HttpSSEServerConfigDialog,
            ConnectType.HTTP_CLIENT: HttpClientConfigDialog,
            ConnectType.WEBSOCKET_SERVER: WebsocketServerConfigDialog,
            ConnectType.WEBSOCKET_CLIENT: WebsocketClientConfigDialog,
        }

        dialog_class = dialog_map.get(connect_type)
        if dialog_class:
            dialog = dialog_class(MainWindow())
            if dialog.exec():
                self.bot_setup_page.connect_widget.add_card(dialog.get_config())

    def _on_pivot(self, index: int) -> None:
        """导航栏切换槽函数

        Args:
            index: 当前页面索引
        """
        widget = self.view.widget(index)
        self.pivot.setCurrentItem(widget.objectName())

        # 定义页面对应的操作字典
        page_actions = {
            self.bot_info_page.objectName(): {
                "return_button": "show",
                "update_config_button": "hide",
                "delete_config_button": "hide",
                "run_button": "hide" if self.is_run else "show",
                "stop_button": "show" if self.is_run else "hide",
                "reboot_button": "show" if self.is_run else "hide",
                "add_connect_config_button": "hide" if self.is_run else "hide",
            },
            self.bot_setup_page.objectName(): {
                "return_button": "show",
                "update_config_button": "show",
                "delete_config_button": "show",
                "run_button": "hide" if self.is_run else "show",
                "stop_button": "show" if self.is_run else "hide",
                "reboot_button": "show" if self.is_run else "hide",
                "add_connect_config_button": "show" if self.is_run else "show",
            },
            self.bot_log_page.objectName(): {
                "return_button": "show",
                "update_config_button": "hide",
                "delete_config_button": "hide",
                "run_button": "hide" if self.is_run else "show",
                "stop_button": "show" if self.is_run else "hide",
                "reboot_button": "show" if self.is_run else "hide",
                "add_connect_config_button": "hide" if self.is_run else "hide",
            },
        }

        # 根据 widget.objectName() 执行相应操作
        if widget.objectName() in page_actions:
            for button, action in page_actions[widget.objectName()].items():
                getattr(self, button).setVisible(action == "show")

    # ==================== 辅助方法 ====================
    def _napcat_handle_stdout(self) -> None:
        """处理 NapCat 标准输出，解析日志并执行相应操作"""
        # 获取所有输出
        data = self.napcat_process.readAllStandardOutput().data().decode()

        # 遍历所有匹配项并移除 ANSI 转义码
        while (matches := QRegularExpression(r"\x1B(?:[@-Z\\-_]|\[[0-?]*[ -/]*[@-~])").globalMatch(data)).hasNext():
            data = data.replace(matches.next().captured(0), "")
            data = data.replace("\n\n", "\n")

        # 获取 CodeEditor 的 cursor 并移动插入输出内容
        cursor = self.bot_log_page.textCursor()
        cursor.movePosition(QTextCursor.MoveOperation.End)
        cursor.insertText(data)
        self.bot_log_page.setTextCursor(cursor)

        # 分发给其他处理函数
        self._get_qr_code(data)
        self._get_bof_offline(data)

    def _on_stop(self) -> None:
        """停止机器人槽函数"""
        if not self.is_run:
            return

        if (parent := psutil.Process(self.napcat_process.processId())).pid != 0:
            [child.kill() for child in parent.children(recursive=True)]
            parent.kill()
            self.napcat_process.kill()
            self.napcat_process.waitForFinished()
            self.napcat_process.deleteLater()
            self.napcat_process = None

        self.is_run = False

    def _auto_restart(self) -> None:
        """设置自动重启"""

        if not self.config.bot.autoRestartSchedule.enable:
            return

        if self.restart_timer.isActive():
            # 如果定时器已存在, 则跳过
            return

        # 项目内模块导入
        from src.core.config.config_enum import TimeUnitEnum

        # 获取时间单位和持续时间
        time_unit: TimeUnitEnum = self.config.bot.autoRestartSchedule.time_unit
        duration: int = self.config.bot.autoRestartSchedule.duration

        # 转换为秒
        unit_to_seconds = {
            TimeUnitEnum.MINUTE: 60,
            TimeUnitEnum.HOUR: 3600,
            TimeUnitEnum.DAY: 86400,
            TimeUnitEnum.MONTH: 2592000,
            TimeUnitEnum.YEAR: 31536000,
        }
        total_seconds = duration * unit_to_seconds.get(time_unit, 0)

        self.restart_timer.timeout.connect(self.on_reboot_button)
        self.restart_timer.start(total_seconds * 1000)

        # 显示提示
        info_bar(self.tr(f"已设置自动重启，间隔为 {duration} {time_unit.value}"))

    def _stop_auto_restart(self) -> None:
        """停止自动重启"""
        if hasattr(self, "restart_timer") and self.restart_timer.isActive():
            self.restart_timer.stop()
            info_bar(self.tr("已停止自动重启"))

    @Slot(int, int)
    def _on_napcat_process_finished(self, exit_code: int, exit_status: int) -> None:
        """NapCat 进程结束槽函数

        Args:
            exit_code: 退出码
            exit_status: 退出状态
        """
        cursor = self.bot_log_page.textCursor()
        cursor.movePosition(QTextCursor.MoveOperation.End)
        cursor.insertText(f"进程结束，退出码为 {exit_code}，状态为 {exit_status}")
        self.bot_log_page.setTextCursor(cursor)

    def _get_qr_code(self, data: str) -> None:
        """从输出中解析二维码信息并显示

        Args:
            data: 日志输出数据
        """
        if "二维码已保存到" in data:
            # 项目内模块导入
            from src.ui.window.main_window import MainWindow

            pattern = rf"[a-zA-Z]:\\(?:[^\\\s]+\\)*[^\\\s]+"
            if not (match := QRegularExpression(pattern).match(data)).hasMatch():
                return

            box = ImageBox(self.tr("请扫码登陆"), match.captured(0), MainWindow())
            box.cancelButton.hide()
            box.image_label.scaledToHeight(256)
            box.exec()

    def _get_bof_offline(self, data: str) -> None:
        """检测用户离线状态并发送通知

        Args:
            data: 日志输出数据
        """
        if not "账号状态变更为离线" in data:
            return

        if not self.config.advanced.offlineNotice:
            return

        if cfg.get(cfg.bot_offline_email_notice):
            offline_email(self.config)
        if cfg.get(cfg.bot_offline_web_hook_notice):
            offline_webhook(self.config)

        warning_bar(self.tr(f"账号 {self.config.bot.QQID} 已离线，请前往 NapCat 日志 查看详情"))
