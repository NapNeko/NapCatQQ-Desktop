# -*- coding: utf-8 -*-

# 第三方库导入
from qfluentwidgets import FluentIcon, ScrollArea, ExpandLayout
from PySide6.QtCore import Qt
from PySide6.QtWidgets import QWidget

# 项目内模块导入
from src.Ui.Icon import NapCatDesktopIcon
from src.Ui.common.InputCard import SwitchConfigCard, ComboBoxConfigCard, LineEditConfigCard
from src.Core.Config.ConfigModel import BotConfig


class BotWidget(ScrollArea):
    """
    ## Bot Item 项对应的 QWidget
    """

    def __init__(self, parent=None, config: BotConfig = None) -> None:
        super().__init__(parent=parent)
        self.setObjectName("BotWidget")
        self.view = QWidget()
        self.cardLayout = ExpandLayout(self)

        # 设置 ScrollArea
        self.setWidget(self.view)
        self.setWidgetResizable(True)
        self.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        self.view.setObjectName("BotWidgetView")

        # 调用方法
        self._initWidget()
        self._setLayout()

        if config is not None:
            # 如果传入了 config 则进行解析并填充内部卡片
            self.config = config
            self.fillValue()

    def _initWidget(self) -> None:
        """
        ## 初始化 QWidget 所需要的控件并配置
        创建 InputCard
        """
        self.botNameCard = LineEditConfigCard(
            icon=FluentIcon.ROBOT,
            title=self.tr("Bot 名称"),
            content=self.tr("设置机器人的名称,为空自动生成"),
            placeholder_text=self.tr("Bot 1"),
            parent=self.view,
        )
        self.botQQIdCard = LineEditConfigCard(
            icon=NapCatDesktopIcon.QQ,
            title=self.tr("Bot QQ"),
            content=self.tr("设置机器人 QQ 号"),
            placeholder_text=self.tr("12345689"),
            parent=self.view,
        )
        self.messageFormatCard = ComboBoxConfigCard(
            icon=FluentIcon.MESSAGE,
            title=self.tr("消息格式"),
            content=self.tr("消息上报格式，array为消息组，string为cq码字符串"),
            texts=["array", "string"],
            parent=self.view,
        )
        self.reportSelfMessageCard = SwitchConfigCard(
            icon=FluentIcon.ROBOT,
            title=self.tr("上报自身消息"),
            content=self.tr("是否上报自己发送的消息"),
            parent=self.view,
        )
        self.musicSignUrl = LineEditConfigCard(
            icon=FluentIcon.MUSIC,
            title=self.tr("音乐签名URL"),
            content=self.tr("用于处理音乐相关请求"),
            placeholder_text=self.tr("Can be empty"),
            parent=self.view,
        )
        self.heartIntervalCard = LineEditConfigCard(
            icon=FluentIcon.HEART,
            title=self.tr("心跳间隔"),
            content=self.tr("WebSocket 心跳间隔, 单位毫秒"),
            placeholder_text="30000",
            parent=self.view,
        )
        self.accessTokenCard = LineEditConfigCard(
            icon=FluentIcon.CERTIFICATE,
            title=self.tr("Access Token"),
            content=self.tr("Access Token, 可为空"),
            parent=self.view,
        )

        self.cards = [
            self.botNameCard,
            self.botQQIdCard,
            self.messageFormatCard,
            self.reportSelfMessageCard,
            self.musicSignUrl,
            self.heartIntervalCard,
            self.accessTokenCard,
        ]

    def fillValue(self) -> None:
        """
        ## 如果传入了 config 则对其内部卡片的值进行填充
        """
        self.botNameCard.fillValue(self.config.name)
        self.botQQIdCard.fillValue(self.config.QQID)
        self.messageFormatCard.fillValue(self.config.messagePostFormat)
        self.reportSelfMessageCard.fillValue(self.config.reportSelfMessage)
        self.musicSignUrl.fillValue(self.config.musicSignUrl)
        self.heartIntervalCard.fillValue(self.config.heartInterval)
        self.accessTokenCard.fillValue(self.config.token)

    def _setLayout(self) -> None:
        """
        ## 将 QWidget 内部的 InputCard 添加到布局中
        """
        self.cardLayout.setContentsMargins(0, 0, 0, 0)
        self.cardLayout.setSpacing(2)
        for card in self.cards:
            self.cardLayout.addWidget(card)
            self.adjustSize()

        self.view.setLayout(self.cardLayout)

    def getValue(self) -> dict:
        """
        ## 返回内部卡片的配置结果
        """
        return {
            "name": self.botNameCard.getValue(),
            "QQID": self.botQQIdCard.getValue(),
            "messagePostFormat": self.messageFormatCard.getValue(),
            "reportSelfMessage": self.reportSelfMessageCard.getValue(),
            "musicSignUrl": self.musicSignUrl.getValue(),
            "heartInterval": int(self.heartIntervalCard.getValue()) if self.heartIntervalCard.getValue() else 0,
            "token": self.accessTokenCard.getValue(),
        }

    def clearValues(self) -> None:
        """
        ## 清空(还原)内部卡片的配置
        """
        for card in self.cards:
            card.clear()

    def adjustSize(self) -> None:
        h = self.cardLayout.heightForWidth(self.width()) + 46
        return self.resize(self.width(), h)
