# -*- coding: utf-8 -*-
from typing import Optional, List

from PySide6.QtCore import Qt, QTimer, QUrl, QUrlQuery, Slot
from PySide6.QtGui import QPixmap
from PySide6.QtNetwork import QNetworkRequest, QNetworkReply
from PySide6.QtWidgets import QWidget, QVBoxLayout, QHBoxLayout, QFrame
from creart import it
from qfluentwidgets import (
    HeaderCardWidget, FluentIcon, TransparentToolButton, ImageLabel, TransparentPushButton, BodyLabel, ScrollArea
)
from qfluentwidgets.common.animation import BackgroundAnimationWidget

from src.Core import timer
from src.Core.Config.ConfigModel import Config
from src.Core.NetworkFunc import Urls, NetworkFunc
from src.Ui.BotListPage import BotListWidget
from src.Ui.StyleSheet import StyleSheet


class BotListCard(HeaderCardWidget):
    """
    ## 用于在首页展示 bot 列表的卡片
    """

    def __init__(self, parent=None) -> None:
        """
        ## 初始化控件
        """
        super().__init__(parent=parent)
        # 创建标签和布局
        self.cardLayout = QVBoxLayout()
        self.noBotLabel = BodyLabel(self.tr("No bots were added ＞﹏＜"), self)
        self.toAddBot = TransparentToolButton(FluentIcon.CHEVRON_RIGHT, self)
        self.botList = BotList(self)

        # 设置控件
        self.setTitle(self.tr("Bot List"))
        self.botList.hide()
        self.toAddBot.clicked.connect(self._toAddBotSlot)

        # 调用方法
        self._setLayout()
        self.monitorBots()

    @staticmethod
    @Slot()
    def _toAddBotSlot() -> None:
        """
        ## 跳转到 AddPage 页面
        """
        from src.Ui.MainWindow.Window import MainWindow
        it(MainWindow).add_widget_button.click()

    @timer(2000)
    def monitorBots(self) -> None:
        """
        ## 监控机器人列表
        """
        if not it(BotListWidget).botList.botCardList:
            # 如果为空则代表没有机器人, 显示提示
            self.botList.hide()
            self.noBotLabel.show()
            self.toAddBot.show()
        else:
            # 不为空则刷新一次列表检查是否有变化
            self.botList.show()
            self.noBotLabel.hide()
            self.toAddBot.hide()
            self.botList.updateList()

    def _setLayout(self) -> None:
        """
        ## 对内部进行布局
        """
        self.headerLayout.addWidget(self.toAddBot, 0, Qt.AlignmentFlag.AlignRight)
        self.cardLayout.addWidget(self.noBotLabel, 0, Qt.AlignmentFlag.AlignCenter)
        self.cardLayout.addWidget(self.botList)
        self.cardLayout.setContentsMargins(0, 0, 0, 0)
        self.cardLayout.setSpacing(2)

        self.viewLayout.addLayout(self.cardLayout)
        self.viewLayout.setContentsMargins(0, 0, 0, 0)


class BotList(ScrollArea):
    """
    ## BotListWidget 内部的机器人列表

    自动读取配置文件中已有的的机器人配置
    """

    def __init__(self, parent) -> None:
        """
        ## 初始化
        """
        super().__init__(parent=parent)
        # 创建属性
        self.botCardList: List[BotCard] = []

        # 调用方法
        self._createView()
        self._initWidget()

        StyleSheet.BOT_LIST_WIDGET.apply(self)

    def _createView(self) -> None:
        """
        ## 构建并设置 ScrollArea 所需的 widget
        """
        self.view = QWidget(self)
        self.cardLayout = QVBoxLayout(self.view)
        self.cardLayout.setContentsMargins(15, 8, 15, 10)
        self.cardLayout.setAlignment(Qt.AlignmentFlag.AlignTop)
        self.cardLayout.setSpacing(6)
        self.view.setObjectName("BotListView")
        self.view.setLayout(self.cardLayout)

    def _initWidget(self) -> None:
        """
        ## 设置 ScrollArea
        """
        self.setWidget(self.view)
        self.setWidgetResizable(True)
        self.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)

    def updateList(self) -> None:
        """
        ## 刷新机器人列表
        """
        if not self.botCardList:
            # 如果是首次运行,直接添加到 botCardList
            for bot_config in it(BotListWidget).botList.botList:
                card = BotCard(bot_config, self)
                self.cardLayout.addWidget(card, 0, Qt.AlignmentFlag.AlignTop)
                self.botCardList.append(card)

        QQList = [card.config.bot.QQID for card in self.botCardList]
        for bot_config in it(BotListWidget).botList.botList:
            # 遍历并判断是否有新增的 bot
            if bot_config.bot.QQID in QQList:
                # 如果属于则直接跳过
                continue

            # 不属于则就属于新增, 创建 card 并 添加到布局
            card = BotCard(bot_config)
            self.cardLayout.addWidget(card, 0, Qt.AlignmentFlag.AlignTop)
            self.botCardList.append(card)

        for card in self.botCardList:
            # 遍历并判断是否有减少的 bot
            if card.config in it(BotListWidget).botList.botList:
                # 属于则就是没有被删除, 跳过
                continue

            # 移除出布局并删除
            self.botCardList.remove(card)
            self.cardLayout.removeWidget(card)
            card.deleteLater()

        # 刷新一次布局
        self.cardLayout.update()


