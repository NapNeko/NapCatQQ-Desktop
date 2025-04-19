# -*- coding: utf-8 -*-
"""
NCD 组件页面 - 框架组件
"""

# 第三方库导入
from qfluentwidgets import FluentIcon as FIcon
from qfluentwidgets import ScrollArea, ExpandLayout
from qfluentwidgets.components.settings import SettingCardGroup, OptionsSettingCard
from PySide6.QtCore import Qt
from PySide6.QtWidgets import QWidget

# 项目内模块导入
from src.core.config import cfg
from src.ui.style_sheet import StyleSheet


class Framework(ScrollArea):
    """框架组件页面"""

    def __init__(self, parent=None) -> None:
        """构造函数"""
        super().__init__(parent)

        # 调用方法
        self.createComponent()
        self.setupLayout()
        self.setComponent()
        self._connectSignalToSlot()

        self.setObjectName("unit_pivot_famework")

        # 调整样式表
        StyleSheet.TRANSPARENT_SCROLL_AREA.apply(self)

    def createComponent(self) -> None:
        """创建组件"""
        self.viewWidget = QWidget()
        self.expandLayout = ExpandLayout(self.viewWidget)

    def setComponent(self) -> None:
        """设置组件"""
        self.setObjectName("settings_pivot_famework")
        self.setWidget(self.viewWidget)
        self.setWidgetResizable(True)
        self.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)

    def setupLayout(self) -> None:
        """设置布局"""
        self.expandLayout.setSpacing(24)
        self.expandLayout.setContentsMargins(0, 0, 0, 0)

    def _connectSignalToSlot(self):
        """连接信号与槽"""
        ...


__all__ = ["Framework"]
