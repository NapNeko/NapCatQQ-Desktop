# -*- coding: utf-8 -*-

"""
添加机器人
"""
from abc import ABC
from typing import TYPE_CHECKING, Self

from PySide6.QtWidgets import QVBoxLayout, QWidget
from creart import add_creator, exists_module, it
from creart.creator import AbstractCreator, CreateTargetInfo
from qfluentwidgets import ScrollArea
from qfluentwidgets.common import FluentIcon
from qfluentwidgets.components import ExpandLayout, SettingCardGroup

from src.Core.PathFunc import PathFunc
from src.Ui.AddPage.Add.ConfigTopCard import ConfigTopCard
from src.Ui.AddPage.Card import (
    ComboBoxConfigCard,
    FolderConfigCard,
    HttpConfigCard,
    HttpReportConfigCard,
    LineEditConfigCard,
    SwitchConfigCard,
    UrlCard,
    WsConfigCard,
)
from src.Ui.Icon import NapCatDesktopIcon
from src.Ui.StyleSheet import StyleSheet

if TYPE_CHECKING:
    from src.Ui.MainWindow import MainWindow


class AddWidget(ScrollArea):

    def __init__(self):
        super().__init__()
        self.topCard: ConfigTopCard = None
        self.viewLayout: QVBoxLayout = None
        self.expandLayout: ExpandLayout = None
        self.view: QWidget = None

    def initialize(self, parent: "MainWindow") -> Self:
        """
        初始化
        """
        # 创建控件
        self.view = QWidget()
        self.expandLayout = ExpandLayout()
        self.viewLayout = QVBoxLayout()
        self.topCard = ConfigTopCard(self)

        # 设置 ScrollArea
        self.setParent(parent)
        self.setObjectName("AddPage")
        self.setWidget(self.view)
        self.setWidgetResizable(True)
        self.setViewportMargins(0, self.topCard.height(), 0, 0)
        self.view.setObjectName("AddView")

        # 调用方法
        self._createConfigCards()
        self._setLayout()

        # 应用样式表
        StyleSheet.ADD_WIDGET.apply(self)

        return self

    def _createConfigCards(self) -> None:
        """
        创建配置卡片
        """
        # 创建组 - 机器人设置
        self.botGroup = SettingCardGroup(title=self.tr("Robot settings"), parent=self.view)
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
            placeholder_text=self.tr("123456"),
        )
        self.httpCard = HttpConfigCard(self)
        self.httpReportCard = HttpReportConfigCard(self)
        self.httpReportUrlCard = UrlCard(
            icon=FluentIcon.SCROLL,
            title=self.tr("Http Report address"),
            content=self.tr("Set the address for reporting HTTP"),
            parent=self,
        )
        self.wsCard = WsConfigCard(self)
        self.wsReverseCard = SwitchConfigCard(
            icon=FluentIcon.SCROLL,
            title=self.tr("Enable WebSocket Reverse"),
            content=self.tr("Enable the reverse web socket service"),
            parent=self,
        )
        self.wsReverseUrlCard = UrlCard(
            icon=FluentIcon.SCROLL,
            title=self.tr("WebSocket Reverse address"),
            content=self.tr("Reverse WebSocket reporting address"),
        )
        self.messageFormatCard = ComboBoxConfigCard(
            icon=FluentIcon.MESSAGE,
            title=self.tr("Message format"),
            content=self.tr("Array is the message group, and string is the cq code string"),
            texts=["array", "string"],
            parent=self,
        )
        self.reportSelfMessageCard = SwitchConfigCard(
            icon=FluentIcon.ROBOT,
            title=self.tr("Report self message"),
            content=self.tr("Whether to report the bot's own message"),
            parent=self,
        )
        self.heartIntervalCard = LineEditConfigCard(
            icon=FluentIcon.HEART,
            title=self.tr("Heart interval"),
            content=self.tr("WebSocket heartbeat interval, in milliseconds"),
            placeholder_text="30000",
            parent=self,
        )
        self.accessTokenCard = LineEditConfigCard(
            icon=FluentIcon.CERTIFICATE,
            title=self.tr("Access Token"),
            content=self.tr("Access Token, can be empty"),
            parent=self,
        )

        # 创建组 - 高级设置
        self.advancedGroup = SettingCardGroup(title=self.tr("Advanced setting"), parent=self.view)
        self.QQPathCard = FolderConfigCard(
            icon=FluentIcon.FOLDER,
            title=self.tr("Specify QQ path"),
            content=str(it(PathFunc).getQQPath()),
        )
        self.ffmpegPathCard = FolderConfigCard(
            icon=FluentIcon.MUSIC_FOLDER,
            title=self.tr("Set the ffmpeg path"),
        )
        self.debugModeCard = SwitchConfigCard(
            icon=FluentIcon.COMMAND_PROMPT,
            title=self.tr("Debug"),
            content=self.tr("The message will carry a raw field, " "which is the original message content"),
            parent=self,
        )
        self.localFile2UrlCard = SwitchConfigCard(
            icon=FluentIcon.SHARE,
            title=self.tr("LocalFile2Url"),
            content=self.tr(
                "If the URL cannot be obtained when calling the get file interface, "
                "use the base 64 field to return the file content"
            ),
            parent=self,
        )

    def _setLayout(self) -> None:
        """
        控件布局
        """
        self.cardList = [
            self.botNameCard,
            self.botQQIdCard,
            self.httpCard,
            self.httpReportCard,
            self.httpReportUrlCard,
            self.wsCard,
            self.wsReverseCard,
            self.wsReverseUrlCard,
            self.messageFormatCard,
            self.reportSelfMessageCard,
            self.debugModeCard,
            self.localFile2UrlCard,
            self.heartIntervalCard,
            self.accessTokenCard,
            self.QQPathCard,
        ]
        self.botGroupCardList = [
            self.botNameCard,
            self.botQQIdCard,
            self.httpCard,
            self.httpReportCard,
            self.httpReportUrlCard,
            self.wsCard,
            self.wsReverseCard,
            self.wsReverseUrlCard,
            self.messageFormatCard,
            self.reportSelfMessageCard,
            self.heartIntervalCard,
            self.accessTokenCard,
        ]
        self.advancedGroupCardList = [
            self.QQPathCard,
            self.ffmpegPathCard,
            self.debugModeCard,
            self.localFile2UrlCard,
        ]
        # 将卡片添加到组
        self.botGroup.addSettingCards(self.botGroupCardList)
        self.advancedGroup.addSettingCards(self.advancedGroupCardList)

        # 添加到布局
        self.expandLayout.addWidget(self.botGroup)
        self.expandLayout.addWidget(self.advancedGroup)
        self.expandLayout.setContentsMargins(30, 0, 40, 10)

        self.viewLayout.addSpacing(5)
        self.viewLayout.addLayout(self.expandLayout)

        self.view.setLayout(self.viewLayout)

    def getConfig(self) -> dict:
        return {
            "bot": {
                "name": self.botNameCard.getValue(),
                "QQID": self.botQQIdCard.getValue(),
                "http": self.httpCard.getValue(),
                "httpReport": self.httpReportCard.getValue(),
                "httpReportUrls": self.httpReportUrlCard.getValue(),
                "ws": self.wsCard.getValue(),
                "wsReverse": self.wsReverseCard.getValue(),
                "wsReverseUrls": self.wsReverseUrlCard.getValue(),
                "msgFormat": self.messageFormatCard.getValue(),
                "reportSelfMsg": self.reportSelfMessageCard.getValue(),
                "heartInterval": self.heartIntervalCard.getValue(),
                "accessToken": self.accessTokenCard.getValue(),
            },
            "advanced": {
                "QQPath": self.QQPathCard.getValue(),
                "ffmpegPath": self.ffmpegPathCard.getValue(),
                "debug": self.debugModeCard.getValue(),
                "localFile2url": self.localFile2UrlCard.getValue(),
            },
        }

    def resizeEvent(self, event) -> None:
        """
        ## 重写缩放事件对 topCard 进行大小调整
        """
        super().resizeEvent(event)
        self.topCard.resize(self.width(), self.topCard.height())


class AddWidgetClassCreator(AbstractCreator, ABC):
    # 定义类方法targets，该方法返回一个元组，元组中包含了一个CreateTargetInfo对象，
    # 该对象描述了创建目标的相关信息，包括应用程序名称和类名。
    targets = (CreateTargetInfo("src.Ui.AddPage.Add.AddWidget", "AddWidget"),)

    # 静态方法available()，用于检查模块"Add"是否存在，返回值为布尔型。
    @staticmethod
    def available() -> bool:
        return exists_module("src.Ui.AddPage.Add.AddWidget")

    # 静态方法create()，用于创建AddWidget类的实例，返回值为AddWidget对象。
    @staticmethod
    def create(create_type: [AddWidget]) -> AddWidget:
        return AddWidget()


add_creator(AddWidgetClassCreator)
