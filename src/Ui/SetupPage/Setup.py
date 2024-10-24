# -*- coding: utf-8 -*-
# 标准库导入
from abc import ABC
from typing import TYPE_CHECKING, Self, Optional

# 第三方库导入
from creart import it, add_creator, exists_module
from creart.creator import AbstractCreator, CreateTargetInfo
from PySide6.QtWidgets import QWidget, QVBoxLayout, QStackedWidget

# 项目内模块导入
from src.Core import timer
from src.Ui.common import CodeEditor
from src.Core.Config import cfg
from src.Ui.StyleSheet import StyleSheet
from src.Ui.common.widget import BackgroundWidget
from src.Core.Utils.logger import INFO_LOG, DEBUG_LOG
from src.Ui.common.CodeEditor import NCDLogHighlighter
from src.Ui.common.stacked_widget import TransparentStackedWidget
from src.Ui.SetupPage.SetupTopCard import SetupTopCard
from src.Ui.SetupPage.SetupScrollArea import SetupScrollArea

if TYPE_CHECKING:
    # 项目内模块导入
    from src.Ui.MainWindow import MainWindow


class SetupWidget(BackgroundWidget):
    """
    ## 设置页面
    """

    def __init__(self):
        super().__init__()
        # 预定义控件及属性
        self.info_log_file_path = INFO_LOG
        self.debug_log_file_path = DEBUG_LOG
        self.view: Optional[TransparentStackedWidget] = None
        self.topCard: Optional[SetupTopCard] = None
        self.setupScrollArea: Optional[SetupScrollArea] = None
        self.vBoxLayout: Optional[QVBoxLayout] = None
        self.infoWidget: Optional[CodeEditor] = None

        # 传入配置
        self.bgEnabledConfig = cfg.bgSettingPage
        self.bgPixmapLightConfig = cfg.bgSettingPageLight
        self.bgPixmapDarkConfig = cfg.bgSettingPageDark
        self.bgOpacityConfig = cfg.bgSettingPageOpacity

        # 调用方法
        super().updateBgImage()

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
        self.updateInfoLogContent()
        self.updateDebugLogContent()

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
        self.view = TransparentStackedWidget(self)
        self.setupScrollArea = SetupScrollArea(self)
        self.infoWidget = CodeEditor(self)
        self.infoWidget.setObjectName("NCD-InfoLogWidget")
        self.debugWidget = CodeEditor(self)
        self.debugWidget.setObjectName("NCD-DebugLogWidget")
        self.infoHighlighter = NCDLogHighlighter(self.infoWidget.document())
        self.debugHighlighter = NCDLogHighlighter(self.debugWidget.document())
        self.view.addWidget(self.setupScrollArea)
        self.view.addWidget(self.infoWidget)
        self.view.addWidget(self.debugWidget)

        self.topCard.pivot.addItem(
            routeKey=self.setupScrollArea.objectName(),
            text=self.tr("设置"),
            onClick=lambda: self.view.setCurrentWidget(self.setupScrollArea),
        )
        self.topCard.pivot.addItem(
            routeKey=self.infoWidget.objectName(),
            text=self.tr("INFO 日志"),
            onClick=lambda: self.view.setCurrentWidget(self.infoWidget),
        )
        self.topCard.pivot.addItem(
            routeKey=self.debugWidget.objectName(),
            text=self.tr("DEBUG 日志"),
            onClick=lambda: self.view.setCurrentWidget(self.debugWidget),
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
    def updateInfoLogContent(self):
        if not self.info_log_file_path.exists():
            return

        with open(self.info_log_file_path, "r", encoding="utf-8") as file:
            # 输出内容
            it(SetupWidget).infoWidget.setPlainText(file.read())

    @timer(1000)
    def updateDebugLogContent(self):
        if not self.debug_log_file_path.exists():
            return

        with open(self.debug_log_file_path, "r", encoding="utf-8") as file:
            # 输出内容
            it(SetupWidget).debugWidget.setPlainText(file.read())


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
