# -*- coding: utf-8 -*-
from PySide6.QtCore import Qt
from PySide6.QtWidgets import QWidget, QVBoxLayout
from qfluentwidgets import ScrollArea

from src.Ui.HomePage.UpdateView.UpdateTopCard import UpdateTopCard
from src.Ui.StyleSheet import StyleSheet
from src.Ui.common.Netwrok import NapCatUpdateCard


class UpdateViewWidget(QWidget):

    def __init__(self) -> None:
        """
        初始化
        """
        super().__init__()
        # 设置控件
        self.setObjectName("update_view")

        # 创建布局和控件
        self.vBoxLayout = QVBoxLayout(self)
        self.topCard = UpdateTopCard(self)
        self.cardView = CardView(self)
        self.ncUpdateCard = NapCatUpdateCard(self)

        # 调用方法
        self.cardView.addWidget(self.ncUpdateCard)
        self.cardView.viewLayout.addStretch(1)
        self._setLayout()

    def _setLayout(self) -> None:
        """
        ## 对内部进行布局
        """
        self.vBoxLayout.addWidget(self.topCard)
        self.vBoxLayout.addWidget(self.cardView)
        self.vBoxLayout.setContentsMargins(24, 20, 24, 10)
        self.setLayout(self.vBoxLayout)


class CardView(ScrollArea):
    """
    ## 更新卡片视图
    """

    def __init__(self, parent):
        super().__init__(parent=parent)

        # 创建控件和布局
        self.view = QWidget(self)
        self.viewLayout = QVBoxLayout(self.view)

        # 设置控件
        self.view.setObjectName("UpdateViewWidget_CardView")
        self.setWidget(self.view)
        self.setWidgetResizable(True)

        # 设置布局
        self.viewLayout.setContentsMargins(0, 0, 10, 0)

        # 应用样式表
        StyleSheet.HOME_WIDGET.apply(self)

    def addWidget(self, widget):
        """
        ## 添加控件到 viewLayout
        """
        self.viewLayout.addWidget(widget, 0, Qt.AlignmentFlag.AlignTop)
