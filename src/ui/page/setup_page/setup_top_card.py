# -*- coding: utf-8 -*-
# 第三方库导入
from qfluentwidgets import CaptionLabel, SegmentedWidget, TitleLabel
from PySide6.QtWidgets import QHBoxLayout, QVBoxLayout, QWidget


class SetupTopCard(QWidget):
    """SetupTopCard 顶部展示的 InputCard"""

    def __init__(self, parent) -> None:
        super().__init__(parent=parent)

        # 创建所需控件
        self.pivot = SegmentedWidget()
        self.title_label = TitleLabel(self.tr("设置"), self)
        self.subtitle_label = CaptionLabel(self.tr("NapCatQQ Desktop 设置"), self)
        self.h_box_layout = QHBoxLayout()
        self.v_box_layout = QVBoxLayout()
        self.label_layout = QVBoxLayout()

        # 设置布局
        self.label_layout.setSpacing(0)
        self.label_layout.setContentsMargins(0, 0, 0, 0)
        self.label_layout.addWidget(self.title_label)
        self.label_layout.addSpacing(5)
        self.label_layout.addWidget(self.subtitle_label)

        self.h_box_layout.addWidget(self.pivot, 1)
        self.h_box_layout.addStretch(3)
        self.h_box_layout.setContentsMargins(0, 0, 0, 0)

        self.v_box_layout.addLayout(self.label_layout)
        self.v_box_layout.addLayout(self.h_box_layout)
        self.v_box_layout.setContentsMargins(0, 0, 0, 0)

        self.setLayout(self.v_box_layout)
