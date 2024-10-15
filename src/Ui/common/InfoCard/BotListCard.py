# -*- coding: utf-8 -*-
# 标准库导入
from typing import List, Optional

# 第三方库导入
from creart import it
from loguru import logger
from qfluentwidgets import (
    BodyLabel,
    FluentIcon,
    ImageLabel,
    ScrollArea,
    HeaderCardWidget,
    TransparentPushButton,
    TransparentToolButton,
    setFont,
)
from qfluentwidgets.common.animation import BackgroundAnimationWidget
from PySide6.QtGui import QPixmap
from PySide6.QtCore import Qt, QUrl, Slot, QTimer, QUrlQuery
from PySide6.QtNetwork import QNetworkReply, QNetworkRequest
from PySide6.QtWidgets import QFrame, QWidget, QHBoxLayout, QVBoxLayout

# 项目内模块导入
from src.Core import timer
from src.Ui.StyleSheet import StyleSheet
from src.Ui.BotListPage import BotListWidget
from src.Core.NetworkFunc import NetworkFunc
from src.Ui.common.info_bar import error_bar
from src.Core.NetworkFunc.Urls import Urls
from src.Core.Config.ConfigModel import Config
from src.Core.Config.OperateConfig import read_config


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
        self.noBotLabel = BodyLabel(self.tr("没有添加机器人 ＞﹏＜"), self)
        self.toAddBot = TransparentToolButton(FluentIcon.CHEVRON_RIGHT, self)
        self.botList = BotList(self)

        # 设置控件
        self.setTitle(self.tr("机器人列表"))
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
        # 项目内模块导入
        from src.Ui.AddPage import AddWidget
        from src.Ui.MainWindow.Window import MainWindow

        it(MainWindow).switchTo(it(AddWidget))

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

        if not (bot_configs := read_config()):
            return

        if not self.botCardList:
            # 如果是首次运行, 则直接添加到布局和 botCardList
            for config in bot_configs:
                card = BotCard(config, self)
                self.cardLayout.addWidget(card)
                self.botCardList.append(card)
            return

        qq_id_list = [card.config.bot.QQID for card in self.botCardList]
        for bot_config in bot_configs:
            # 遍历并判断是否有新增的 bot 配置
            if bot_config.bot.QQID not in qq_id_list:
                # 不属于则就属于新增, 创建 card 并添加到布局
                new_card = BotCard(bot_config)
                self.cardLayout.addWidget(new_card)
                self.botCardList.append(new_card)

        for card in self.botCardList:
            # 遍历并判断是否有减少的 bot 配置
            if card.config not in bot_configs:
                # 不属于则代表已经删除, 移除出布局并删除
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
        self.runButton = TransparentPushButton(FluentIcon.POWER_BUTTON, self.tr("启动"), self)
        self.stopButton = TransparentPushButton(FluentIcon.POWER_BUTTON, self.tr("停止"), self)

        # 连接信号以及设置控件
        self.runButton.clicked.connect(self._runButtonSlot)
        self.stopButton.clicked.connect(self._stopButtonSlot)
        self.stopButton.hide()

        # 调整样式
        self.setMinimumHeight(60)
        setFont(self.botNameLabel, 16)

        # 调用方法
        self._QQAvatar()
        self._setLayout()
        self.monitorBots()

    @timer(2000)
    def monitorBots(self) -> None:
        """
        ## 监控机器人列表
        """
        # 项目内模块导入
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
        # 项目内模块导入
        from src.Ui.MainWindow import MainWindow
        from src.Ui.BotListPage.BotListWidget import BotListWidget

        for card in it(BotListWidget).botList.botCardList:
            if self.config.bot.QQID == card.config.bot.QQID:
                it(MainWindow).switchTo(it(BotListWidget))
                card.clicked.emit()
                card.botWidget.runButtonSlot()
                break
        self.runButton.hide()
        self.stopButton.show()

    @Slot()
    def _stopButtonSlot(self) -> None:
        """
        ## 停止按钮
        """
        # 项目内模块导入
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
            self.QQAvatarLabel.scaledToHeight(48)
            self.QQAvatarLabel.setBorderRadius(5, 5, 5, 5)
        else:
            logger.error(f"获取 QQ 头像时引发错误: {replay.errorString()}")
            error_bar(self.tr("获取 QQ 头像时引发错误, 请前往 设置 > log 查看错误原因"))

    def _setLayout(self) -> None:
        """
        ## 对控件进行布局
        """

        self.hBoxLayout.addWidget(self.QQAvatarLabel)
        self.hBoxLayout.addSpacing(10)
        self.hBoxLayout.addWidget(self.botNameLabel)
        self.hBoxLayout.addStretch(1)
        self.hBoxLayout.addWidget(self.runButton)
        self.hBoxLayout.addWidget(self.stopButton)

        self.setLayout(self.hBoxLayout)
