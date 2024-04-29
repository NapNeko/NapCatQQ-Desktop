# -*- coding: utf-8 -*-

from qfluentwidgets.common import FluentIcon

from src.Ui.AddPage.Card.BaseClass import GroupCardBase
from src.Ui.AddPage.Card.Item import LineEditItem, SwitchItem


class WsConfigCard(GroupCardBase):

    def __init__(self, parent=None) -> None:
        super().__init__(
            icon=FluentIcon.SCROLL,
            title=self.tr("WebSocket service"),
            content=self.tr("Configure WebSocket service"),
            parent=parent,
        )

        # 正向 Ws 服务开关
        self.wsEnableItem = SwitchItem(self.tr("Enable WebSockets"), self)
        # 正向 Ws 服务监听 ip/地址
        self.wsAddressesItem = LineEditItem(
            self.tr("WebSockets Address"), "127.0.0.1", self
        )
        # 正向 Ws 服务 端口
        self.wsPortItem = LineEditItem(self.tr("WebSockets Port"), "3001", self)

        # 添加 item
        self.addItem(self.wsEnableItem)
        self.addItem(self.wsAddressesItem)
        self.addItem(self.wsPortItem)

    def getValue(self) -> dict:
        return {
            "enable": self.wsEnableItem.getValue(),
            "addresses": self.wsAddressesItem.getValue(),
            "port": self.wsPortItem.getValue(),
        }

    def clear(self):
        self.wsEnableItem.clear(),
        self.wsAddressesItem.clear(),
        self.wsPortItem.clear()
