# -*- coding: utf-8 -*-

from qfluentwidgets.common import FluentIcon

from src.Core.Config.ConfigModel import HttpConfig
from src.Ui.common.InputCard.Item import SwitchItem, LineEditItem
from src.Ui.common.InputCard.BaseClass import GroupCardBase


class HttpConfigCard(GroupCardBase):
    """
    ## HTTP 配置卡片, 包含 HTTP 服务配置, HTTP 上报配置
    """

    def __init__(self, parent=None) -> None:
        super().__init__(
            icon=FluentIcon.SCROLL,
            title=self.tr("HTTP 配置"),
            content=self.tr("配置 HTTP 服务和上报"),
            parent=parent,
        )
        # HTTP 服务配置项
        self.httpEnableItem = SwitchItem(self.tr("启用 HTTP 服务"), self)
        self.httpHostItem = LineEditItem(self.tr("HTTP 主机"), self.tr("为空则监听所有地址"), self)
        self.httpPortItem = LineEditItem(self.tr("HTTP 服务端口"), "3000", self)

        # HTTP 上报配置项
        self.httpSecretItem = LineEditItem(self.tr("HTTP 上报密钥"), self.tr("可为空"), self)
        self.httpEnableHeart = SwitchItem(self.tr("是否启用http心跳"), self)
        self.httpEnablePost = SwitchItem(self.tr("是否启用http上报服务"), self)

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
            "port": int(self.httpPortItem.getValue()) if self.httpPortItem.getValue() else 0,
            "secret": self.httpSecretItem.getValue(),
            "enableHeart": self.httpEnableHeart.getValue(),
            "enablePost": self.httpEnablePost.getValue(),
        }

    def clear(self) -> None:
        self.httpEnableItem.clear()
        self.httpHostItem.clear()
        self.httpPortItem.clear()
        self.httpSecretItem.clear()
        self.httpEnableHeart.clear()
        self.httpEnablePost.clear()
