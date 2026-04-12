# -*- coding: utf-8 -*-
# 第三方库导入
from qfluentwidgets import CaptionLabel, FluentIcon, PrimaryPushButton, SegmentedWidget, TitleLabel
from PySide6.QtCore import Qt
from PySide6.QtWidgets import QHBoxLayout, QVBoxLayout, QWidget


class TopWidget(QWidget):
    """下载页面顶部控件，包含标题、副标题、标签切换和刷新按钮"""

    def __init__(self, parent: QWidget) -> None:
        """初始化顶部控件

        Args:
            parent: 父控件
        """
        super().__init__(parent=parent)

        # 创建控件
        self.title_label = TitleLabel(self.tr("组件"), self)
        self.pivot = SegmentedWidget()
        self.update_button = PrimaryPushButton(FluentIcon.UPDATE, self.tr("刷新"))

        # 创建布局
        self.h_box_layout = QHBoxLayout()
        self.label_layout = QVBoxLayout()
        self.button_layout = QHBoxLayout()

        # 设置布局
        self._set_layout()

    def _set_layout(self) -> None:
        """设置控件布局"""
        # 设置标签区域布局
        self.label_layout.setSpacing(0)
        self.label_layout.setContentsMargins(0, 0, 0, 0)
        self.label_layout.addWidget(self.title_label)
        self.label_layout.addSpacing(4)
        self.label_layout.addWidget(self.pivot)

        # 设置按钮区域布局
        self.button_layout.addSpacing(4)
        self.button_layout.addWidget(self.update_button)
        self.button_layout.setAlignment(Qt.AlignmentFlag.AlignBottom | Qt.AlignmentFlag.AlignLeft)

        # 设置主布局
        self.h_box_layout.addLayout(self.label_layout, 1)
        self.h_box_layout.addStretch(2)
        self.h_box_layout.addLayout(self.button_layout, 0)
        self.button_layout.setAlignment(Qt.AlignmentFlag.AlignBottom | Qt.AlignmentFlag.AlignLeft)
        self.h_box_layout.setContentsMargins(1, 0, 1, 5)

        self.setLayout(self.h_box_layout)
