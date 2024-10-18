# -*- coding: utf-8 -*-
# 标准库导入
import re

# 第三方库导入
import psutil
from creart import it
from qfluentwidgets import (
    FluentIcon,
    PushButton,
    ToolButton,
    ToolTipFilter,
    SegmentedWidget,
    PrimaryPushButton,
    TransparentToolButton,
)
from PySide6.QtGui import QTextCursor
from PySide6.QtCore import Qt, Slot, QProcess
from PySide6.QtWidgets import QWidget, QHBoxLayout, QVBoxLayout, QStackedWidget

# 项目内模块导入
from src.Ui.common import CodeEditor, LogHighlighter
from src.Ui.StyleSheet import StyleSheet
from src.Ui.common.info_bar import info_bar, error_bar, success_bar, warning_bar
from src.Core.Utils.RunNapCat import create_process
from src.Ui.common.message_box import AskBox, ImageBox
from src.Core.Config.ConfigModel import Config
from src.Core.Config.OperateConfig import delete_config, update_config
from src.Ui.BotListPage.BotWidget.BotSetupPage import BotSetupPage


class BotWidget(QWidget):
    """
    ## 机器人卡片对应的 Widget
    """

    def __init__(self, config: Config) -> None:
        super().__init__()
        self.config = config
        self.isRun = False  # 用于标记机器人是否在运行
        self.isLogin = False  # 用于标记机器人是否登录

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
            text=self.tr("Bot Setup"),
            onClick=lambda: self.view.setCurrentWidget(self.botSetupPage),
        )
        self.pivot.addItem(
            routeKey=self.botLogPage.objectName(),
            text=self.tr("Bot Log"),
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
        self.runButton = PrimaryPushButton(FluentIcon.POWER_BUTTON, self.tr("Startup"))  # 启动按钮
        self.stopButton = PushButton(FluentIcon.POWER_BUTTON, self.tr("Stop"))  # 停止按钮
        self.rebootButton = PrimaryPushButton(FluentIcon.UPDATE, self.tr("Reboot"))  # 重启按钮
        self.showQRCodeButton = TransparentToolButton(FluentIcon.QRCODE, self)  # 显示登录二维码按钮
        self.updateConfigButton = PrimaryPushButton(FluentIcon.UPDATE, self.tr("Update config"))  # 更新配置按钮
        self.deleteConfigButton = ToolButton(FluentIcon.DELETE, self)  # 删除配置按钮
        self.returnButton = TransparentToolButton(FluentIcon.RETURN, self)  # 返回到按钮

        # 连接槽函数
        self.runButton.clicked.connect(self.runButtonSlot)
        self.stopButton.clicked.connect(self.stopButtonSlot)
        self.rebootButton.clicked.connect(self.rebootButtonSlot)
        self.showQRCodeButton.clicked.connect(lambda: self.qrcodeMsgBox.show())
        self.updateConfigButton.clicked.connect(self._updateButtonSlot)
        self.deleteConfigButton.clicked.connect(self._deleteButtonSlot)
        self.returnButton.clicked.connect(self._returnButtonSlot)

        # 隐藏按钮
        self.stopButton.hide()
        self.rebootButton.hide()
        self.updateConfigButton.hide()
        self.deleteConfigButton.hide()
        self.showQRCodeButton.hide()

    def _addTooltips(self) -> None:
        """
        ## 为按钮添加悬停提示
        """
        # 返回按钮提示
        self.returnButton.setToolTip(self.tr("Click Back"))
        self.returnButton.installEventFilter(ToolTipFilter(self.returnButton))

        # 二维码按钮提示
        self.showQRCodeButton.setToolTip(self.tr("Click to show the login QR code"))
        self.showQRCodeButton.installEventFilter(ToolTipFilter(self.showQRCodeButton))

        # 删除按钮提示
        self.deleteConfigButton.setToolTip(self.tr("Click Delete bot configuration"))
        self.deleteConfigButton.installEventFilter(ToolTipFilter(self.deleteConfigButton))

    @Slot()
    def runButtonSlot(self) -> None:
        """
        ## 启动按钮槽函数
        """
        # 获取主框架
        # 项目内模块导入
        from src.Ui.MainWindow import MainWindow

        # 创建组件
        self.process = create_process(self.config)
        self.highlighter = LogHighlighter(self.botLogPage.document())
        self.qrcodeMsgBox = ImageBox(self.tr("QR Code"), "", it(MainWindow))

        # 设置组件
        self.botLogPage.clear()
        self.process.setParent(self)
        self.process.setProcessChannelMode(QProcess.ProcessChannelMode.MergedChannels)
        self.process.readyReadStandardOutput.connect(self._handle_stdout)
        self.process.finished.connect(self._processFinishedSlot)

        # 启动进程
        self.process.start()
        self.process.waitForStarted()
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

        if parent := psutil.Process(self.process.processId()):
            [child.kill() for child in parent.children(recursive=True)]
            parent.kill()

        self.isRun = False
        self.isLogin = False
        self.view.setCurrentWidget(self.botSetupPage)
        self.showQRCodeButton.hide()

    @Slot()
    def rebootButtonSlot(self) -> None:
        """
        ## 重启机器人, 直接调用函数
        """
        self.stopButtonSlot()
        self.runButtonSlot()

    def _handle_stdout(self) -> None:
        """
        ## 日志管道并检测内部信息执行操作
        """
        # 获取所有输出
        data = self.process.readAllStandardOutput().data().decode()

        # 遍历所有匹配项并移除
        while (matches := QRegularExpression(r"\x1B(?:[@-Z\\-_]|\[[0-?]*[ -/]*[@-~])").globalMatch(data)).hasNext():
            data = data.replace(matches.next().captured(0), "")

        # 获取 CodeEditor 的 cursor 并移动插入输出内容
        cursor = self.botLogPage.textCursor()
        cursor.movePosition(QTextCursor.MoveOperation.End)
        cursor.insertText(data)
        self.botLogPage.setTextCursor(cursor)

    @Slot()
    def _processFinishedSlot(self, exit_code, exit_status) -> None:
        """
        ## 进程结束槽函数
        """
        cursor = self.botLogPage.textCursor()
        cursor.movePosition(QTextCursor.MoveOperation.End)
        cursor.insertText(f"进程结束，退出码为 {exit_code}，状态为 {exit_status}")
        self.botLogPage.setTextCursor(cursor)

    @Slot()
    def _updateButtonSlot(self) -> None:
        """
        ## 更新按钮的槽函数
        """
        self.newConfig = Config(**self.botSetupPage.getValue())
        if update_config(self.newConfig):
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
        from src.Ui.MainWindow.Window import MainWindow

        if self.isRun:
            # 如果 NC 正在运行, 则提示停止运行后删除配置
            warning_bar(self.tr("NapCat 还在运行, 请停止运行再进行操作"))
            return

        if AskBox(
                self.tr("确认删除"),
                self.tr(f"你确定要删除 {self.config.bot.QQID} 吗? \n\n此操作无法撤消, 请谨慎操作"),
                it(MainWindow)
        ).exec():
            # 询问用户是否确认删除, 确认删除执行删除操作
            # 项目内模块导入
            from src.Ui.BotListPage.BotListWidget import BotListWidget

            if delete_config(self.config):
                self.returnButton.clicked.emit()
                it(BotListWidget).botList.updateList()
                success_bar(self.tr(f"成功删除配置 {self.config.bot.QQID}({self.config.bot.name})"))
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
            from src.Ui.BotListPage.BotListWidget import BotListWidget

            it(BotListWidget).view.setCurrentIndex(0)
            it(BotListWidget).topCard.breadcrumbBar.setCurrentIndex(0)
            it(BotListWidget).topCard.updateListButton.show()
        else:
            self.view.setCurrentWidget(self.botSetupPage)

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
                "showQRCodeButton": "hide",
                "runButton": "hide" if self.isRun else "show",
                "stopButton": "show" if self.isRun else "hide",
                "rebootButton": "show" if self.isRun else "hide",
            },
            self.botSetupPage.objectName(): {
                "updateConfigButton": "show",
                "deleteConfigButton": "show",
                "showQRCodeButton": "hide",
                "returnButton": "show",
                "runButton": "hide",
                "stopButton": "hide",
                "rebootButton": "hide",
            },
            self.botLogPage.objectName(): {
                "returnButton": "show",
                "updateConfigButton": "hide",
                "deleteConfigButton": "hide",
                "runButton": "hide" if self.isRun else "show",
                "stopButton": "show" if self.isRun else "hide",
                "rebootButton": "show" if self.isRun else "hide",
                "showQRCodeButton": "show" if not self.isLogin and self.isRun else "hide",
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
        self.buttonLayout.addWidget(self.showQRCodeButton)
        self.buttonLayout.addWidget(self.updateConfigButton)
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
