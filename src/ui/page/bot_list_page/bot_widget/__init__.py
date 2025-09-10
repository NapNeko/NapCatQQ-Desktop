# -*- coding: utf-8 -*-
# 标准库导入
from typing import Optional

# 第三方库导入
import psutil
from qfluentwidgets import PushButton, ToolTipFilter, TransparentToolButton
from qfluentwidgets.common import FluentIcon
from qfluentwidgets.components import PrimaryPushButton, SegmentedWidget, ToolButton
from PySide6.QtCore import QProcess, QRegularExpression, Qt, Slot
from PySide6.QtGui import QTextCursor
from PySide6.QtWidgets import QHBoxLayout, QStackedWidget, QVBoxLayout, QWidget

# 项目内模块导入
from src.core.config import cfg
from src.core.config.config_model import Config
from src.core.config.operate_config import delete_config, update_config
from src.core.network.email import offline_email
from src.core.network.webhook import offline_webhook
from src.core.utils.run_napCat import create_napcat_process
from src.ui.common.style_sheet import StyleSheet
from src.ui.components.code_editor.editor import CodeEditor
from src.ui.components.code_editor.highlight import LogHighlighter
from src.ui.components.info_bar import error_bar, info_bar, success_bar, warning_bar
from src.ui.components.message_box import AskBox, ImageBox
from src.ui.page.add_page.enum import ConnectType
from src.ui.page.add_page.msg_box import (
    HttpClientConfigDialog,
    HttpServerConfigDialog,
    HttpSSEServerConfigDialog,
    WebsocketClientConfigDialog,
    WebsocketServerConfigDialog,
)
from src.ui.page.bot_list_page.bot_widget.bot_setup_page import BotSetupPage
from src.ui.page.bot_list_page.bot_widget.meg_box import ChooseConfigTypeDialog
from src.ui.page.bot_list_page.signal_bus import botListPageSignalBus


