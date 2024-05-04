# -*- coding: utf-8 -*-
from typing import TYPE_CHECKING

from PySide6.QtCore import Qt
from PySide6.QtWidgets import QWidget
from qfluentwidgets import ExpandLayout, FluentIcon, ScrollArea

from src.Ui.AddPage.Card import (
    HttpConfigCard,
    HttpReportConfigCard,
    SwitchConfigCard,
    UrlCard,
    WsConfigCard,
)

if TYPE_CHECKING:
    from src.Ui.AddPage.Add.AddWidget import AddWidget


class ConnectWidget(ScrollArea):
    """
    ## Connect Item 项对应的 QWidget
    """

    def __init__(self, parent: "AddWidget") -> None:
        super().__init__(parent=parent)
        self.setObjectName("ConnectWidget")
        self.view = QWidget()
        self.cardLayout = ExpandLayout(self)

        # 设置 ScrollArea
        self.setWidget(self.view)
        self.setWidgetResizable(True)
        self.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        self.view.setObjectName("ConnectWidgetView")

        # 调用方法
        self._initWidget()
        self._setLayout()

    def _initWidget(self):
        """
        ## 初始化 QWidget 所需要的控件并配置
        创建 Card
        """
        self.httpCard = HttpConfigCard(self.view)
        self.httpReportCard = HttpReportConfigCard(self.view)
        self.httpReportUrlCard = UrlCard(
            icon=FluentIcon.SCROLL,
            title=self.tr("Http Report address"),
            content=self.tr("Set the address for reporting HTTP"),
            parent=self.view,
        )
        self.wsCard = WsConfigCard(self.view)
        self.wsReverseCard = SwitchConfigCard(
            icon=FluentIcon.SCROLL,
            title=self.tr("Enable WebSocket Reverse"),
            content=self.tr("Enable the reverse web socket service"),
            parent=self.view,
        )
        self.wsReverseUrlCard = UrlCard(
            icon=FluentIcon.SCROLL,
            title=self.tr("WebSocket Reverse address"),
            content=self.tr("Reverse WebSocket reporting address"),
            parent=self.view,
        )

        # 当 HTTP 地址添加时, 自动展开 httpReportCard 并将 ReportEnableItem 设置为 True
        # 当 HTTP 地址被删除至空时, 自动关闭并设置为 False
        self.httpReportUrlCard.addSignal.connect(
            lambda: self.httpReportCard.setExpand(True)
        )
        self.httpReportUrlCard.addSignal.connect(
            lambda: self.httpReportCard.ReportEnableItem.button.setChecked(True)
        )
        self.httpReportUrlCard.emptiedSignal.connect(
            lambda: self.httpReportCard.setExpand(False)
        )
        self.httpReportUrlCard.emptiedSignal.connect(
            lambda: self.httpReportCard.ReportEnableItem.button.setChecked(False)
        )

        # 当 反向WS 地址添加时, 自动将 wsReverseCard 设置为 True
        # 当 反向WS 地址被删除至空时, 设置为 False
        self.wsReverseUrlCard.addSignal.connect(
            lambda: self.wsReverseCard.switchButton.setChecked(True)
        )
        self.wsReverseUrlCard.emptiedSignal.connect(
            lambda: self.wsReverseCard.switchButton.setChecked(False)
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
