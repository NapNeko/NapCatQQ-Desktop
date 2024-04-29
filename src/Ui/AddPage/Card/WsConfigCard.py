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
        # 正向 Ws 服务 端口
        self.wsPortItem = LineEditItem(self.tr("WebSockets Port"), "3001", self)

        # 添加 item
        self.addItem(self.wsEnableItem)
        self.addItem(self.wsPortItem)

    def getValue(self) -> dict:
        return {
            "enable": self.wsEnableItem.getValue(),
            "port": self.wsPortItem.getValue(),
        }

    def clear(self):
        self.wsEnableItem.clear(),
        self.wsPortItem.clear()
