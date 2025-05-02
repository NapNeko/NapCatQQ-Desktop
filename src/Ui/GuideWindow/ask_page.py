# -*- coding: utf-8 -*-
# 第三方库导入
from qfluentwidgets import CardWidget, ImageLabel, SubtitleLabel, StrongBodyLabel
from qframelesswindow import FramelessWindow
from PySide6.QtCore import Qt
from PySide6.QtWidgets import QWidget, QHBoxLayout, QVBoxLayout


class AskPage(QWidget):
    """询问用户是新手还是老手页面"""

    def __init__(self, parent: FramelessWindow) -> None:
        super().__init__(parent)

        # 创建控件
        self.newbieCard = NewbieCard(self)
        self.veteranCard = VeteranCard(self)
        self.titleLabel = SubtitleLabel(self.tr("你是新手还是老手？"), self)

        # 设置属性

        # 布局
        self.cardLayout = QHBoxLayout()
        self.cardLayout.setContentsMargins(0, 0, 0, 0)
        self.cardLayout.setSpacing(0)
        self.cardLayout.addStretch(1)
        self.cardLayout.addWidget(self.newbieCard)
        self.cardLayout.addStretch(1)
        self.cardLayout.addWidget(self.veteranCard)
        self.cardLayout.addStretch(1)

        self.vBoxLayout = QVBoxLayout(self)
        self.vBoxLayout.setContentsMargins(0, 0, 0, 0)
        self.vBoxLayout.setSpacing(0)
        self.vBoxLayout.addStretch(1)
        self.vBoxLayout.addWidget(self.titleLabel, 0, Qt.AlignmentFlag.AlignCenter)
        self.vBoxLayout.addSpacing(30)
        self.vBoxLayout.addLayout(self.cardLayout, 0)
        self.vBoxLayout.addStretch(1)


class NewbieCard(CardWidget):
    """新手卡片"""

    def __init__(self, parent: AskPage):
        super().__init__(parent)

        # 创建控件
        self.imageLabel = ImageLabel(":/FuFuFace/image/FuFuFace/g_fufu_11.gif", self)
        self.titleLabel = StrongBodyLabel(self.tr("我是新手"), self)

        # 设置属性
        self.setFixedSize(200, 240)
        self.imageLabel.scaledToHeight(128)

        # 布局
        self.vBoxLayout = QVBoxLayout(self)
        self.vBoxLayout.setContentsMargins(0, 0, 0, 0)
        self.vBoxLayout.setSpacing(0)
        self.vBoxLayout.addStretch(1)
        self.vBoxLayout.addWidget(self.imageLabel, 0, Qt.AlignmentFlag.AlignCenter)
        self.vBoxLayout.addSpacing(10)
        self.vBoxLayout.addWidget(self.titleLabel, 0, Qt.AlignmentFlag.AlignCenter)
        self.vBoxLayout.addStretch(1)

        # 信号连接
        self.clicked.connect(self.parent().parent().on_next_page)


class VeteranCard(CardWidget):
    """老手卡片"""

    def __init__(self, parent: AskPage):
        super().__init__(parent)

        # 创建控件
        self.imageLabel = ImageLabel(":/FuFuFace/image/FuFuFace/g_fufu_12.gif", self)
        self.titleLabel = StrongBodyLabel(self.tr("我是老手"), self)

        # 设置属性
        self.setFixedSize(200, 240)
        self.imageLabel.scaledToHeight(128)

        # 布局
        self.vBoxLayout = QVBoxLayout(self)
        self.vBoxLayout.setContentsMargins(0, 0, 0, 0)
        self.vBoxLayout.setSpacing(0)
        self.vBoxLayout.addStretch(1)
        self.vBoxLayout.addWidget(self.imageLabel, 0, Qt.AlignmentFlag.AlignCenter)
        self.vBoxLayout.addSpacing(10)
        self.vBoxLayout.addWidget(self.titleLabel, 0, Qt.AlignmentFlag.AlignCenter)
        self.vBoxLayout.addStretch(1)

        # 信号连接
        self.clicked.connect(self.on_click)

    def on_click(self) -> None:
        """点击事件"""
        # 项目内模块导入
        from src.Ui.GuideWindow.guide_window import GuideWindow

        GuideWindow().close()