class BotWidget(QWidget):
    """
    ## 机器人卡片对应的 Widget
    """

    def __init__(self, config: Config) -> None:
        super().__init__()
        self.config = config
        self.isRun = False  # 用于标记机器人是否在运行
        self.napcatProcess: Optional[QProcess] = None  # 用于存储 QProcess 实例

        # 创建所需控件
        self._createView()
        self._createPivot()
        self._createButton()
        self.vBoxLayout = QVBoxLayout()
        self.hBoxLayout = QHBoxLayout()
        self.buttonLayout = QHBoxLayout()

        # 调用方法
        self._setLayout()
        self._addTooltips()

        StyleSheet.BOT_WIDGET.apply(self)

    def _createPivot(self) -> None:
        """
        ## 创建机器人 Widget 顶部导航栏
            - routeKey 使用 QQ号 作为前缀保证该 pivot 的 objectName 全局唯一
        """
        self.pivot = SegmentedWidget(self)

        self.pivot.addItem(
            routeKey=self.botSetupPage.objectName(),
            text=self.tr("设置"),
            onClick=lambda: self.view.setCurrentWidget(self.botSetupPage),
        )
        self.pivot.addItem(
            routeKey=self.botLogPage.objectName(),
            text=self.tr("NapCat 日志"),
            onClick=lambda: self.view.setCurrentWidget(self.botLogPage),
        )
        # self.pivot.addItem(
        #     routeKey=self.botInfoPage.objectName(),
        #     text=self.tr("Bot info"),
        #     onClick=lambda: self.view.setCurrentWidget(self.botInfoPage)
        # )

        self.pivot.setCurrentItem(self.botSetupPage.objectName())
        self.pivot.setMaximumWidth(300)

    def _createView(self) -> None:
        """
        ## 创建用于切换页面的 view
        """
        # 创建 view 和 页面
        self.view = QStackedWidget()
        self.botInfoPage = QWidget(self)
        self.botInfoPage.setObjectName(f"{self.config.bot.QQID}_BotWidgetPivot_BotInfo")

        self.botSetupPage = BotSetupPage(self.config, self)
        self.botSetupPage.setObjectName(f"{self.config.bot.QQID}_BotWidgetPivot_BotSetup")

        self.botLogPage = CodeEditor(self)
        self.botLogPage.setObjectName(f"{self.config.bot.QQID}_BotWidgetPivot_BotLog")

        # 将页面添加到 view
        self.view.addWidget(self.botInfoPage)
        self.view.addWidget(self.botSetupPage)
        self.view.addWidget(self.botLogPage)
        self.view.setObjectName("BotView")
        self.view.setCurrentWidget(self.botSetupPage)
        self.view.currentChanged.connect(self._pivotSlot)

    def _createButton(self) -> None:
        """
        ## 创建按钮并设置
        """
        # 创建按钮
        self.runButton = PrimaryPushButton(FluentIcon.POWER_BUTTON, self.tr("启动"))  # 启动按钮
        self.stopButton = PushButton(FluentIcon.POWER_BUTTON, self.tr("停止"))  # 停止按钮
        self.rebootButton = PrimaryPushButton(FluentIcon.UPDATE, self.tr("重启"))  # 重启按钮
        self.updateConfigButton = PrimaryPushButton(FluentIcon.UPDATE, self.tr("更新配置"))  # 更新配置按钮
        self.deleteConfigButton = ToolButton(FluentIcon.DELETE, self)  # 删除配置按钮
        self.returnButton = TransparentToolButton(FluentIcon.RETURN, self)  # 返回到按钮
        self.addConnectConfigButton = PrimaryPushButton(FluentIcon.ADD, self.tr("添加连接配置"), self)

        # 连接槽函数
        self.runButton.clicked.connect(self.runButtonSlot)
        self.stopButton.clicked.connect(self.stopButtonSlot)
        self.rebootButton.clicked.connect(self.rebootButtonSlot)
        self.updateConfigButton.clicked.connect(self._updateButtonSlot)
        self.deleteConfigButton.clicked.connect(self._deleteButtonSlot)
        self.returnButton.clicked.connect(self._returnButtonSlot)
        self.addConnectConfigButton.clicked.connect(self._onAddConnectConfigButtonClicked)
        botListPageSignalBus.chooseConnectType.connect(self._onShowTypeDialog)

        # 隐藏按钮
        self.stopButton.hide()
        self.rebootButton.hide()

    def _addTooltips(self) -> None:
        """
        ## 为按钮添加悬停提示
        """
        # 返回按钮提示
        self.returnButton.setToolTip(self.tr("返回"))
        self.returnButton.installEventFilter(ToolTipFilter(self.returnButton))

        # 删除按钮提示
        self.deleteConfigButton.setToolTip(self.tr("点击删除配置"))
        self.deleteConfigButton.installEventFilter(ToolTipFilter(self.deleteConfigButton))

        # 更新按钮提示
        self.updateConfigButton.setToolTip(self.tr("点击更新配置"))
        self.updateConfigButton.installEventFilter(ToolTipFilter(self.updateConfigButton))

        # 启动按钮提示
        self.runButton.setToolTip(self.tr("点击启动机器人"))
        self.runButton.installEventFilter(ToolTipFilter(self.runButton))

        # 停止按钮提示
        self.stopButton.setToolTip(self.tr("点击停止机器人"))
        self.stopButton.installEventFilter(ToolTipFilter(self.stopButton))

        # 重启按钮提示
        self.rebootButton.setToolTip(self.tr("点击重启机器人"))
        self.rebootButton.installEventFilter(ToolTipFilter(self.rebootButton))

        # 添加连接配置按钮提示
        self.addConnectConfigButton.setToolTip(self.tr("添加连接配置"))
        self.addConnectConfigButton.installEventFilter(ToolTipFilter(self.addConnectConfigButton))

    @Slot()
    def runButtonSlot(self) -> None:
        """
        ## 启动按钮槽函数
        """
        # 判断是否存在旧实例, 如果不存在则创建新实例, 存在则销毁后创建新实例
        if self.napcatProcess is None:
            self.napcatProcess = create_napcat_process(self.config)
        elif isinstance(self.napcatProcess, QProcess):
            self.napcatProcess.deleteLater()
            self.napcatProcess = create_napcat_process(self.config)

        # NapCat 启动
        self.highlighter = LogHighlighter(self.botLogPage.document())
        self.botLogPage.clear()
        self.napcatProcess.setParent(self)
        self.napcatProcess.setProcessChannelMode(QProcess.ProcessChannelMode.MergedChannels)
        self.napcatProcess.readyReadStandardOutput.connect(self._napcatHandleStdout)
        self.napcatProcess.finished.connect(self._napcatProcessFinishedSlot)
        self.napcatProcess.start()
        self.napcatProcess.waitForStarted()

        # 参数调整
        self.isRun = True
        self.view.setCurrentWidget(self.botLogPage)
        self.runButton.setVisible(False)
        self.stopButton.setVisible(True)
        self.rebootButton.setVisible(True)

        # 显示提示
        info_bar(self.tr("已执行启动命令, 如果长时间没有输出, 请查看日志"))

    @Slot()
    def stopButtonSlot(self) -> None:
        """
        ## 停止按钮槽函数
        """
        if not self.isRun:
            return

        if (parent := psutil.Process(self.napcatProcess.processId())).pid != 0:
            [child.kill() for child in parent.children(recursive=True)]
            parent.kill()
            self.napcatProcess.kill()
            self.napcatProcess.waitForFinished()
            self.napcatProcess.deleteLater()
            self.napcatProcess = None

        self.isRun = False
        self.view.setCurrentWidget(self.botSetupPage)

    @Slot()
    def rebootButtonSlot(self) -> None:
        """
        ## 重启机器人, 直接调用函数
        """
        self.stopButtonSlot()
        self.runButtonSlot()

    def _napcatHandleStdout(self) -> None:
        """
        ## 日志管道并检测内部信息执行操作
        """
        # 获取所有输出
        data = self.napcatProcess.readAllStandardOutput().data().decode()

        # 遍历所有匹配项并移除
        while (matches := QRegularExpression(r"\x1B(?:[@-Z\\-_]|\[[0-?]*[ -/]*[@-~])").globalMatch(data)).hasNext():
            data = data.replace(matches.next().captured(0), "")
            data = data.replace("\n\n", "\n")

        # 获取 CodeEditor 的 cursor 并移动插入输出内容
        cursor = self.botLogPage.textCursor()
        cursor.movePosition(QTextCursor.MoveOperation.End)
        cursor.insertText(data)
        self.botLogPage.setTextCursor(cursor)

        # 分发给其他处理函数
        self._getQRCode(data)
        self._getBofOffline(data)

    @Slot()
    def _napcatProcessFinishedSlot(self, exit_code, exit_status) -> None:
        """
        ## 进程结束槽函数
        """
        cursor = self.botLogPage.textCursor()
        cursor.movePosition(QTextCursor.MoveOperation.End)
        cursor.insertText(f"进程结束，退出码为 {exit_code}，状态为 {exit_status}")
        self.botLogPage.setTextCursor(cursor)

    def _getQRCode(self, data: str) -> None:
        """
        ## 判断是否需要显示二维码
        """
        if "二维码已保存到" in data:
            # 项目内模块导入
            from src.ui.window.main_window import MainWindow

            pattern = rf"[a-zA-Z]:\\(?:[^\\\s]+\\)*[^\\\s]+"
            if not (match := QRegularExpression(pattern).match(data)).hasMatch():
                # 如果匹配不成功, 则退出
                return

            box = ImageBox(self.tr("请扫码登陆"), match.captured(0), MainWindow())
            box.cancelButton.hide()
            box.image_label.scaledToHeight(256)
            box.exec()

    def _getBofOffline(self, data: str):
        """
        ## 检测用户状态
        """
        # 正则匹配
        pattern = rf"\[INFO\] .+\({self.config.bot.QQID}\) \| 账号状态变更为离线"
        if not (match := QRegularExpression(pattern).match(data)).hasMatch():
            # 如果匹配不成功, 则退出
            return

        if not self.config.advanced.offlineNotice:
            # 如果未开启通知, 退出
            return

        if cfg.get(cfg.bot_offline_email_notice):
            # 如果开启了邮件通知, 则发送邮件
            offline_email(self.config)
        elif cfg.get(cfg.bot_offline_web_hook_notice):
            # 如果开启了 WebHook 通知, 则发送 WebHook
            offline_webhook(self.config)
        else:
            # 否则显示提示
            warning_bar(self.tr(f"账号 {self.config.bot.QQID} 已离线, 请前往 NapCat 日志 查看详情"))

    @Slot()
    def _updateButtonSlot(self) -> None:
        """
        ## 更新按钮的槽函数
        """
        self.config = Config(**self.botSetupPage.getValue())
        if update_config(self.config):
            # 更新成功提示
            success_bar(self.tr("更新配置成功"))
        else:
            error_bar(self.tr("更新配置文件时引发错误, 请前往 设置 > log 中查看详细错误"))

    @Slot()
    def _deleteButtonSlot(self) -> None:
        """
        ## 删除机器人配置按钮
        """
        # 项目内模块导入
        from src.ui.window.main_window.window import MainWindow

        if self.isRun:
            # 如果 NC 正在运行, 则提示停止运行后删除配置
            warning_bar(self.tr("NapCat 还在运行, 请停止运行再进行操作"))
            return

        if AskBox(
            self.tr("确认删除"),
            self.tr(f"你确定要删除 {self.config.bot.QQID} 吗? \n\n此操作无法撤消, 请谨慎操作"),
            MainWindow(),
        ).exec():
            # 询问用户是否确认删除, 确认删除执行删除操作
            # 项目内模块导入
            from src.ui.page.bot_list_page.bot_list_widget import BotListWidget

            if delete_config(self.config):
                # 删除成功后的操作
                self.returnButton.clicked.emit()
                BotListWidget().botList.updateList()
                success_bar(self.tr(f"成功删除配置 {self.config.bot.QQID}({self.config.bot.name})"))

                self.returnButton.click()
                self.close()

                # 处理 TabBar
                # 项目内模块导入
                from src.ui.window.main_window.window import MainWindow

                MainWindow().title_bar.tab_bar.removeTabByKey(f"{self.config.bot.QQID}")
            else:
                error_bar(self.tr("删除配置文件时引发错误, 请前往 设置 > log 查看错误原因"))

    @Slot()
    def _returnButtonSlot(self) -> None:
        """
        ## 返回列表按钮的槽函数
        """
        if self.view.currentWidget() in [self.botInfoPage, self.botSetupPage, self.botLogPage]:
            # 判断当前处于哪个页面
            # 项目内模块导入
            from src.ui.page.bot_list_page.bot_list_widget import BotListWidget

            BotListWidget().view.setCurrentIndex(0)
            BotListWidget().topCard.breadcrumbBar.setCurrentIndex(0)
            BotListWidget().topCard.updateListButton.show()
        else:
            self.view.setCurrentWidget(self.botSetupPage)

    @Slot()
    def _onAddConnectConfigButtonClicked(self) -> None:
        """添加连接配置按钮的槽函数"""
        # 项目内模块导入
        from src.ui.window.main_window import MainWindow

        ChooseConfigTypeDialog(MainWindow()).exec()

    @Slot()
    def _onShowTypeDialog(self, connectType: ConnectType) -> None:
        """添加连接配置按钮的槽函数"""
        # 项目内模块导入
        from src.ui.window.main_window import MainWindow

        if (
            dialog := {
                ConnectType.HTTP_SERVER: HttpServerConfigDialog,
                ConnectType.HTTP_SSE_SERVER: HttpSSEServerConfigDialog,
                ConnectType.HTTP_CLIENT: HttpClientConfigDialog,
                ConnectType.WEBSOCKET_SERVER: WebsocketServerConfigDialog,
                ConnectType.WEBSOCKET_CLIENT: WebsocketClientConfigDialog,
            }.get(connectType)(MainWindow())
        ).exec():
            self.botSetupPage.connectWidget.add_card(dialog.get_config())

    @Slot()
    def _pivotSlot(self, index: int) -> None:
        """
        ## pivot 切换槽函数
        """
        widget = self.view.widget(index)
        self.pivot.setCurrentItem(widget.objectName())

        # 定义页面对应的操作字典
        # 塞一大堆 if-else 是及其不专业的行为(
        page_actions = {
            self.botInfoPage.objectName(): {
                "returnButton": "show",
                "updateConfigButton": "hide",
                "deleteConfigButton": "hide",
                "runButton": "hide" if self.isRun else "show",
                "stopButton": "show" if self.isRun else "hide",
                "rebootButton": "show" if self.isRun else "hide",
                "addConnectConfigButton": "hide" if self.isRun else "hide",
            },
            self.botSetupPage.objectName(): {
                "updateConfigButton": "show",
                "deleteConfigButton": "show",
                "returnButton": "show",
                "runButton": "hide" if self.isRun else "show",
                "stopButton": "show" if self.isRun else "hide",
                "rebootButton": "show" if self.isRun else "hide",
                "addConnectConfigButton": "show" if self.isRun else "show",
            },
            self.botLogPage.objectName(): {
                "returnButton": "show",
                "updateConfigButton": "hide",
                "deleteConfigButton": "hide",
                "runButton": "hide" if self.isRun else "show",
                "stopButton": "show" if self.isRun else "hide",
                "rebootButton": "show" if self.isRun else "hide",
                "addConnectConfigButton": "hide" if self.isRun else "hide",
            },
        }

        # 根据 widget.objectName() 执行相应操作
        if widget.objectName() in page_actions:
            for button, action in page_actions[widget.objectName()].items():
                getattr(self, button).setVisible(action == "show")

    def _setLayout(self) -> None:
        """
        ## 对内部进行布局
        """
        self.buttonLayout.setSpacing(8)
        self.buttonLayout.setContentsMargins(0, 0, 0, 0)
        self.buttonLayout.addWidget(self.runButton)
        self.buttonLayout.addWidget(self.stopButton)
        self.buttonLayout.addWidget(self.rebootButton)
        self.buttonLayout.addWidget(self.updateConfigButton)
        self.buttonLayout.addWidget(self.addConnectConfigButton)
        self.buttonLayout.addWidget(self.deleteConfigButton)
        self.buttonLayout.addWidget(self.returnButton)
        self.buttonLayout.setAlignment(Qt.AlignmentFlag.AlignRight)

        self.hBoxLayout.setSpacing(0)
        self.hBoxLayout.setContentsMargins(0, 0, 0, 0)
        self.hBoxLayout.addWidget(self.pivot)
        self.hBoxLayout.addLayout(self.buttonLayout)

        self.vBoxLayout.setSpacing(0)
        self.vBoxLayout.setContentsMargins(0, 0, 0, 0)
        self.vBoxLayout.addLayout(self.hBoxLayout)
        self.vBoxLayout.addSpacing(10)
        self.vBoxLayout.addWidget(self.view)

        self.setLayout(self.vBoxLayout)
