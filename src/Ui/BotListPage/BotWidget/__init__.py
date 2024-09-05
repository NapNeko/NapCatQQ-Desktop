# -*- coding: utf-8 -*-
import re
import json

import psutil
from creart import it
from loguru import logger
from PySide6.QtGui import QPixmap, QTextCursor
from PySide6.QtCore import Qt, Slot, QProcess
from qfluentwidgets import (
    BodyLabel,
    FluentIcon,
    ImageLabel,
    PushButton,
    ToolButton,
    SubtitleLabel,
    ToolTipFilter,
    MessageBoxBase,
    SegmentedWidget,
    PrimaryPushButton,
    TransparentToolButton,
)
from PySide6.QtWidgets import QWidget, QHBoxLayout, QVBoxLayout, QStackedWidget

from src.Ui.common import CodeEditor, LogHighlighter
from src.Core.PathFunc import PathFunc
from src.Ui.StyleSheet import StyleSheet
from src.Core.Config.ConfigModel import Config
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
            routeKey=self.botLogPage.objectName(),
            text=self.tr("Bot Log"),
            onClick=lambda: self.view.setCurrentWidget(self.botLogPage),
        )
        # self.pivot.addItem(
        #     routeKey=self.botInfoPage.objectName(),
        #     text=self.tr("Bot info"),
        #     onClick=lambda: self.view.setCurrentWidget(self.botInfoPage)
        # )
        self.pivot.addItem(
            routeKey=self.botSetupPage.objectName(),
            text=self.tr("Bot Setup"),
            onClick=lambda: self.view.setCurrentWidget(self.botSetupPage),
        )
        self.pivot.setCurrentItem(self.botLogPage.objectName())
        self.pivot.setMaximumWidth(300)

    def _createView(self) -> None:
        """
        ## 创建用于切换页面的 view
        """
        # 创建 view 和 页面
        self.view = QStackedWidget()
        # self.botInfoPage = QWidget(self)
        # self.botInfoPage.setObjectName(f"{self.config.bot.QQID}_BotWidgetPivot_BotInfo")

        self.botSetupPage = BotSetupPage(self.config, self)

        self.botLogPage = CodeEditor(self)
        self.botLogPage.setObjectName(f"{self.config.bot.QQID}_BotWidgetPivot_BotLog")

        # 将页面添加到 view
        # self.view.addWidget(self.botInfoPage)
        self.view.addWidget(self.botSetupPage)
        self.view.addWidget(self.botLogPage)
        self.view.setObjectName("BotView")
        self.view.setCurrentWidget(self.botLogPage)
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
        self.returnListButton = TransparentToolButton(FluentIcon.RETURN, self)  # 返回到列表按钮
        self.botSetupSubPageReturnButton = TransparentToolButton(FluentIcon.RETURN, self)  # 返回到 BotSetup 按钮

        # 连接槽函数
        self.runButton.clicked.connect(self._runButtonSlot)
        self.stopButton.clicked.connect(self._stopButtonSlot)
        self.rebootButton.clicked.connect(self._rebootButtonSlot)
        self.showQRCodeButton.clicked.connect(lambda: self.qrcodeMsgBox.show())
        self.updateConfigButton.clicked.connect(self._updateButtonSlot)
        self.deleteConfigButton.clicked.connect(self._deleteButtonSlot)
        self.botSetupSubPageReturnButton.clicked.connect(self._botSetupSubPageReturnButtonSlot)
        self.returnListButton.clicked.connect(self._returnListButtonSlot)

        # 隐藏按钮
        self.stopButton.hide()
        self.rebootButton.hide()
        self.updateConfigButton.hide()
        self.deleteConfigButton.hide()
        self.showQRCodeButton.hide()
        self.botSetupSubPageReturnButton.hide()

    def _addTooltips(self) -> None:
        """
        ## 为按钮添加悬停提示
        """
        self.returnListButton.setToolTip(self.tr("Click Back to list"))
        self.returnListButton.installEventFilter(ToolTipFilter(self.returnListButton))

        self.botSetupSubPageReturnButton.setToolTip(self.tr("Click Back to BotSetup"))
        self.botSetupSubPageReturnButton.installEventFilter(ToolTipFilter(self.botSetupSubPageReturnButton))

        self.showQRCodeButton.setToolTip(self.tr("Click to show the login QR code"))
        self.showQRCodeButton.installEventFilter(ToolTipFilter(self.showQRCodeButton))

        self.deleteConfigButton.setToolTip(self.tr("Click Delete bot configuration"))
        self.deleteConfigButton.installEventFilter(ToolTipFilter(self.deleteConfigButton))

    @Slot()
    def _runButtonSlot(self):
        """
        ## 启动按钮槽函数

        # 清除已有(如果有)的log
        # 配置环境变量
            - 使用 QProcess.systemEnvironment() 复制一份系统环境变量列表
            - 使用 append 添加 NapCat 所需的 ELECTRON_RUN_AS_NODE=1
        # 创建 QProcess
            - 设置先前配置好的 环境变量
            - 设置启动的程序(QQ)路径
            - 将 NapCat 以参数新式启动
        # 配置 QProcess
            - 设置输出合并
            - 连接日志输出管道
            - 连接结束函数
            - 设置日志高亮
            - 创建登录二维码消息盒
        # 启动 QProcess
            - 进度条设置完成
            - 切换到 botLogPage
        # 切换按钮显示
        """
        from src.Ui.BotListPage import BotListWidget

        self.botLogPage.clear()

        self.env = QProcess.systemEnvironment()
        self.env.append(f"NAPCAT_PATCH_PATH={it(PathFunc).getNapCatPath() / 'patchNapCat.js'}")
        self.env.append(f"NAPCAT_LOAD_PATH={it(PathFunc).getNapCatPath() / 'loadNapCat.js'}")
        self.env.append(f"NAPCAT_INJECT_PATH={it(PathFunc).getNapCatPath() / 'NapCatWinBootHook.dll'}")
        self.env.append(f"NAPCAT_LAUNCHER_PATH={it(PathFunc).getNapCatPath() / 'NapCatWinBootMain.exe'}")
        self.env.append(f"NAPCAT_MAIN_PATH={it(PathFunc).getNapCatPath() / 'napcat.mjs'}")

        self.process = QProcess(self)
        self.process.setEnvironment(self.env)
        self.process.setProgram(str(it(PathFunc).getNapCatPath() / "NapCatWinBootMain.exe"))
        self.process.setArguments(
            [str(it(PathFunc).getQQPath() / "QQ.exe"), str(it(PathFunc).getNapCatPath() / "NapCatWinBootHook.dll")]
        )
        self.process.setProcessChannelMode(QProcess.ProcessChannelMode.MergedChannels)
        self.process.readyReadStandardOutput.connect(self._handle_stdout)
        self.process.readyReadStandardOutput.connect(self._showQRCode)
        self.process.finished.connect(self._processFinishedSlot)
        self.highlighter = LogHighlighter(self.botLogPage.document())
        self.qrcodeMsgBox = QRCodeMessageBox(self.parent().parent())
        self.process.start()
        self.process.waitForStarted()

        it(BotListWidget).showInfo(
            title=self.tr("The run command has been executed"),
            content=self.tr("If there is no output for a long time, check the QQ path and NapCat path"),
        )
        self.isRun = True
        self.view.setCurrentWidget(self.botLogPage)
        self.runButton.setVisible(False)
        self.stopButton.setVisible(True)
        self.rebootButton.setVisible(True)

    @Slot()
    def _stopButtonSlot(self):
        """
        ## 停止按钮槽函数
        """
        parent = psutil.Process(self.process.processId())
        [child.kill() for child in parent.children(recursive=True)]
        parent.kill()

        self.isRun = False
        self.isLogin = False
        self.view.setCurrentWidget(self.botLogPage)
        self.showQRCodeButton.hide()

    @Slot()
    def _rebootButtonSlot(self):
        """
        ## 重启机器人, 直接调用函数吧
        """
        self._stopButtonSlot()
        self._runButtonSlot()

    def _handle_stdout(self):
        """
        ## 日志管道并检测内部信息执行操作
        """
        # 获取所有输出
        data = self.process.readAllStandardOutput().data().decode()
        # 匹配并移除 ANSI 转义码
        ansi_escape = re.compile(r"\x1B(?:[@-Z\\-_]|\[[0-?]*[ -/]*[@-~])")
        data = ansi_escape.sub("", data)
        # 获取 CodeEditor 的 cursor 并移动插入输出内容
        cursor = self.botLogPage.textCursor()
        cursor.movePosition(QTextCursor.MoveOperation.End)
        cursor.insertText(data)
        self.botLogPage.setTextCursor(cursor)

        # 执行一些操作
        self._showQRCode(data)

    def _showQRCode(self, data=""):
        """
        ## 显示二维码
        """
        if self.isLogin:
            # 如果是已经登录成功的状态,则直接跳过
            return

        from src.Ui.BotListPage import BotListWidget

        if "[ERROR] () | 快速登录错误" in data:
            # 引发此错误时自动重启
            self._rebootButtonSlot()
            it(BotListWidget).showInfo(
                title=self.tr("Sign-in error"),
                content=self.tr(
                    "Quick login error, NapCat has been automatically restarted, "
                    "the following is the error message\n"
                    "Quick login error"
                ),
            )
            return

        if match := re.search(r"二维码已保存到\s(.+)", data):
            # 如果已经显示了则关闭
            self.qrcodeMsgBox.cancelButton.click()
            # 提取匹配的路径
            qrcode_path = match.group(1).strip()
            self.qrcodeMsgBox.setQRCode(qrcode_path)
            self.showQRCodeButton.show()
            self.showQRCodeButton.click()
            return

        if f"[INFO] ({self.config.bot.QQID}) | 登录成功! " in data:
            # 如果登录成功
            self.qrcodeMsgBox.cancelButton.click()
            self.showQRCodeButton.hide()
            self.isLogin = True
            it(BotListWidget).showSuccess(
                title=self.tr("Login successful!"), content=self.tr(f"Account {self.config.bot.QQID} login successful!")
            )

    @Slot()
    def _processFinishedSlot(self, exit_code, exit_status):
        cursor = self.botLogPage.textCursor()
        cursor.movePosition(QTextCursor.MoveOperation.End)
        cursor.insertText(f"进程结束，退出码为 {exit_code}，状态为 {exit_status}")
        self.botLogPage.setTextCursor(cursor)

    @Slot()
    def _updateButtonSlot(self) -> None:
        """
        ## 更新按钮的槽函数
        """
        from src.Ui.BotListPage import BotListWidget
        from src.Core.Config.ConfigModel import DEFAULT_CONFIG

        self.newConfig = Config(**self.botSetupPage.getValue())

        # 读取配置列表
        with open(str(it(PathFunc).bot_config_path), "r", encoding="utf-8") as f:
            bot_configs = [it(BotListWidget).botList.updateConfig(config, DEFAULT_CONFIG) for config in json.load(f)]
            bot_configs = [Config(**config) for config in bot_configs]

        if not bot_configs:
            # 为空报错
            logger.error(f"机器人列表加载失败 数据为空: {bot_configs}")
            it(BotListWidget).showError(title=self.tr("Update error"), content=self.tr("Data loss within the profile"))
            return

        # 如果从文件加载的 bot_config 不为空则进行更新(为空怎么办呢,我能怎么办,崩了呗bushi
        for index, config in enumerate(bot_configs):
            # 遍历配置列表,找到一样则替换
            if config.bot.QQID == self.newConfig.bot.QQID:
                bot_configs[index] = self.newConfig
                break
        # 不可以直接使用 dict方法 转为 dict对象, 内部 WebsocketUrl 和 HttpUrl 不会自动转为 str
        bot_configs = [json.loads(config.json()) for config in bot_configs]
        with open(str(it(PathFunc).bot_config_path), "w", encoding="utf-8") as f:
            json.dump(bot_configs, f, indent=4)
        # 更新成功提示
        it(BotListWidget).showSuccess(
            title=self.tr("Update success"), content=self.tr("The updated configuration is successful")
        )

    @Slot()
    def _deleteButtonSlot(self) -> None:
        """
        ## 删除机器人配置按钮
        """
        from src.Ui.BotListPage import BotListWidget
        DeleteConfigMessageBox(self.isRun, self, it(BotListWidget)).exec()

    @staticmethod
    @Slot()
    def _returnListButtonSlot() -> None:
        """
        ## 返回列表按钮的槽函数
        """
        from src.Ui.BotListPage.BotListWidget import BotListWidget

        it(BotListWidget).view.setCurrentIndex(0)
        it(BotListWidget).topCard.breadcrumbBar.setCurrentIndex(0)
        it(BotListWidget).topCard.updateListButton.show()

    @Slot()
    def _botSetupSubPageReturnButtonSlot(self) -> None:
        """
        ## BotSetup 页面中子页面返回按钮的槽函数
        """
        self.botSetupSubPageReturnButton.hide()
        self.returnListButton.show()
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
            # self.botInfoPage.objectName(): {
            #     'returnListButton': 'show',
            #     'updateConfigButton': 'hide',
            #     'deleteConfigButton': 'hide',
            #     'botSetupSubPageReturnButton': 'hide',
            #     'showQRCodeButton': 'hide',
            #     'runButton': 'hide' if self.isRun else 'show',
            #     'stopButton': 'show' if self.isRun else 'hide',
            #     'rebootButton': 'show' if self.isRun else 'hide'
            # },
            self.botSetupPage.objectName(): {
                "updateConfigButton": "show",
                "deleteConfigButton": "show",
                "botSetupSubPageReturnButton": "hide",
                "showQRCodeButton": "hide",
                "returnListButton": "show",
                "runButton": "hide",
                "stopButton": "hide",
                "rebootButton": "hide",
            },
            self.botLogPage.objectName(): {
                "returnListButton": "show",
                "updateConfigButton": "hide",
                "deleteConfigButton": "hide",
                "botSetupSubPageReturnButton": "hide",
                "runButton": "hide" if self.isRun else "show",
                "stopButton": "show" if self.isRun else "hide",
                "rebootButton": "show" if self.isRun else "hide",
                "showQRCodeButton": "show" if not self.isLogin and self.isRun else "hide",
            },
        }

        # 根据 widget.objectName() 执行相应操作
        if widget.objectName() in page_actions:
            actions = page_actions[widget.objectName()]
            for button, action in actions.items():
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
        self.buttonLayout.addWidget(self.returnListButton)
        self.buttonLayout.addWidget(self.botSetupSubPageReturnButton)
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


