# -*- coding: utf-8 -*-
from typing import TYPE_CHECKING

from PySide6.QtWidgets import QWidget
from creart import it
from qfluentwidgets import ExpandLayout, FluentIcon

from src.Core.PathFunc import PathFunc
from src.Ui.AddPage.Card import (
    SwitchConfigCard,
    FolderConfigCard,
)

if TYPE_CHECKING:
    from src.Ui.AddPage.Add.AddWidget import AddWidget


class AdvancedWidget(QWidget):
    """
    ## Advance Item 项对应的 QWidget
    """

    def __init__(self, parent: "AddWidget") -> None:
        super().__init__(parent=parent)
        self.setObjectName("AdvanceWidget")
        self.cardLayout = ExpandLayout(self)

        # 调用方法
        self._initWidget()
        self._setLayout()

    def _initWidget(self):
        """
        ## 初始化 QWidget 所需要的控件并配置
        创建 Card
        """
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

        self.cards = [
            self.QQPathCard,
            self.ffmpegPathCard,
            self.debugModeCard,
            self.localFile2UrlCard,
        ]

    def _setLayout(self):
        """
        ## 将 QWidget 内部的 Card 添加到布局中
        """
        self.cardLayout.setContentsMargins(25, 0, 35, 10)
        self.cardLayout.setSpacing(2)
        for card in self.cards:
            self.cardLayout.addWidget(card)
            self.adjustSize()

    def getValue(self) -> dict:
        """
        ## 返回内部卡片的配置结果
        """
        return {
            "QQPath": self.QQPathCard.getValue(),
            "ffmpegPath": self.ffmpegPathCard.getValue(),
            "debug": self.debugModeCard.getValue(),
            "localFile2url": self.localFile2UrlCard.getValue(),
        }

    def adjustSize(self):
        h = self.cardLayout.heightForWidth(self.width()) + 46
        return self.resize(self.width(), h)
