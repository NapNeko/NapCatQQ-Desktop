# -*- coding: utf-8 -*-
from typing import TYPE_CHECKING

from PySide6.QtWidgets import QWidget
from qfluentwidgets import ExpandLayout, FluentIcon

from src.Ui.AddPage.Card import (
    HttpConfigCard,
    HttpReportConfigCard,
    SwitchConfigCard,
    UrlCard,
    WsConfigCard,
)

if TYPE_CHECKING:
    from src.Ui.AddPage.Add.AddWidget import AddWidget


class ConnectWidget(QWidget):
    """
    ## Connect Item 项对应的 QWidget
    """

    def __init__(self, parent: "AddWidget") -> None:
        super().__init__(parent=parent)
        self.setObjectName("ConnectWidget")
        self.cardLayout = ExpandLayout(self)

        # 调用方法
        self._initWidget()
        self._setLayout()

    def _initWidget(self):
        """
        ## 初始化 QWidget 所需要的控件并配置
        创建 Card
        """
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
            parent=self,
        )

        self.cards = [
            self.httpCard,
            self.httpReportCard,
            self.httpReportUrlCard,
            self.wsCard,
            self.wsReverseCard,
            self.wsReverseUrlCard,
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
            "http": self.httpCard.getValue(),
            "httpReport": self.httpReportCard.getValue(),
            "httpReportUrls": self.httpReportUrlCard.getValue(),
            "ws": self.wsCard.getValue(),
            "wsReverse": self.wsReverseCard.getValue(),
            "wsReverseUrls": self.wsReverseUrlCard.getValue(),
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
