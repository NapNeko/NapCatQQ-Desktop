# -*- coding: utf-8 -*-
# 标准库导入
from typing import TYPE_CHECKING, Self, Optional

from PySide6.QtWidgets import QWidget, QVBoxLayout

# 项目内模块导入
from src.Ui.common import CodeEditor
from src.Core.Config import cfg
from src.Ui.StyleSheet import StyleSheet
from src.Core.Utils.singleton import singleton
from src.Ui.common.stacked_widget import TransparentStackedWidget
from src.Ui.SetupPage.SetupTopCard import SetupTopCard
from src.Ui.SetupPage.SetupScrollArea import SetupScrollArea

if TYPE_CHECKING:
    # 项目内模块导入
    from src.Ui.MainWindow import MainWindow


@singleton
class SetupWidget(QWidget):
    """设置页面"""

    view: Optional[TransparentStackedWidget]
    topCard: Optional[SetupTopCard]
    setupScrollArea: Optional[SetupScrollArea]
    vBoxLayout: Optional[QVBoxLayout]
    infoWidget: Optional[CodeEditor]

    def __init__(self):
        super().__init__()

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
        self.view.addWidget(self.setupScrollArea)

        # 连接信号并初始化当前标签页
        self.view.setCurrentWidget(self.setupScrollArea)
