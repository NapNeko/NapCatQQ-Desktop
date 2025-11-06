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
        self.title_tab_bar_setting_card = ShowDialogCardBase(
            dialog=TitleTabBarSettingDialog,
            icon=FluentIcon.TILES,
            title=self.tr("标题选项卡"),
            content=self.tr("标题选项卡相关设置"),
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
        self.window_group.addSettingCard(self.title_tab_bar_setting_card)
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


class TitleTabBarSettingDialog(MessageBoxBase):

    def __init__(self, parent: QObject) -> None:
        super().__init__(parent=parent)

        # 创建控件
        self.title_label = TitleLabel(self.tr("标题选项卡配置"))
        self.enable_card = SwitchConfigCard(FI.IOT, self.tr("启用标题选项卡"))
        self.enable_movable_card = SwitchConfigCard(FI.MOVE, self.tr("启用拖动标签页"))
        self.enable_scrollable_card = SwitchConfigCard(FI.SCROLL, self.tr("启用标签页范围可滚动"))
        self.enable_tab_shadow_card = SwitchConfigCard(FI.FIT_PAGE, self.tr("启用标签页阴影"))
        self.set_close_mode_card = ComboBoxConfigCard(
            FI.CLOSE,
            self.tr("关闭按钮显示方式"),
            [self.tr("始终显示"), self.tr("悬停显示"), self.tr("永不显示")],
        )
        self.set_tab_maximum_width_card = RangeSettingCard(
            cfg.title_tab_bar_max_width, FI.TRANSPARENT, self.tr("标签最大宽度")
        )
        self.set_tab_minimum_width_card = RangeSettingCard(
            cfg.title_tab_bar_min_width, FI.TRANSPARENT, self.tr("标签最小宽度")
        )

        # 布局
        self.grid_layout = QGridLayout()
        self.grid_layout.addWidget(self.enable_card, 0, 0, 1, 2)
        self.grid_layout.addWidget(self.enable_movable_card, 0, 2, 1, 2)
        self.grid_layout.addWidget(self.enable_scrollable_card, 1, 0, 1, 2)
        self.grid_layout.addWidget(self.enable_tab_shadow_card, 1, 2, 1, 2)
        self.grid_layout.addWidget(self.set_close_mode_card, 2, 0, 1, 4)
        self.grid_layout.addWidget(self.set_tab_maximum_width_card, 3, 0, 1, 4)
        self.grid_layout.addWidget(self.set_tab_minimum_width_card, 4, 0, 1, 4)
        self.grid_layout.setContentsMargins(0, 0, 0, 0)
        self.grid_layout.setSpacing(8)

        # 设置布局
        self.viewLayout.addWidget(self.title_label)
        self.viewLayout.addLayout(self.grid_layout)
        self.widget.setMinimumSize(500, 450)

        # 填充配置
        self.enable_card.fill_value(cfg.get(cfg.title_tab_bar))
        self.enable_movable_card.fill_value(cfg.get(cfg.title_tab_bar_movable))
        self.enable_scrollable_card.fill_value(cfg.get(cfg.title_tab_bar_scrollable))
        self.enable_tab_shadow_card.fill_value(cfg.get(cfg.title_tab_bar_shadow))
        self.set_close_mode_card.fill_value(cfg.get(cfg.title_tab_bar_close_mode))

    def accept(self) -> None:
        """接受按钮"""
        cfg.set(cfg.title_tab_bar, self.enable_card.get_value())
        cfg.set(cfg.title_tab_bar_movable, self.enable_movable_card.get_value())
        cfg.set(cfg.title_tab_bar_scrollable, self.enable_scrollable_card.get_value())
        cfg.set(cfg.title_tab_bar_shadow, self.enable_tab_shadow_card.get_value())
        cfg.set(cfg.title_tab_bar_close_mode, self.set_close_mode_card.get_value())
        super().accept()
