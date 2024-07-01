# -*- coding: utf-8 -*-
import re
import time
from abc import ABC
from pathlib import Path
from typing import TYPE_CHECKING, Self, Optional

from PySide6.QtCore import QThread
from PySide6.QtWidgets import QWidget, QStackedWidget, QVBoxLayout
from creart import add_creator, exists_module, it
from creart.creator import AbstractCreator, CreateTargetInfo

from src.Ui.SetupPage.SetupScrollArea import SetupScrollArea
from src.Ui.SetupPage.SetupTopCard import SetupTopCard
from src.Ui.StyleSheet import StyleSheet
from src.Ui.common import CodeEditor
from src.Ui.common.CodeEditor import NCDLogHighlighter

if TYPE_CHECKING:
    from src.Ui.MainWindow import MainWindow


class SetupWidget(QWidget):
    """
    ## 设置页面
    """

    def __init__(self):
        super().__init__()
        self.view: Optional[QStackedWidget] = None
        self.topCard: Optional[SetupTopCard] = None
        self.setupScrollArea: Optional[SetupScrollArea] = None
        self.vBoxLayout: Optional[QVBoxLayout] = None
        self.logWidget: Optional[CodeEditor] = None

    def initialize(self, parent: "MainWindow") -> Self:
        """
        ## 初始化
        """
        # 创建控件
        self.topCard = SetupTopCard(self)
        self.vBoxLayout = QVBoxLayout()
        self._createView()

        # 跳转控件
        self.setParent(parent)
        self.setObjectName("SetupPage")
        self.view.setObjectName("SetupView")

        # 设置布局
        self.vBoxLayout.addWidget(self.topCard)
        self.vBoxLayout.addWidget(self.view)
        self.vBoxLayout.setContentsMargins(24, 20, 24, 10)
        self.setLayout(self.vBoxLayout)

        # 应用样式表
        StyleSheet.SETUP_WIDGET.apply(self)
        return self

    def _createView(self) -> None:
        """
        ## 创建并配置 QStackedWidget
        """
        self.view = QStackedWidget(self)
        self.setupScrollArea = SetupScrollArea(self)
        self.logWidget = CodeEditor(self)
        self.logWidget.setObjectName("NCD-LogWidget")
        self.highlighter = NCDLogHighlighter(self.logWidget.document())
        self.updateLogWorker = UpdateLogWorker(self)
        self.updateLogWorker.start()
        self.view.addWidget(self.setupScrollArea)
        self.view.addWidget(self.logWidget)

        self.topCard.pivot.addItem(
            routeKey=self.setupScrollArea.objectName(),
            text=self.tr("Settings"),
            onClick=lambda: self.view.setCurrentWidget(self.setupScrollArea),
        )
        self.topCard.pivot.addItem(
            routeKey=self.logWidget.objectName(),
            text=self.tr("Log"),
            onClick=lambda: self.view.setCurrentWidget(self.logWidget)
        )

        # 连接信号并初始化当前标签页
        self.view.currentChanged.connect(self.onCurrentIndexChanged)
        self.view.setCurrentWidget(self.setupScrollArea)
        self.topCard.pivot.setCurrentItem(self.setupScrollArea.objectName())

    def onCurrentIndexChanged(self, index) -> None:
        """
        ## 切换 Pivot 和 view 的槽函数
        """
        widget = self.view.widget(index)
        self.topCard.pivot.setCurrentItem(widget.objectName())


class UpdateLogWorker(QThread):

    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)

    def run(self):
        while True:
            # 暴力循环算了
            self.updateLogContent()
            time.sleep(1)

    @staticmethod
    def updateLogContent():
        log_file_path = Path.cwd() / "log/ALL.log"

        if not log_file_path.exists():
            return

        with open(log_file_path, "r", encoding="utf-8") as file:
            # 匹配并移除 ANSI 转义码
            ansi_escape = re.compile(r'\x1B(?:[@-Z\\-_]|\[[0-?]*[ -/]*[@-~])')
            content = ansi_escape.sub('', file.read())
            # 替换特定字符串
            content = content.replace(
                "\n📢 Tips: QFluentWidgets Pro is now released. Click "
                "https://qfluentwidgets.com/pages/pro to learn more about it.\n\n",
                ""
            )
            # 输出内容
            it(SetupWidget).logWidget.setPlainText(content)


class SetupWidgetClassCreator(AbstractCreator, ABC):
    # 定义类方法targets，该方法返回一个元组，元组中包含了一个CreateTargetInfo对象，
    # 该对象描述了创建目标的相关信息，包括应用程序名称和类名。
    targets = (CreateTargetInfo("src.Ui.SetupPage.Setup", "SetupWidget"),)

    # 静态方法available()，用于检查模块"Setup"是否存在，返回值为布尔型。
    @staticmethod
    def available() -> bool:
        return exists_module("src.Ui.SetupPage.Setup")

    # 静态方法create()，用于创建SetupWidget类的实例，返回值为SetupWidget对象。
    @staticmethod
    def create(create_type: [SetupWidget]) -> SetupWidget:
        return SetupWidget()


add_creator(SetupWidgetClassCreator)
