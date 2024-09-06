# -*- coding: utf-8 -*-

from qfluentwidgets.common import FluentIcon

from src.Core.Config.ConfigModel import WsConfig
from src.Ui.common.InputCard.Item import SwitchItem, LineEditItem
from src.Ui.common.InputCard.BaseClass import GroupCardBase


class WsConfigCard(GroupCardBase):
    """
    ## 正向 Websocket 服务配置
    """

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
        self.wsHostItem = LineEditItem(self.tr("WebSockets Host"), "Listens for all host for null", self)
        # 正向 Ws 服务端口
        self.wsPortItem = LineEditItem(self.tr("WebSockets Port"), "3001", self)

        # 添加 item
        self.addItem(self.wsEnableItem)
        self.addItem(self.wsHostItem)
        self.addItem(self.wsPortItem)

    def fillValue(self, values: WsConfig) -> None:
        self.wsEnableItem.fillValue(values.enable)
        self.wsHostItem.fillValue(values.host)
        self.wsPortItem.fillValue(values.port)

    def getValue(self) -> dict:
        return {
            "enable": self.wsEnableItem.getValue(),
            "host": self.wsHostItem.getValue(),
            "port": int(self.wsPortItem.getValue()) if self.wsPortItem.getValue() else 0,
        }

    def clear(self) -> None:
        self.wsEnableItem.clear()
        self.wsHostItem.clear()
        self.wsPortItem.clear()
