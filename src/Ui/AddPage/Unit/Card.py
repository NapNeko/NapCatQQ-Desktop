# -*- coding: utf-8 -*-
# 第三方库导入
from qfluentwidgets import BodyLabel, CardWidget, ImageLabel, FluentIconBase
from PySide6.QtGui import QPixmap
from PySide6.QtCore import Qt, QSize
from PySide6.QtWidgets import QVBoxLayout


class BuilderAppCard(CardWidget):
    """
    ## BuilderAppCard
        - 用于展示 Builder 页面用户选择的 模式
    """

    def __init__(self, icon: FluentIconBase, text: str) -> None:
        super().__init__()

        # 创建组件
        self.icon = ImageLabel(icon.icon().pixmap(QSize(64, 64)), self)
        self.label = BodyLabel(text, self)
        self.vBoxLayout = QVBoxLayout(self)

        # 调整布局
        self.vBoxLayout.addStretch(2)
        self.vBoxLayout.addWidget(self.icon, alignment=Qt.AlignmentFlag.AlignHCenter)
        self.vBoxLayout.addWidget(self.label, alignment=Qt.AlignmentFlag.AlignHCenter)
        self.vBoxLayout.addStretch(2)
        self.vBoxLayout.setSpacing(30)

    def paintEvent(self, event):
        super().paintEvent(event)
        self.icon.scaledToHeight(self.height() // 4)

