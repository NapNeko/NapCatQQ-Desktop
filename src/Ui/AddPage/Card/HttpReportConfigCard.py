# -*- coding: utf-8 -*-

from qfluentwidgets.common import FluentIcon

from src.Ui.AddPage.Card.BaseClass import GroupCardBase
from src.Ui.AddPage.Card.Item import LineEditItem, SwitchItem


class HttpReportConfigCard(GroupCardBase):

    def __init__(self, parent=None) -> None:
        super().__init__(
            icon=FluentIcon.SCROLL,
            title=self.tr("HTTP report"),
            content=self.tr("Configure HTTP reporting related services"),
            parent=parent,
        )

        # http 上报服务
        self.ReportEnableItem = SwitchItem(
            self.tr("Enable HTTP reporting service"), self
        )
        # http 上报心跳
        self.HeartEnableItem = SwitchItem(self.tr("Enable HTTP reporting heart"), self)
        # http 上报 token
        self.ReportTokenItem = LineEditItem(
            self.tr("Set HTTP reporting token"), "Can be empty", self
        )

        # 添加 Item
        self.addItem(self.ReportEnableItem)
        self.addItem(self.HeartEnableItem)
        self.addItem(self.ReportTokenItem)

    def getValue(self) -> dict:
        return {
            "enable": self.ReportEnableItem.getValue(),
            "enableHeart": self.HeartEnableItem.getValue(),
            "token": self.ReportTokenItem.getValue(),
        }

    def clear(self) -> None:
        self.ReportEnableItem.clear()
        self.HeartEnableItem.clear()
        self.ReportTokenItem.clear()
