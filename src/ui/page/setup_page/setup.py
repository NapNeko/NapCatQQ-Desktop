# -*- coding: utf-8 -*-
# 标准库导入
from typing import TYPE_CHECKING, Self

from PySide6.QtWidgets import QWidget, QVBoxLayout

# 项目内模块导入
from src.core.config import cfg
from src.ui.StyleSheet import StyleSheet
from src.core.utils.singleton import singleton
from src.ui.SetupPage.General import General
from src.ui.common.code_editor import CodeExibit
from src.ui.common.stacked_widget import TransparentStackedWidget
from src.ui.SetupPage.SetupTopCard import SetupTopCard
from src.ui.SetupPage.Personalization import Personalization

if TYPE_CHECKING:
    # 项目内模块导入
    from src.ui.MainWindow import MainWindow


@singleton
class SetupWidget(QWidget):
    """设置页面"""

    topCard: SetupTopCard
    vBoxLayout: QVBoxLayout
    infoWidget: CodeExibit

    view: TransparentStackedWidget
    personalization: Personalization

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
        # 创建控件
        self.view = TransparentStackedWidget(self)
        self.general = General(self)
        self.personalization = Personalization(self)

        # 设置控件
        self.view.addWidget(self.personalization)
        self.view.addWidget(self.general)

        self.topCard.pivot.addItem(
            routeKey=self.personalization.objectName(),
            text=self.tr("个性化"),
            onClick=lambda: self.view.setCurrentWidget(self.personalization),
        )
        self.topCard.pivot.addItem(
            routeKey=self.general.objectName(),
            text=self.tr("常规"),
            onClick=lambda: self.view.setCurrentWidget(self.general),
        )

        # 连接信号并初始化当前标签页
        self.view.setCurrentWidget(self.personalization)
        self.topCard.pivot.setCurrentItem(self.personalization.objectName())
