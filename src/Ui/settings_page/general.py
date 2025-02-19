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
    ExpandGroupSettingCard,
)
from PySide6.QtCore import Qt
from PySide6.QtWidgets import QWidget

# 项目内模块导入
from src.core.config import cfg
from src.ui.style_sheet import StyleSheet
from src.ui.common.info_bar import success_bar
from src.ui.common.signal_bus import settingsSignalBus


class General(QWidget):
    """个性化设置页面"""

    def __init__(self, parent=None) -> None:
        """构造函数"""
        super().__init__(parent)

        # 调用方法
        # self.createComponent()
        # self.setupLayout()
        # self.setComponent()
        # self._connectSignalToSlot()

        self.setObjectName("settings_pivot_general")

        # 调整样式表
        StyleSheet.TRANSPARENT_SCROLL_AREA.apply(self)
