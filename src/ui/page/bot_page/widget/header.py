# -*- coding: utf-8 -*-

# 第三方库导入
from qfluentwidgets import BreadcrumbBar, setFont
from PySide6.QtGui import QFont
from PySide6.QtWidgets import QWidget


class HeaderWidget(QWidget):
    """页面顶部的信息标头"""

    def __init__(self, parent: QWidget | None = None) -> None:
        """构造函数

        Args:
            parent (QWidget | None, optional): 父控件. Defaults to None.
        """
        super().__init__(parent)

        # 创建控件
        self.breadcrumb_bar = BreadcrumbBar(self)

        # 设置控件
        setFont(self.breadcrumb_bar, 28, QFont.Weight.Bold)
        self.breadcrumb_bar.addItem("title", self.tr("Bot"))
        self.breadcrumb_bar.setSpacing(16)
