# -*- coding: utf-8 -*-
# 标准库导入
from typing import TYPE_CHECKING

# 第三方库导入
from qfluentwidgets import CustomColorSettingCard, ExpandLayout
from qfluentwidgets import FluentIcon
from qfluentwidgets import FluentIcon as FI
from qfluentwidgets import (
    MessageBoxBase,
    OptionsSettingCard,
    RangeSettingCard,
    ScrollArea,
    SettingCardGroup,
    TitleLabel,
    setTheme,
    setThemeColor,
)
from PySide6.QtCore import QObject, Qt
from PySide6.QtWidgets import QGridLayout, QWidget

# 项目内模块导入
from src.core.config import cfg
from src.ui.components.info_bar import success_bar
from src.ui.components.input_card.generic_card import (
    ComboBoxConfigCard,
    ShowDialogCardBase,
    SwitchConfigCard,
    FontFamilyConfigCatd,
)
from creart import it

if TYPE_CHECKING:
    # 项目内模块导入
    from src.ui.window.main_window import MainWindow


class Personalization(ScrollArea):

    def __init__(self, parent) -> None:
        super().__init__(parent=parent)
        # 创建控件
        self.view = QWidget()
        self.expand_layout = ExpandLayout(self.view)

        # 设置 ScrollArea 和控件
        self.setWidget(self.view)
        self.setWidgetResizable(True)
        self.view.setObjectName("SetupView")
        self.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        self.setObjectName("SetupPersonalizationWidget")

        # 调用方法
        self._createConfigCards()
        self._connect_signal()
        self._set_layout()

    def _createConfigCards(self) -> None:
        """创建配置项卡片"""
        # 创建组 - 主题
        self.theme_group = SettingCardGroup(title=self.tr("主题"), parent=self.view)
        # 创建项
        self.theme_card = OptionsSettingCard(
            configItem=cfg.theme_mode,
            icon=FluentIcon.BRUSH,
            title=self.tr("切换主题"),
            content=self.tr("切换程序的主题"),
            texts=[self.tr("明亮模式"), self.tr("极夜模式"), self.tr("跟随系统")],
            parent=self.theme_group,
        )
        self.theme_color_card = CustomColorSettingCard(
            configItem=cfg.theme_color,
            icon=FluentIcon.PALETTE,
            title=self.tr("主题颜色"),
            content=self.tr("选择主题色"),
            parent=self.theme_group,
        )
        self.font_family_card = FontFamilyConfigCatd(
            configItem=cfg.fontFamilies,
            icon=FluentIcon.FONT,
            title=self.tr("字体"),
            content=self.tr("设置应用程序的字体"),
            parent=self.theme_group,
        )
        # 创建组 - 窗体
        self.window_group = SettingCardGroup(title=self.tr("窗体"), parent=self.view)
        # 创建项
        self.window_opacity_card = RangeSettingCard(
            configItem=cfg.window_opacity,
            icon=FluentIcon.FIT_PAGE,
            title=self.tr("窗口透明度"),
            content=self.tr("设置窗口的透明度"),
            parent=self.window_group,
        )
        self.zoom_card = OptionsSettingCard(
            configItem=cfg.dpi_scale,
            icon=FluentIcon.ZOOM,
            title=self.tr("界面缩放"),
            content=self.tr("更改 Widget 和字体的大小"),
            texts=["100%", "125%", "150%", "175%", "200%", self.tr("跟随系统")],
            parent=self.window_group,
        )

    def _set_layout(self) -> None:
        """控件布局"""
        # 将卡片添加到组

        self.theme_group.addSettingCard(self.theme_card)
        self.theme_group.addSettingCard(self.theme_color_card)
        self.theme_group.addSettingCard(self.font_family_card)
        # self.personalGroup.addSettingCard(self.languageCard)

        self.window_group.addSettingCard(self.zoom_card)
        self.window_group.addSettingCard(self.window_opacity_card)
        # 添加到布局
        self.expand_layout.addWidget(self.theme_group)
        self.expand_layout.addWidget(self.window_group)
        self.expand_layout.setContentsMargins(0, 0, 0, 0)
        self.view.setLayout(self.expand_layout)

    def _connect_signal(self) -> None:
        """信号处理"""
        # 项目内模块导入
        from src.ui.window.main_window.window import MainWindow

        # 连接重启提示
        cfg.app_restart_signal.connect(lambda: success_bar(self.tr("配置在重启后生效")))

        # 连接启动相关

        # 连接个性化相关
        self.theme_card.optionChanged.connect(self._on_theme_mode_changed)
        self.theme_color_card.colorChanged.connect(lambda color: setThemeColor(color, save=True, lazy=True))
        self.window_opacity_card.valueChanged.connect(lambda value: it(MainWindow).setWindowOpacity(value / 100))

    @staticmethod
    def _on_theme_mode_changed(theme) -> None:
        """主题切换"""
        setTheme(cfg.get(theme), save=True)
