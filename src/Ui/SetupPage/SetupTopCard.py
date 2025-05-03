# -*- coding: utf-8 -*-
# 第三方库导入
from qfluentwidgets import TitleLabel, CaptionLabel, SegmentedWidget
from PySide6.QtWidgets import QWidget, QHBoxLayout, QVBoxLayout


class SetupTopCard(QWidget):
    """
    ## SetupTopCard 顶部展示的 InputCard
    """

    def __init__(self, parent) -> None:
        super().__init__(parent=parent)

        # 创建所需控件
        self.pivot = SegmentedWidget()
        self.titleLabel = TitleLabel(self.tr("设置"), self)
        self.subtitleLabel = CaptionLabel(self.tr("NapCatQQ Desktop 设置"), self)
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
