# -*- coding: utf-8 -*-

from qfluentwidgets.common import FluentIcon

from src.Ui.AddPage.Card.BaseClass import GroupCardBase
from src.Ui.AddPage.Card.Item import LineEditItem, SwitchItem

from src.Core.Config.ConfigModel import HttpConfig


class HttpConfigCard(GroupCardBase):
    """
    ## HTTP 配置卡片, 包含 HTTP 服务配置, HTTP 上报配置
    """

    def __init__(self, parent=None) -> None:
        super().__init__(
            icon=FluentIcon.SCROLL,
            title=self.tr("HTTP Config"),
            content=self.tr("Configure HTTP services and reporting"),
            parent=parent,
        )
        # HTTP 服务配置项
        self.httpEnableItem = SwitchItem(self.tr("Enable the HTTP service"), self)
        self.httpHostItem = LineEditItem(
            self.tr("HTTP Host"), self.tr("Listens for all host for null"), self
        )
        self.httpPortItem = LineEditItem(
            self.tr("HTTP Port"), "8080", self
        )

        # HTTP 上报配置项
        self.httpSecretItem = LineEditItem(
            self.tr("HTTP Secret"), self.tr("Can be empty"), self
        )
        self.httpEnableHeart = SwitchItem(self.tr("Enable HTTP heartbeats"), self)
        self.httpEnablePost = SwitchItem(self.tr("Enable HTTP Post"), self)

        self.addItem(self.httpEnableItem)
        self.addItem(self.httpHostItem)
        self.addItem(self.httpPortItem)
        self.addItem(self.httpSecretItem)
        self.addItem(self.httpEnableHeart)
        self.addItem(self.httpEnablePost)

    def fillValue(self, values: HttpConfig) -> None:
        self.httpEnableItem.fillValue(values.enable)
        self.httpHostItem.fillValue(values.host)
        self.httpPortItem.fillValue(values.port)
        self.httpSecretItem.fillValue(values.secret)
        self.httpEnableHeart.fillValue(values.enableHeart)
        self.httpEnablePost.fillValue(values.enablePost)

    def getValue(self) -> dict:
        return {
            "enable": self.httpEnableItem.getValue(),
            "host": self.httpHostItem.getValue(),
            "port": self.httpPortItem.getValue(),
            "secret": self.httpSecretItem.getValue(),
            "enableHeart": self.httpEnableHeart.getValue(),
            "enablePost": self.httpEnablePost.getValue()
        }

    def clear(self) -> None:
        self.httpEnableItem.clear()
        self.httpHostItem.clear()
        self.httpPortItem.clear()
        self.httpSecretItem.clear()
        self.httpEnableHeart.clear()
        self.httpEnablePost.clear()
