# -*- coding: utf-8 -*-
from typing import TYPE_CHECKING

from PySide6.QtWidgets import QWidget
from creart import it
from qfluentwidgets import ExpandLayout, FluentIcon

from src.Core.PathFunc import PathFunc
from src.Ui.AddPage.Card import (
    SwitchConfigCard,
    FolderConfigCard,
    ComboBoxConfigCard,
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
            parent=self,
        )
        self.startScriptPathCard = FolderConfigCard(
            icon=FluentIcon.FOLDER,
            title=self.tr("Specifies the path created by the script"),
            content=str(it(PathFunc).getStartScriptPath()),
            parent=self,
        )
        self.ffmpegPathCard = FolderConfigCard(
            icon=FluentIcon.MUSIC_FOLDER,
            title=self.tr("Specifies ffmpeg path"),
            parent=self,
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
        self.fillLogCard = SwitchConfigCard(
            icon=FluentIcon.SAVE_AS,
            title=self.tr("Whether to enable file logging"),
            content=self.tr("Log to a file(It is off by default)"),
            parent=self,
        )
        self.consoleLogCard = SwitchConfigCard(
            icon=FluentIcon.COMMAND_PROMPT,
            title=self.tr("Whether to enable console logging"),
            content=self.tr("Log to a console(It is off by default)"),
            parent=self,
        )
        self.fileLogLevelCard = ComboBoxConfigCard(
            icon=FluentIcon.EMOJI_TAB_SYMBOLS,
            title=self.tr("File log level"),
            content=self.tr("Set the log level when the output file is output (default debug)"),
            texts=['debug', 'info', 'error'],
            parent=self,
        )
        self.consoleLevelCard = ComboBoxConfigCard(
            icon=FluentIcon.EMOJI_TAB_SYMBOLS,
            title=self.tr("Console log level"),
            content=self.tr("Setting the Console Output Log Level (Default Info)"),
            texts=['info', 'debug', 'error'],
            parent=self,
        )

        self.cards = [
            self.QQPathCard,
            self.startScriptPathCard,
            self.ffmpegPathCard,
            self.debugModeCard,
            self.localFile2UrlCard,
            self.fillLogCard,
            self.consoleLogCard,
            self.fileLogLevelCard,
            self.consoleLevelCard,
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
            "startScriptPath": self.startScriptPathCard.getValue(),
            "ffmpegPath": self.ffmpegPathCard.getValue(),
            "debug": self.debugModeCard.getValue(),
            "localFile2url": self.localFile2UrlCard.getValue(),
            "fileLog": self.fillLogCard.getValue(),
            "consoleLog": self.consoleLogCard.getValue(),
            "fileLogLevel": self.fileLogLevelCard.getValue(),
            "consoleLogLevel": self.consoleLevelCard.getValue(),
        }

    def clearValues(self) -> None:
        """
        ## 清空(还原)内部卡片的配置
        """
        for card in self.cards:
            card.clear()

    def adjustSize(self):
        h = self.cardLayout.heightForWidth(self.width()) + 46
        return self.resize(self.width(), h)