class BotCard(BackgroundAnimationWidget, QFrame):
    """
    ## BotListCard 中展示的 BotCard
    """

    def __init__(self, config: Config, parent=None) -> None:
        """
        ## 初始化机器人
        """
        super().__init__(parent=parent)
        self.timer: Optional[QTimer] = None
        self.config = config

        # 创建布局和标签
        self.hBoxLayout = QHBoxLayout()
        self.QQAvatarLabel = ImageLabel(":Global/logo.png", self)
        self.botNameLabel = BodyLabel(f"{self.config.bot.name}({self.config.bot.QQID})", self)
        self.runButton = TransparentPushButton(FluentIcon.POWER_BUTTON, self.tr("Start"), self)
        self.stopButton = TransparentPushButton(FluentIcon.POWER_BUTTON, self.tr("Stop"), self)

        # 连接信号以及设置控件
        self.runButton.clicked.connect(self._runButtonSlot)
        self.stopButton.clicked.connect(self._stopButtonSlot)
        self.stopButton.hide()

        # 调用方法
        self._QQAvatar()
        self._setLayout()
        self.monitorBots()

    @timer(2000)
    def monitorBots(self) -> None:
        """
        ## 监控机器人列表
        """
        from src.Ui.BotListPage.BotListWidget import BotListWidget
        for card in it(BotListWidget).botList.botCardList:
            if self.config.bot.QQID != card.config.bot.QQID:
                continue
            else:
                if card.botWidget is None:
                    return
                if card.botWidget.isRun:
                    self.runButton.hide()
                    self.stopButton.show()
                else:
                    self.runButton.show()
                    self.stopButton.hide()

    @Slot()
    def _runButtonSlot(self) -> None:
        """
        ## 运行按钮
        """
        from src.Ui.MainWindow import MainWindow
        from src.Ui.BotListPage.BotListWidget import BotListWidget
        for card in it(BotListWidget).botList.botCardList:
            if self.config.bot.QQID == card.config.bot.QQID:
                it(MainWindow).bot_list_widget_button.click()
                card.clicked.emit()
                card.botWidget.runButton.click()
                break
        self.runButton.hide()
        self.stopButton.show()

    @Slot()
    def _stopButtonSlot(self) -> None:
        """
        ## 停止按钮
        """
        from src.Ui.BotListPage.BotListWidget import BotListWidget
        for card in it(BotListWidget).botList.botCardList:
            if self.config.bot.QQID == card.config.bot.QQID:
                card.botWidget.stopButton.click()
            self.runButton.show()
            self.stopButton.hide()

    def _QQAvatar(self) -> None:
        """
        ## 创建展示 QQ头像 的 ImageLabel
        """
        self.QQAvatarLabel.scaledToHeight(28)
        self.QQAvatarLabel.setBorderRadius(5, 5, 5, 5)

        # 处理 QQ头像 的 Url
        avatar_url: QUrl = Urls.QQ_AVATAR.value
        query = QUrlQuery()
        query.addQueryItem("spec", "640")
        query.addQueryItem("dst_uin", self.config.bot.QQID)
        avatar_url.setQuery(query)

        # 创建请求并链接槽函数
        request = QNetworkRequest(avatar_url)
        replay = it(NetworkFunc).manager.get(request)
        replay.finished.connect(lambda: self._setAvatar(replay))

    def _setAvatar(self, replay: QNetworkReply) -> None:
        """
        ## 设置头像
        """
        if replay.error() == QNetworkReply.NetworkError.NoError:
            # 如果请求成功则设置反之显示错误提示
            avatar = QPixmap()
            avatar.loadFromData(replay.readAll())
            self.QQAvatarLabel.setImage(avatar)
            self.QQAvatarLabel.scaledToHeight(28)
            self.QQAvatarLabel.setBorderRadius(5, 5, 5, 5)
        else:
            from src.Ui.HomePage import HomeWidget
            it(HomeWidget).showError(
                title=self.tr("Failed to get the QQ avatar"),
                content=replay.errorString()
            )

    def _setLayout(self) -> None:
        """
        ## 对控件进行布局
        """

        self.hBoxLayout.addWidget(self.QQAvatarLabel)
        self.hBoxLayout.addWidget(self.botNameLabel)
        self.hBoxLayout.addStretch(1)
        self.hBoxLayout.addWidget(self.runButton)
        self.hBoxLayout.addWidget(self.stopButton)

        self.setLayout(self.hBoxLayout)
