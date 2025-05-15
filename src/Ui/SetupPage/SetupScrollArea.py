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
from src.Ui.SetupPage.ExpandGroupSettingItem import FileItem, RangeItem, SwitchItem, ComboBoxItem, LineEditItem

if TYPE_CHECKING:
    # 项目内模块导入
    from src.Ui.MainWindow import MainWindow


class SetupScrollArea(ScrollArea):

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

        # 调用方法
        self._createConfigCards()
        self._connect_signal()
        self._setLayout()

    def _createConfigCards(self) -> None:
        """
        创建配置项卡片
        """
        # 创建组 - 个性化
        self.personalGroup = SettingCardGroup(title=self.tr("个性化"), parent=self.view)
        # 创建项
        self.themeCard = OptionsSettingCard(
            configItem=cfg.themeMode,
            icon=FluentIcon.BRUSH,
            title=self.tr("切换主题"),
            content=self.tr("切换程序的主题"),
            texts=[self.tr("明亮模式"), self.tr("极夜模式"), self.tr("跟随系统")],
            parent=self.personalGroup,
        )
        self.themeColorCard = CustomColorSettingCard(
            configItem=cfg.themeColor,
            icon=FluentIcon.PALETTE,
            title=self.tr("主题颜色"),
            content=self.tr("选择主题色"),
            parent=self.personalGroup,
        )
        self.windowOpacityCard = RangeSettingCard(
            configItem=cfg.windowOpacity,
            icon=FluentIcon.FIT_PAGE,
            title=self.tr("窗口透明度"),
            content=self.tr("设置窗口的透明度"),
            parent=self.personalGroup,
        )
        self.zoomCard = OptionsSettingCard(
            configItem=cfg.dpiScale,
            icon=FluentIcon.ZOOM,
            title=self.tr("界面缩放"),
            content=self.tr("更改 Widget 和字体的大小"),
            texts=["100%", "125%", "150%", "175%", "200%", self.tr("跟随系统")],
            parent=self.personalGroup,
        )
        self.titleTabBarSettingCard = TitleTabBarSettingCard(self)
        self.closeBtnCard = OptionsSettingCard(
            configItem=cfg.closeBtnAction,
            icon=FluentIcon.CLOSE,
            title=self.tr("关闭按钮"),
            content=self.tr("选择点击关闭按钮时的行为"),
            texts=[self.tr("关闭程序"), self.tr("最小化隐藏到托盘")],
            parent=self.personalGroup,
        )

        # 创建组 - 事件
        self.eventGroup = SettingCardGroup(title=self.tr("事件"), parent=self.view)
        # 创建项
        self.botOfflineEventCard = BotOfflineEventCard(self)

    def _setLayout(self) -> None:
        """
        控件布局
        """
        # 将卡片添加到组

        self.personalGroup.addSettingCard(self.themeCard)
        self.personalGroup.addSettingCard(self.themeColorCard)
        # self.personalGroup.addSettingCard(self.languageCard)
        self.personalGroup.addSettingCard(self.zoomCard)
        self.personalGroup.addSettingCard(self.windowOpacityCard)
        self.personalGroup.addSettingCard(self.titleTabBarSettingCard)
        self.personalGroup.addSettingCard(self.closeBtnCard)

        self.eventGroup.addSettingCard(self.botOfflineEventCard)

        # 添加到布局
        # self.expand_layout.addWidget(self.startGroup)
        self.expand_layout.addWidget(self.personalGroup)
        self.expand_layout.addWidget(self.eventGroup)
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


class BotOfflineEventCard(ExpandGroupSettingCard):
    """
    ## 机器人离线事件
    """

    def __init__(self, parent) -> None:
        super().__init__(
            icon=FluentIcon.RINGER,
            title=self.tr("机器人离线事件"),
            content=self.tr("设置机器人离线时的事件"),
            parent=parent,
        )

        # 创建项
        self.enabledEmailNoticeItem = SwitchItem(cfg.botOfflineEmailNotice, self.tr("启用邮件通知"), self)
        self.emailReceiversItem = LineEditItem(
            cfg.emailReceiver, self.tr("接收人邮箱"), placeholder_text=self.tr("A@qq.com"), parent=self
        )
        self.emailSenderItem = LineEditItem(
            cfg.emailSender, self.tr("发送人邮箱"), placeholder_text=self.tr("B@qq.com"), parent=self
        )
        self.emailToken = LineEditItem(cfg.emailToken, self.tr("发送人邮箱密钥"), parent=self)
        self.emailStmpServer = LineEditItem(cfg.emailStmpServer, self.tr("SMTP服务器"), parent=self)

        # 调整内部布局
        self.viewLayout.setContentsMargins(0, 0, 1, 0)
        self.viewLayout.setSpacing(2)

        # 添加组件
        self.addGroupWidget(self.enabledEmailNoticeItem)
        self.addGroupWidget(self.emailReceiversItem)
        self.addGroupWidget(self.emailSenderItem)
        self.addGroupWidget(self.emailToken)
        self.addGroupWidget(self.emailStmpServer)

        # 信号连接
        self.enabledEmailNoticeItem.checkedChanged.connect(self.isHide)

        # 调用方法
        self.isHide()

    def isHide(self) -> None:
        """
        ## 判断是否需要隐藏
        """
        for item in [self.enabledEmailNoticeItem]:
            if cfg.get(item.configItem):
                self.showItem(item)
            else:
                self.hideItem(item)

        self._adjustViewSize()

    def showItem(self, item: SwitchItem) -> None:
        """
        ## 显示项
        """
        match item.configItem:
            case cfg.botOfflineEmailNotice:
                self.emailReceiversItem.setEnabled(True)
                self.emailSenderItem.setEnabled(True)
                self.emailToken.setEnabled(True)
                self.emailStmpServer.setEnabled(True)

    def hideItem(self, item: SwitchItem) -> None:
        """
        ## 隐藏项
        """
        match item.configItem:
            case cfg.botOfflineEmailNotice:
                self.emailReceiversItem.setEnabled(False)
                self.emailSenderItem.setEnabled(False)
                self.emailToken.setEnabled(False)
                self.emailStmpServer.setEnabled(False)

    def wheelEvent(self, event) -> None:
        """
        ## 滚动事件上传到父控件
        """
        self.parent().wheelEvent(event)
        super().wheelEvent(event)
