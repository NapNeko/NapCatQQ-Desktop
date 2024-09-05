# -*- coding: utf-8 -*-
from PySide6.QtWidgets import QHBoxLayout, QVBoxLayout, QWidget
from qfluentwidgets import CaptionLabel, SegmentedWidget, TitleLabel


class SetupTopCard(QWidget):
    """
    ## SetupTopCard 顶部展示的 InputCard
    """

    def __init__(self, parent) -> None:
        super().__init__(parent=parent)

        # 创建所需控件
        self.pivot = SegmentedWidget()
        self.titleLabel = TitleLabel(self.tr("Settings"), self)
        self.subtitleLabel = CaptionLabel(self.tr("NapCat-Desktop Settings"), self)
        self.hBoxLayout = QHBoxLayout()
        self.vBoxLayout = QVBoxLayout()
        self.labelLayout = QVBoxLayout()

        # 设置布局
        self.labelLayout.setSpacing(0)
        self.labelLayout.setContentsMargins(0, 0, 0, 0)
        self.labelLayout.addWidget(self.titleLabel)
        self.labelLayout.addSpacing(5)
        self.labelLayout.addWidget(self.subtitleLabel)

        self.hBoxLayout.addWidget(self.pivot, 1)
        self.hBoxLayout.addStretch(3)
        self.hBoxLayout.setContentsMargins(0, 0, 0, 0)

        self.vBoxLayout.addLayout(self.labelLayout)
        self.vBoxLayout.addLayout(self.hBoxLayout)
        self.vBoxLayout.setContentsMargins(0, 0, 0, 0)

        self.setLayout(self.vBoxLayout)
