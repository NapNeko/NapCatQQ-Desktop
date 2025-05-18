# -*- coding: utf-8 -*-
# 标准库导入
from typing import TYPE_CHECKING

# 第三方库导入
from qfluentwidgets import (
    FluentIcon,
    ScrollArea,
    ExpandLayout,
    RangeSettingCard,
    SettingCardGroup,
    OptionsSettingCard,
    CustomColorSettingCard,
    ExpandGroupSettingCard,
    setTheme,
    setThemeColor,
)
from PySide6.QtCore import Qt, Slot
from PySide6.QtWidgets import QWidget

# 项目内模块导入
from src.Core.Config import cfg
from src.Ui.common.info_bar import success_bar
from src.Ui.SetupPage.ExpandGroupSettingItem import RangeItem, SwitchItem, ComboBoxItem

if TYPE_CHECKING:
    # 项目内模块导入
    from src.Ui.MainWindow import MainWindow


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
            configItem=cfg.themeMode,
            icon=FluentIcon.BRUSH,
            title=self.tr("切换主题"),
            content=self.tr("切换程序的主题"),
            texts=[self.tr("明亮模式"), self.tr("极夜模式"), self.tr("跟随系统")],
            parent=self.themeGroup,
        )
        self.themeColorCard = CustomColorSettingCard(
            configItem=cfg.themeColor,
            icon=FluentIcon.PALETTE,
            title=self.tr("主题颜色"),
            content=self.tr("选择主题色"),
            parent=self.themeGroup,
        )
        # 创建组 - 窗体
        self.windowGroup = SettingCardGroup(title=self.tr("窗体"), parent=self.view)
        # 创建项
        self.windowOpacityCard = RangeSettingCard(
            configItem=cfg.windowOpacity,
            icon=FluentIcon.FIT_PAGE,
            title=self.tr("窗口透明度"),
            content=self.tr("设置窗口的透明度"),
            parent=self.windowGroup,
        )
        self.zoomCard = OptionsSettingCard(
            configItem=cfg.dpiScale,
            icon=FluentIcon.ZOOM,
            title=self.tr("界面缩放"),
            content=self.tr("更改 Widget 和字体的大小"),
            texts=["100%", "125%", "150%", "175%", "200%", self.tr("跟随系统")],
            parent=self.windowGroup,
        )
        self.titleTabBarSettingCard = TitleTabBarSettingCard(self)

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
        from src.Ui.MainWindow.Window import MainWindow

        # 连接重启提示
        cfg.appRestartSig.connect(lambda: success_bar(self.tr("配置在重启后生效")))

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
        from src.Ui.HomePage import HomeWidget

        setTheme(cfg.get(theme), save=True)
        HomeWidget().updateBgImageSize()


class TitleTabBarSettingCard(ExpandGroupSettingCard):
    """
    ## 标题选项卡配置项
    """

    def __init__(self, parent) -> None:
        super().__init__(
            icon=FluentIcon.TAG,
            title=self.tr("标题选项卡配置"),
            content=self.tr("设置标题选项卡的开关和行为"),
            parent=parent,
        )

        # 创建项
        self.enabledTabBarItem = SwitchItem(cfg.titleTabBar, self.tr("启用标题选项卡"), self)
        self.enabledMovableItem = SwitchItem(cfg.titleTabBarMovable, self.tr("启用拖动标签页"), self)
        self.enabledScrollableItem = SwitchItem(cfg.titleTabBarScrollable, self.tr("启用标签页范围可滚动"), self)
        self.enabledTabShadowItem = SwitchItem(cfg.titleTabBarShadow, self.tr("启用标签页阴影"), self)
        self.setCloseModeItem = ComboBoxItem(
            cfg.titleTabBarCloseMode,
            self.tr("关闭按钮显示方式"),
            [self.tr("始终显示"), self.tr("悬停显示"), self.tr("永不显示")],
            self,
        )
        self.setTabMaximumWidthItem = RangeItem(cfg.titleTabBarMaxWidth, self.tr("标签最大宽度"), self)
        self.setTabMinimumWidthItem = RangeItem(cfg.titleTabBarMinWidth, self.tr("标签最小宽度"), self)

        # 调整内部布局
        self.viewLayout.setContentsMargins(0, 0, 0, 0)
        self.viewLayout.setSpacing(0)

        # 启用列表
        self.itemList = [
            self.enabledTabBarItem,
            self.enabledMovableItem,
            self.enabledScrollableItem,
            self.enabledTabShadowItem,
            self.setCloseModeItem,
            self.setTabMaximumWidthItem,
            self.setTabMinimumWidthItem,
        ]

        # 添加组件
        for item in self.itemList:
            self.addGroupWidget(item)

        # 调用方法
        self.signalConnect()
        self.enabledTabBarItemSlot(cfg.get(cfg.titleTabBar))
        self.enabledScrollableItemSlot(cfg.get(cfg.titleTabBarScrollable))

    def signalConnect(self) -> None:
        """
        ## 信号连接
        """
        # 项目内模块导入
        from src.Ui.MainWindow import MainWindow

        self.enabledTabBarItem.checkedChanged.connect(self.enabledTabBarItemSlot)
        self.enabledMovableItem.checkedChanged.connect(lambda state: MainWindow().title_bar.tabBar.setMovable(state))
        self.enabledScrollableItem.checkedChanged.connect(self.enabledScrollableItemSlot)
        self.enabledTabShadowItem.checkedChanged.connect(
            lambda state: MainWindow().title_bar.tabBar.setTabShadowEnabled(state)
        )
        self.setTabMaximumWidthItem.valueChanged.connect(
            lambda value: MainWindow().title_bar.tabBar.setTabMaximumWidth(value)
        )
        self.setTabMinimumWidthItem.valueChanged.connect(
            lambda value: MainWindow().title_bar.tabBar.setTabMinimumWidth(value)
        )

    @Slot(bool)
    def enabledTabBarItemSlot(self, state: bool) -> None:
        """
        ## 启用标题选项卡槽函数
        """
        # 项目内模块导入
        from src.Ui.MainWindow import MainWindow

        if not state:
            # 如果状态为 False, 则隐藏标题选项卡, 禁用其他配置项
            MainWindow().title_bar.tabBar.hide()
            [_.setEnabled(False) for _ in self.itemList if _ != self.enabledTabBarItem]
        else:
            # 如果状态为 True, 则显示标题选项卡, 启用其他配置项
            MainWindow().title_bar.tabBar.show()
            [_.setEnabled(True) for _ in self.itemList if _ != self.enabledTabBarItem]

    @Slot(bool)
    def enabledScrollableItemSlot(self, state: bool) -> None:
        """
        ## 启用标签页范围可滚动槽函数
        """
        # 项目内模块导入
        from src.Ui.MainWindow import MainWindow

        if state:
            # 如果状态为 True, 则设置标签页范围可滚动, 并且禁用调整标签页最大宽度和最小宽度
            MainWindow().title_bar.tabBar.setScrollable(True)
            self.setTabMaximumWidthItem.setEnabled(False)
            self.setTabMinimumWidthItem.setEnabled(False)
        else:
            # 如果状态为 False, 则设置标签页范围不可滚动, 并且启用调整标签页最大宽度和最小宽度
            MainWindow().title_bar.tabBar.setScrollable(False)
            self.setTabMaximumWidthItem.setEnabled(True)
            self.setTabMinimumWidthItem.setEnabled(True)

    def wheelEvent(self, event):
        """
        ## 滚动事件上传到父控件
        """
        self.parent().wheelEvent(event)
        super().wheelEvent(event)
