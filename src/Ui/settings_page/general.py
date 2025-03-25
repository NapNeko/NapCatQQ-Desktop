# -*- coding: utf-8 -*-
"""
NCD 设置页面 - 个性化
"""

# 第三方库导入
from qfluentwidgets import FluentIcon as FIcon
from qfluentwidgets import ScrollArea, ExpandLayout
from qfluentwidgets.common import setTheme, setThemeColor
from qfluentwidgets.components.settings import (
    SettingCardGroup,
    SwitchSettingCard,
    OptionsSettingCard,
    CustomColorSettingCard,
)
from PySide6.QtCore import Qt
from PySide6.QtWidgets import QWidget

# 项目内模块导入
from src.core.config import cfg
from src.ui.style_sheet import StyleSheet
from src.ui.common.info_bar import success_bar


class General(ScrollArea):
    """通用设置页面"""

    def __init__(self, parent=None) -> None:
        """构造函数"""
        super().__init__(parent)

        # 调用方法
        self.createComponent()
        self.setupLayout()
        self.setComponent()
        self._connectSignalToSlot()

        self.setObjectName("settings_pivot_general")

        # 调整样式表
        StyleSheet.TRANSPARENT_SCROLL_AREA.apply(self)

    def createComponent(self) -> None:
        """创建组件"""
        self.viewWidget = QWidget()
        self.expandLayout = ExpandLayout(self.viewWidget)

        # 行为配置项
        self.ActionGroup = SettingCardGroup(self.tr("样式"), self.viewWidget)
        self.closeBtnCard = OptionsSettingCard(
            configItem=cfg.closeBtnAction,
            icon=FIcon.CLOSE,
            title=self.tr("关闭按钮"),
            content=self.tr("选择点击关闭按钮时的行为"),
            texts=[self.tr("关闭程序"), self.tr("最小化隐藏到托盘")],
            parent=self.ActionGroup,
        )

    def setComponent(self) -> None:
        """设置组件"""
        self.setObjectName("settings_pivot_general")
        self.setWidget(self.viewWidget)
        self.setWidgetResizable(True)
        self.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)

    def setupLayout(self) -> None:
        """设置布局"""

        self.ActionGroup.addSettingCard(self.closeBtnCard)

        self.expandLayout.setSpacing(24)
        self.expandLayout.setContentsMargins(0, 0, 0, 0)
        self.expandLayout.addWidget(self.ActionGroup)

    def _connectSignalToSlot(self):
        """连接信号与槽"""
        ...


__all__ = ["General"]
