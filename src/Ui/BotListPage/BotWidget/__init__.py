# -*- coding: utf-8 -*-
import json
from typing import TYPE_CHECKING, List

from PySide6.QtCore import Qt
from PySide6.QtWidgets import QWidget, QVBoxLayout, QHBoxLayout, QStackedWidget
from creart import it
from qfluentwidgets import (
    SegmentedWidget, TransparentToolButton, FluentIcon, ToolTipFilter, PrimaryPushButton, InfoBar, InfoBarPosition
)

from src.Core.Config.ConfigModel import Config
from src.Core.PathFunc import PathFunc
from src.Ui.BotListPage.BotWidget.BotSetupPage import BotSetupPage
from src.Ui.StyleSheet import StyleSheet


class BotWidget(QWidget):
    """
    ## 机器人卡片对应的 Widget
    """

    def __init__(self, config: Config) -> None:
        super().__init__()
        self.config = config
        # 创建所需控件
        self._createView()
        self._createPivot()
        self._createButton()

        self.vBoxLayout = QVBoxLayout()
        self.hBoxLayout = QHBoxLayout()
        self.buttonLayout = QHBoxLayout()

        # 调用方法
        self._setLayout()
        self._addTooltips()

        StyleSheet.BOT_WIDGET.apply(self)

    def _createPivot(self) -> None:
        """
        ## 创建机器人 Widget 顶部导航栏
            - routeKey 使用 QQ号 作为前缀保证该 pivot 的 objectName 全局唯一
        """
        self.pivot = SegmentedWidget(self)
        self.pivot.addItem(
            routeKey=self.botInfoPage.objectName(),
            text=self.tr("Bot info"),
            onClick=lambda: self.view.setCurrentWidget(self.botInfoPage)
        )
        self.pivot.addItem(
            routeKey=self.botSetupPage.objectName(),
            text=self.tr("Bot Setup"),
            onClick=lambda: self.view.setCurrentWidget(self.botSetupPage)
        )
        self.pivot.addItem(
            routeKey=self.botLogPage.objectName(),
            text=self.tr("Bot Log"),
            onClick=lambda: self.view.setCurrentWidget(self.botLogPage)
        )
        self.pivot.setCurrentItem(self.botInfoPage.objectName())
        self.pivot.setMaximumWidth(300)

    def _createView(self) -> None:
        """
        ## 创建用于切换页面的 view
        """
        # 创建 view 和 页面
        self.view = QStackedWidget()
        self.botInfoPage = QWidget(self)
        self.botInfoPage.setObjectName(f"{self.config.bot.QQID}_BotWidgetPivot_BotInfo")

        self.botSetupPage = BotSetupPage(self.config, self)

        self.botLogPage = QWidget(self)
        self.botLogPage.setObjectName(f"{self.config.bot.QQID}_BotWidgetPivot_BotLog")

        # 将页面添加到 view
        self.view.addWidget(self.botInfoPage)
        self.view.addWidget(self.botSetupPage)
        self.view.addWidget(self.botLogPage)
        self.view.setObjectName("BotView")
        self.view.setCurrentWidget(self.botInfoPage)
        self.view.currentChanged.connect(self._pivotSolt)

    def _createButton(self) -> None:
        """
        ## 创建按钮并设置
        """
        # 创建按钮
        self.updateConfigButton = PrimaryPushButton(FluentIcon.UPDATE, self.tr("Update config"))  # 更新配置按钮
        self.returnListButton = TransparentToolButton(FluentIcon.RETURN)  # 返回到列表按钮
        self.botSetupSubPageReturnButton = TransparentToolButton(FluentIcon.RETURN)  # 返回到 BotSetup 按钮

        # 连接槽函数
        self.updateConfigButton.clicked.connect(self._updateButtonSolt)
        self.botSetupSubPageReturnButton.clicked.connect(self._botSetupSubPageReturnButtonSolt)
        self.returnListButton.clicked.connect(self._returnListButtonSolt)

        # 隐藏按钮
        self.updateConfigButton.hide()
        self.botSetupSubPageReturnButton.hide()

    def _addTooltips(self) -> None:
        """
        ## 为按钮添加悬停提示
        """
        self.returnListButton.setToolTip(self.tr("Click Back to list"))
        self.returnListButton.installEventFilter(ToolTipFilter(self.returnListButton))

        self.botSetupSubPageReturnButton.setToolTip(self.tr("Click Back to BotSetup"))
        self.botSetupSubPageReturnButton.installEventFilter(ToolTipFilter(self.botSetupSubPageReturnButton))

    def _updateButtonSolt(self) -> None:
        """
        ## 更新按钮的槽函数
        """
        self.newConfig = Config(**self.botSetupPage.getValue())

        # 读取配置列表
        with open(str(it(PathFunc).bot_config_path), "r", encoding="utf-8") as f:
            bot_configs = json.load(f)
            bot_configs = [Config(**config) for config in bot_configs]
        if bot_configs:
            # 如果从文件加载的 bot_config 不为空则进行更新(为空怎么办呢,我能怎么办,崩了呗bushi
            for index, config in enumerate(bot_configs):
                # 遍历配置列表,找到一样则替换
                if config.bot.QQID == self.newConfig.bot.QQID:
                    bot_configs[index] = self.newConfig
                    break
            # 不可以直接使用 dict方法 转为 dict对象, 内部 WebsocketUrl 和 HttpUrl 不会自动转为 str
            bot_configs = [json.loads(config.json()) for config in bot_configs]
            with open(str(it(PathFunc).bot_config_path), "w", encoding="utf-8") as f:
                json.dump(bot_configs, f, indent=4)
            # 更新成功提示
            InfoBar.success(
                title=self.tr("Update success"),
                content=self.tr("The updated configuration is successful"),
                orient=Qt.Orientation.Vertical,
                position=InfoBarPosition.BOTTOM_RIGHT,
                duration=2000,
                parent=self.parent().parent()
            )
        else:
            # 为空报错
            InfoBar.error(
                title=self.tr("Update error"),
                content=self.tr("Data loss within the profile"),
                orient=Qt.Orientation.Vertical,
                position=InfoBarPosition.BOTTOM_RIGHT,
                duration=50000,
                isClosable=True,
                parent=self.parent().parent()
            )


    @staticmethod
    def _returnListButtonSolt() -> None:
        """
        ## 返回列表按钮的槽函数
        """
        from src.Ui.BotListPage.BotListWidget import BotListWidget
        it(BotListWidget).view.setCurrentIndex(0)
        it(BotListWidget).topCard.breadcrumbBar.setCurrentIndex(0)
        it(BotListWidget).topCard.updateListButton.show()

    def _botSetupSubPageReturnButtonSolt(self) -> None:
        """
        ## BotSetup 页面中子页面返回按钮的槽函数
        """
        self.botSetupSubPageReturnButton.hide()
        self.returnListButton.show()
        self.view.setCurrentWidget(self.botSetupPage)

    def _pivotSolt(self, index: int) -> None:
        """
        ## pivot 切换槽函数
        """
        widget = self.view.widget(index)
        self.pivot.setCurrentItem(widget.objectName())

        # 匹配切换的页面并进行一些设置
        if widget.objectName() == self.botInfoPage.objectName():
            self.returnListButton.show()
            self.updateConfigButton.hide()
            self.botSetupSubPageReturnButton.hide()
        if widget.objectName() == self.botSetupPage.objectName():
            self.updateConfigButton.show()
            self.botSetupSubPageReturnButton.show()
            self.returnListButton.hide()
        if widget.objectName() == self.botLogPage.objectName():
            self.returnListButton.show()
            self.updateConfigButton.hide()
            self.botSetupSubPageReturnButton.hide()

    def _setLayout(self) -> None:
        """
        ## 对内部进行布局
        """
        self.buttonLayout.setSpacing(4)
        self.buttonLayout.setContentsMargins(0, 0, 0, 0)
        self.buttonLayout.addWidget(self.updateConfigButton)
        self.buttonLayout.addWidget(self.returnListButton)
        self.buttonLayout.addWidget(self.botSetupSubPageReturnButton)
        self.buttonLayout.setAlignment(Qt.AlignmentFlag.AlignRight)

        self.hBoxLayout.setSpacing(0)
        self.hBoxLayout.setContentsMargins(0, 0, 0, 0)
        self.hBoxLayout.addWidget(self.pivot)
        self.hBoxLayout.addLayout(self.buttonLayout)

        self.vBoxLayout.setSpacing(0)
        self.vBoxLayout.setContentsMargins(0, 0, 0, 0)
        self.vBoxLayout.addLayout(self.hBoxLayout)
        self.vBoxLayout.addSpacing(10)
        self.vBoxLayout.addWidget(self.view)

        self.setLayout(self.vBoxLayout)
