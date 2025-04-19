# -*- coding: utf-8 -*-
"""
NCD 组件页面 - 通用组件
"""

# 第三方库导入
from qfluentwidgets import FluentIcon as FIcon
from qfluentwidgets import ScrollArea, ExpandLayout
from qfluentwidgets.components.widgets.stacked_widget import FadeEffectAniStackedWidget
from PySide6.QtCore import Qt
from PySide6.QtWidgets import QWidget, QVBoxLayout

# 项目内模块导入
from src.core.config import cfg
from src.ui.style_sheet import StyleSheet

# from src.ui.unit_page.general.card import CardPage, CardWidgetBase


class General(QWidget):
    """通用组件页面"""

    def __init__(self, parent=None) -> None:
        """构造函数"""
        super().__init__(parent)

        # 调用方法
        self.createComponent()
        self.setComponent()
        self._connectSignalToSlot()

        self.setObjectName("unit_pivot_general")

        # 调整样式表
        StyleSheet.TRANSPARENT_SCROLL_AREA.apply(self)

    def createComponent(self) -> None:
        """创建组件"""
        # self.cardPage = CardPage(self)
        ...

    def setComponent(self) -> None:
        """设置组件"""
        self.setObjectName("settings_pivot_general")

    def _connectSignalToSlot(self):
        """连接信号与槽"""
        ...


__all__ = ["General"]
