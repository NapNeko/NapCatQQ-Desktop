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
from src.ui.components.input_card.generic_card import ComboBoxConfigCard, ShowDialogCard, SwitchConfigCard

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
        self._setLayout()

    def _createConfigCards(self) -> None:
        """
        创建配置项卡片
        """
        # 创建组 - 主题
        self.themeGroup = SettingCardGroup(title=self.tr("主题"), parent=self.view)
        # 创建项
        self.themeCard = OptionsSettingCard(
            configItem=cfg.theme_mode,
            icon=FluentIcon.BRUSH,
            title=self.tr("切换主题"),
            content=self.tr("切换程序的主题"),
            texts=[self.tr("明亮模式"), self.tr("极夜模式"), self.tr("跟随系统")],
            parent=self.themeGroup,
        )
        self.themeColorCard = CustomColorSettingCard(
            configItem=cfg.theme_color,
            icon=FluentIcon.PALETTE,
            title=self.tr("主题颜色"),
            content=self.tr("选择主题色"),
            parent=self.themeGroup,
        )
        # 创建组 - 窗体
        self.windowGroup = SettingCardGroup(title=self.tr("窗体"), parent=self.view)
        # 创建项
        self.windowOpacityCard = RangeSettingCard(
            configItem=cfg.window_opacity,
            icon=FluentIcon.FIT_PAGE,
            title=self.tr("窗口透明度"),
            content=self.tr("设置窗口的透明度"),
            parent=self.windowGroup,
        )
        self.zoomCard = OptionsSettingCard(
            configItem=cfg.dpi_scale,
            icon=FluentIcon.ZOOM,
            title=self.tr("界面缩放"),
            content=self.tr("更改 Widget 和字体的大小"),
            texts=["100%", "125%", "150%", "175%", "200%", self.tr("跟随系统")],
            parent=self.windowGroup,
        )
        self.titleTabBarSettingCard = ShowDialogCard(
            dialog=TitleTabBarSettingDialog,
            icon=FluentIcon.TILES,
            title=self.tr("标题选项卡"),
            content=self.tr("标题选项卡相关设置"),
            parent=self.windowGroup,
        )

    def _setLayout(self) -> None:
        """
        控件布局
        """
        # 将卡片添加到组

        self.themeGroup.addSettingCard(self.themeCard)
        self.themeGroup.addSettingCard(self.themeColorCard)
        # self.personalGroup.addSettingCard(self.languageCard)

        self.windowGroup.addSettingCard(self.zoomCard)
        self.windowGroup.addSettingCard(self.windowOpacityCard)
        self.windowGroup.addSettingCard(self.titleTabBarSettingCard)
        # 添加到布局
        self.expand_layout.addWidget(self.themeGroup)
        self.expand_layout.addWidget(self.windowGroup)
        self.expand_layout.setContentsMargins(0, 0, 0, 0)
        self.view.setLayout(self.expand_layout)

    def _connect_signal(self) -> None:
        """
        信号处理
        """
        # 项目内模块导入
        from src.ui.window.main_window.window import MainWindow

        # 连接重启提示
        cfg.app_restart_signal.connect(lambda: success_bar(self.tr("配置在重启后生效")))

        # 连接启动相关

        # 连接个性化相关
        self.themeCard.optionChanged.connect(self._themeModeChanged)
        self.themeColorCard.colorChanged.connect(lambda color: setThemeColor(color, save=True, lazy=True))
        self.windowOpacityCard.valueChanged.connect(lambda value: MainWindow().setWindowOpacity(value / 100))

    @staticmethod
    def _themeModeChanged(theme) -> None:
        """
        主题切换槽函数
        """
        # 项目内模块导入
        from src.ui.page.home_page import HomeWidget

        setTheme(cfg.get(theme), save=True)


class TitleTabBarSettingDialog(MessageBoxBase):

    def __init__(self, parent: QObject) -> None:
        super().__init__(parent=parent)

        # 创建控件
        self.titleLabel = TitleLabel(self.tr("标题选项卡配置"))
        self.enableCard = SwitchConfigCard(FI.IOT, self.tr("启用标题选项卡"))
        self.enableMovableCard = SwitchConfigCard(FI.MOVE, self.tr("启用拖动标签页"))
        self.enableScrollableCard = SwitchConfigCard(FI.SCROLL, self.tr("启用标签页范围可滚动"))
        self.enableTabShadowCard = SwitchConfigCard(FI.FIT_PAGE, self.tr("启用标签页阴影"))
        self.setCloseModeCard = ComboBoxConfigCard(
            FI.CLOSE,
            self.tr("关闭按钮显示方式"),
            [self.tr("始终显示"), self.tr("悬停显示"), self.tr("永不显示")],
        )
        self.setTabMaximumWidthCard = RangeSettingCard(
            cfg.title_tab_bar_max_width, FI.TRANSPARENT, self.tr("标签最大宽度")
        )
        self.setTabMinimumWidthCard = RangeSettingCard(
            cfg.title_tab_bar_min_width, FI.TRANSPARENT, self.tr("标签最小宽度")
        )

        # 布局
        self.gridLayout = QGridLayout()
        self.gridLayout.addWidget(self.enableCard, 0, 0, 1, 2)
        self.gridLayout.addWidget(self.enableMovableCard, 0, 2, 1, 2)
        self.gridLayout.addWidget(self.enableScrollableCard, 1, 0, 1, 2)
        self.gridLayout.addWidget(self.enableTabShadowCard, 1, 2, 1, 2)
        self.gridLayout.addWidget(self.setCloseModeCard, 2, 0, 1, 4)
        self.gridLayout.addWidget(self.setTabMaximumWidthCard, 3, 0, 1, 4)
        self.gridLayout.addWidget(self.setTabMinimumWidthCard, 4, 0, 1, 4)
        self.gridLayout.setContentsMargins(0, 0, 0, 0)
        self.gridLayout.setSpacing(8)

        # 设置布局
        self.viewLayout.addWidget(self.titleLabel)
        self.viewLayout.addLayout(self.gridLayout)
        self.widget.setMinimumSize(500, 450)

        # 填充配置
        self.enableCard.fillValue(cfg.get(cfg.title_tab_bar))
        self.enableMovableCard.fillValue(cfg.get(cfg.title_tab_bar_movable))
        self.enableScrollableCard.fillValue(cfg.get(cfg.title_tab_bar_scrollable))
        self.enableTabShadowCard.fillValue(cfg.get(cfg.title_tab_bar_shadow))
        self.setCloseModeCard.fillValue(cfg.get(cfg.title_tab_bar_close_mode).value)

    def accept(self) -> None:
        """接受按钮"""
        cfg.set(cfg.title_tab_bar, self.enableCard.getValue())
        cfg.set(cfg.title_tab_bar_movable, self.enableMovableCard.getValue())
        cfg.set(cfg.title_tab_bar_scrollable, self.enableScrollableCard.getValue())
        cfg.set(cfg.title_tab_bar_shadow, self.enableTabShadowCard.getValue())
        cfg.set(cfg.title_tab_bar_close_mode, self.setCloseModeCard.getValue())
        super().accept()
