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
from src.ui.common.signal_bus import settingsSignalBus


class Personalized(ScrollArea):
    """个性化设置页面"""

    def __init__(self, parent=None) -> None:
        """构造函数"""
        super().__init__(parent)

        # 调用方法
        self.createComponent()
        self.setupLayout()
        self.setComponent()
        self._connectSignalToSlot()

        # 调整样式表
        StyleSheet.TRANSPARENT_SCROLL_AREA.apply(self)

    def createComponent(self) -> None:
        """创建组件"""
        self.viewWidget = QWidget()
        self.expandLayout = ExpandLayout(self.viewWidget)

        # 外观配置项
        self.styleGroup = SettingCardGroup(self.tr("样式"), self.viewWidget)
        self.themeCard = OptionsSettingCard(
            configItem=cfg.themeMode,
            icon=FIcon.BRIGHTNESS,
            title=self.tr("NapCatQQ Desktop 主题"),
            content=self.tr("调整为浅色或深色主题"),
            texts=[self.tr("浅色"), self.tr("深色"), self.tr("自动")],
            parent=self.styleGroup,
        )
        self.themeColorCard = CustomColorSettingCard(
            configItem=cfg.themeColor,
            icon=FIcon.PALETTE,
            title=self.tr("NapCatQQ Desktop 主题颜色"),
            content=self.tr("调整主题颜色"),
            parent=self.styleGroup,
        )
        self.zoomCard = OptionsSettingCard(
            configItem=cfg.dpiScale,
            icon=FIcon.ZOOM,
            title=self.tr("界面缩放"),
            content=self.tr("更改 Widget 和字体的大小"),
            texts=["100%", "125%", "150%", "175%", "200%", self.tr("跟随系统")],
            parent=self.styleGroup,
        )

        # 标题栏配置项
        self.titleBarGroup = SettingCardGroup(self.tr("标题栏"), self.viewWidget)
        self.commandCenterCard = SwitchSettingCard(
            configItem=cfg.commandCenter,
            icon=FIcon.COMMAND_PROMPT,
            title=self.tr("命令中心"),
            content=self.tr("启用或禁用命令中心"),
            parent=self.titleBarGroup,
        )

    def setComponent(self) -> None:
        """设置组件"""
        self.setObjectName("settings_pivot_personalized")
        self.setWidget(self.viewWidget)
        self.setWidgetResizable(True)
        self.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)

    def setupLayout(self) -> None:
        """设置布局"""
        self.styleGroup.addSettingCard(self.themeCard)
        self.styleGroup.addSettingCard(self.themeColorCard)
        self.styleGroup.addSettingCard(self.zoomCard)

        self.titleBarGroup.addSettingCard(self.commandCenterCard)

        self.expandLayout.setSpacing(24)
        self.expandLayout.setContentsMargins(0, 0, 0, 0)
        self.expandLayout.addWidget(self.styleGroup)
        self.expandLayout.addWidget(self.titleBarGroup)

    def _connectSignalToSlot(self):
        """连接信号与槽"""

        # 个性化
        self.themeCard.optionChanged.connect(setTheme)
        self.themeColorCard.colorChanged.connect(setThemeColor)

        # 标题栏
        self.commandCenterCard.checkedChanged.connect(settingsSignalBus.commandCenterSignal)


__all__ = ["Personalized"]