class QRCodeMessageBox(MessageBoxBase):
    """
    ## 用于展示登录用的 QRCode
    """

    def __init__(self, parent=None) -> None:
        super().__init__(parent=parent)
        self.titleLabel = SubtitleLabel(self.tr("Scan the QR code to log in"), self)
        self.qrcodeLabel = ImageLabel(self)
        self.qrcodePath = None

        # 设置图片宽高
        self.qrcodeLabel.setFixedSize(100, 100)

        # 将组件添加到布局中
        self.viewLayout.addWidget(self.titleLabel)
        self.viewLayout.addWidget(self.qrcodeLabel, alignment=Qt.AlignmentFlag.AlignHCenter)

        # 设置对话框
        self.widget.setMinimumWidth(350)
        self.yesButton.setText(self.tr("Refresh the QR code"))
        self.yesButton.clicked.disconnect()
        self.yesButton.clicked.connect(lambda: self.setQRCode(self.qrcodePath) if self.qrcodePath else None)

    def setQRCode(self, qrcode_path: str) -> None:
        qr_code = QPixmap(qrcode_path)
        self.qrcodeLabel.setImage(qr_code)
        self.qrcodePath = qrcode_path


class DeleteConfigMessageBox(MessageBoxBase):
    """
    ## 删除机器人配置对话框
    """

    def __init__(self, is_run: bool, parent: BotWidget, page) -> None:
        super().__init__(parent=page)
        self.titleLabel = SubtitleLabel()
        self.contentLabel = BodyLabel()

        if is_run:
            # 如果正在运行, 则提示停止运行
            self.titleLabel.setText(self.tr("Deletion failed"))
            self.contentLabel.setText(
                self.tr("NapCat is currently running, please stop running and delete the configuration")
            )
            self.yesButton.setText(self.tr("Stop NapCat operation"))
            self.yesButton.clicked.connect(lambda: parent.stopButton.click())
        else:
            # 不在运行则确认删除
            self.titleLabel.setText(self.tr("Confirm the deletion"))
            self.contentLabel.setText(
                self.tr(
                    f"Are you sure you want to delete {parent.config.bot.QQID}? \n\n"
                    f"This operation cannot be undone, please proceed with caution"
                )
            )
            self.yesButton.setText(self.tr("Confirm the deletion"))
            self.yesButton.clicked.connect(lambda: self._deleteConfigSlot(parent))

        # 将组件添加到布局中
        self.viewLayout.addWidget(self.titleLabel)
        self.viewLayout.addWidget(self.contentLabel)

        # 设置对话框
        self.widget.setMinimumWidth(350)

    @staticmethod
    @Slot()
    def _deleteConfigSlot(parent: BotWidget) -> None:
        """
        ## 执行删除配置
            - 返回到列表, 删除配置并保存, 刷新列表
        """
        from src.Ui.BotListPage import BotListWidget

        it(BotListWidget).botList.botList.remove(parent.config)

        bot_configs = [json.loads(config.json()) for config in it(BotListWidget).botList.botList]

        with open(str(it(PathFunc).bot_config_path), "w", encoding="utf-8") as f:
            json.dump(bot_configs, f, indent=4)

        parent.returnListButton.click()
        it(BotListWidget).botList.updateList()
