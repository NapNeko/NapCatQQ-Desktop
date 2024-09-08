# -*- coding: utf-8 -*-
from abc import ABC
from typing import TYPE_CHECKING, Self, Optional
from pathlib import Path

from creart import it, add_creator, exists_module
from creart.creator import AbstractCreator, CreateTargetInfo
from PySide6.QtWidgets import QWidget, QVBoxLayout, QStackedWidget

from src.Core import timer
from src.Ui.common import CodeEditor
from src.Ui.StyleSheet import StyleSheet
from src.Ui.common.CodeEditor import NCDLogHighlighter
from src.Ui.SetupPage.SetupTopCard import SetupTopCard
from src.Ui.SetupPage.SetupScrollArea import SetupScrollArea

if TYPE_CHECKING:
    from src.Ui.MainWindow import MainWindow


class SetupWidget(QWidget):
    """
    ## 设置页面
    """

    def __init__(self):
        super().__init__()
        self.log_file_path = Path.cwd() / "log/ALL.log"
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

        # 调整控件
        self.setParent(parent)
        self.setObjectName("SetupPage")
        self.view.setObjectName("SetupView")

        # 调用一次方法以启动计时器
        self.updateLogContent()

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
        self.view.addWidget(self.setupScrollArea)
        self.view.addWidget(self.logWidget)

        self.topCard.pivot.addItem(
            routeKey=self.setupScrollArea.objectName(),
            text=self.tr("设置"),
            onClick=lambda: self.view.setCurrentWidget(self.setupScrollArea),
        )
        self.topCard.pivot.addItem(
            routeKey=self.logWidget.objectName(),
            text=self.tr("日志"),
            onClick=lambda: self.view.setCurrentWidget(self.logWidget),
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

    @timer(1000)
    def updateLogContent(self):
        if not self.log_file_path.exists():
            return

        with open(self.log_file_path, "r", encoding="utf-8") as file:
            # 输出内容
            it(SetupWidget).logWidget.setPlainText(file.read())


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
