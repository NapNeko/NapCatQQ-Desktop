# -*- coding: utf-8 -*-

"""
添加机器人
"""
from abc import ABC
from typing import TYPE_CHECKING, Self

from PySide6.QtWidgets import QWidget, QVBoxLayout
from creart import add_creator, exists_module
from creart import it
from creart.creator import AbstractCreator, CreateTargetInfo
from qfluentwidgets.common import FluentIcon
from qfluentwidgets.components import SettingCardGroup, ExpandLayout

from src.Core.PathFunc import PathFunc
from src.Ui import PageBase
from src.Ui.AddPage.Card import (
    LineEditConfigCard, ComboBoxConfigCard, HttpConfigCard,
    SwitchConfigCard, HttpReportConfigCard, WsConfigCard,
    WsReverseConfigCard, ConfigTopCard, FolderConfigCard
)
from src.Ui.Icon import NapCatDesktopIcon
from src.Ui.StyleSheet import StyleSheet

if TYPE_CHECKING:
    from src.Ui.MainWindow import MainWindow


class AddWidget(PageBase):

    def __init__(self):
        super().__init__()

    def initialize(self, parent: "MainWindow") -> Self:
        """
        初始化
        """
        # 创建控件
        self.view = QWidget()
        self.expandLayout = ExpandLayout()
        self.viewLayout = QVBoxLayout()

        # 设置 ScrollArea
        self.setParent(parent),
        self.setObjectName("AddPage")
        self.setWidget(self.view)
        self.setWidgetResizable(True)
        self.view.setObjectName("AddView")

        # 调用方法
        self.updateBgImage()
        self.__createConfigCards()
        self.__setLayout()

        # 应用样式表
        StyleSheet.ADD_WIDGET.apply(self)

        return self

    def __createConfigCards(self) -> None:
        """
        创建配置卡片
        """
        # 创建顶部栏
        self.topCard = ConfigTopCard(self)
        # 创建组 - 机器人设置
        self.botGroup = SettingCardGroup(
            title=self.tr("Robot settings"), parent=self.view
        )
        self.botNameCard = LineEditConfigCard(
            icon=FluentIcon.ROBOT,
            title=self.tr("Bot name"),
            content=self.tr("Set your bot name"),
            placeholder_text=self.tr("Bot 1"),
        )
        self.botQQIdCard = LineEditConfigCard(
            icon=NapCatDesktopIcon.QQ,
            title=self.tr("Bot QQ"),
            content=self.tr("Set your bot QQ"),
            placeholder_text=self.tr("123456")
        )
        self.httpCard = HttpConfigCard(self)
        self.httpReportCard = HttpReportConfigCard(self)
        self.wsCard = WsConfigCard(self)
        self.wsReverseCard = WsReverseConfigCard(self)
        self.messageFormatCard = ComboBoxConfigCard(
            icon=FluentIcon.MESSAGE,
            title=self.tr("Message format"),
            content=self.tr(
                "Array is the message group, and string is the cq code string"
            ),
            texts=["array", "string"],
            parent=self
        )
        self.reportSelfMessageCard = SwitchConfigCard(
            icon=FluentIcon.ROBOT,
            title=self.tr("Report self message"),
            content=self.tr("Whether to report the bot's own message"),
            parent=self
        )
        self.debugModeCard = SwitchConfigCard(
            icon=FluentIcon.COMMAND_PROMPT,
            title=self.tr("Debug"),
            content=self.tr(
                "The message will carry a raw field, "
                "which is the original message content"
            ),
            parent=self
        )
        self.localFile2UrlCard = SwitchConfigCard(
            icon=FluentIcon.SHARE,
            title=self.tr("LocalFile2Url"),
            content=self.tr(
                "If the URL cannot be obtained when calling the get file interface, "
                "use the base 64 field to return the file content"
            ),
            parent=self
        )
        self.heartIntervalCard = LineEditConfigCard(
            icon=FluentIcon.HEART,
            title=self.tr("Heart interval"),
            content=self.tr("WebSocket heartbeat interval, in milliseconds"),
            placeholder_text="30000",
            parent=self
        )
        self.accessTokenCard = LineEditConfigCard(
            icon=FluentIcon.CERTIFICATE,
            title=self.tr("Access Token"),
            content=self.tr("Access Token, can be empty"),
            parent=self
        )

        # 创建组 - 高级设置
        self.advancedGroup = SettingCardGroup(
            title=self.tr("Advanced setting"), parent=self.view
        )
        self.QQPathCard = FolderConfigCard(
            icon=FluentIcon.FOLDER,
            title=self.tr("Specify QQ path"),
            content=it(PathFunc).getQQPath(),
        )

    def __setLayout(self):
        """
        控件布局
        """
        self.cardList = [
            self.botNameCard, self.botQQIdCard, self.httpCard,
            self.httpReportCard, self.wsCard, self.wsReverseCard,
            self.messageFormatCard, self.reportSelfMessageCard,
            self.debugModeCard, self.localFile2UrlCard, self.heartIntervalCard,
            self.accessTokenCard, self.QQPathCard
        ]
        self.botGroupCardList = [
            self.botNameCard, self.botQQIdCard, self.httpCard,
            self.httpReportCard, self.wsCard, self.wsReverseCard,
            self.messageFormatCard, self.reportSelfMessageCard,
            self.debugModeCard, self.localFile2UrlCard, self.heartIntervalCard,
            self.accessTokenCard
        ]
        self.advancedGroupCardList = [
            self.QQPathCard
        ]
        # 将卡片添加到组
        for card in self.botGroupCardList:
            self.botGroup.addSettingCard(card)

        for card in self.advancedGroupCardList:
            self.advancedGroup.addSettingCard(card)

        # 添加到布局
        self.expandLayout.addWidget(self.botGroup)
        self.expandLayout.addWidget(self.advancedGroup)
        self.expandLayout.setContentsMargins(30, 0, 40, 10)

        self.viewLayout.setSpacing(0)
        self.viewLayout.addWidget(self.topCard)
        self.viewLayout.addSpacing(5)
        self.viewLayout.addLayout(self.expandLayout)

        self.view.setLayout(self.viewLayout)


class AddWidgetClassCreator(AbstractCreator, ABC):
    # 定义类方法targets，该方法返回一个元组，元组中包含了一个CreateTargetInfo对象，
    # 该对象描述了创建目标的相关信息，包括应用程序名称和类名。
    targets = (CreateTargetInfo("src.Ui.AddPage.Add", "AddWidget"),)

    # 静态方法available()，用于检查模块"Add"是否存在，返回值为布尔型。
    @staticmethod
    def available() -> bool:
        return exists_module("src.Ui.AddPage.Add")

    # 静态方法create()，用于创建AddWidget类的实例，返回值为AddWidget对象。
    @staticmethod
    def create(create_type: [AddWidget]) -> AddWidget:
        return AddWidget()


add_creator(AddWidgetClassCreator)
