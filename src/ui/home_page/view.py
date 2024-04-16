# -*- coding: utf-8 -*-
from abc import ABC
from loguru import logger
from typing import TYPE_CHECKING

from PySide6.QtCore import Qt, QRectF
from PySide6.QtGui import QPixmap, QPainter, QColor, QBrush, QPainterPath, QFont
from PySide6.QtWidgets import QWidget, QVBoxLayout, QLabel, QSizePolicy
from qfluentwidgets.components import ScrollArea, ImageLabel, TitleLabel
from qfluentwidgets.common import setFont
from creart import add_creator, exists_module, create
from creart.creator import AbstractCreator, CreateTargetInfo

from src.ui.StyleSheet import StyleSheet


class ViewWidget(QWidget):

    def __init__(self) -> None:
        """
        初始化
        """
        super().__init__()
        self.setObjectName("view")

        # 创建控件
        self.v_box_layout = QVBoxLayout(self)
        self.logo_image = ImageLabel(self)
        self.logo_label = TitleLabel("NapCat-Desktop", self)

        # 设置控件
        self.logo_image.setImage(":Global/logo.png")
        self.logo_image.scaledToWidth(self.width() // 5)

        # 进行布局
        self.set_layout()

        # 应用样式表
        StyleSheet.HOME_WIDGET.apply(self)

    def set_layout(self) -> None:
        """
        对 ViewWidget 内控件进行布局
        """
        self.v_box_layout.addStretch(1)
        self.v_box_layout.addWidget(
            self.logo_image, alignment=Qt.AlignmentFlag.AlignCenter
        )
        self.v_box_layout.addSpacing(20)
        self.v_box_layout.setStretch(2, 0.5)
        self.v_box_layout.addWidget(
            self.logo_label, alignment=Qt.AlignmentFlag.AlignCenter
        )
        self.v_box_layout.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.v_box_layout.addStretch(1)

        self.setLayout(self.v_box_layout)

    def resizeEvent(self, event):
        """
        重写实现自动缩放
        """
        super().resizeEvent(event)
        # 缩放 Logo
        self.logo_image.scaledToWidth(self.width() // 5)
        # 缩放字体
        new_font_size = max(28, self.width() // 30)
        setFont(self.logo_label, new_font_size, QFont.Weight.DemiBold)
        # 重绘页面
        self.update()
