# -*- coding: utf-8 -*-

from qfluentwidgets.common import FluentIcon

from src.Ui.AddPage.Card.BaseClass import GroupCardBase
from src.Ui.AddPage.Card.Item import LineEditItem, SwitchItem


class HttpConfigCardBase(GroupCardBase):

    def __init__(self, parent=None) -> None:
        super().__init__(
            icon=FluentIcon.SCROLL,
            title=self.tr("HTTP service"),
            content=self.tr("Configure HTTP related services"),
            parent=parent,
        )
        self.httpEnableItem = SwitchItem(self.tr("Enable HTTP"), self)
        self.httpPortItem = LineEditItem(self.tr("HTTP Port"), "8080", self)

        self.addItem(self.httpEnableItem)
        self.addItem(self.httpPortItem)

    def getValue(self) -> dict:
        return {
            "enable": self.httpEnableItem.getValue(),
            "port": self.httpPortItem.getValue(),
        }

    def clear(self) -> None:
        self.httpEnableItem.clear()
        self.httpPortItem.clear()
