# -*- coding: utf-8 -*-

from qfluentwidgets.common import FluentIcon
from qfluentwidgets.components import (
    SwitchButton,
    BodyLabel,
    LineEdit,
    IndicatorPosition,
)

from src.Ui.AddPage.Card.BaseClass import GroupCardBase


class WsConfigCard(GroupCardBase):

    def __init__(self, parent=None) -> None:
        super().__init__(
            icon=FluentIcon.SCROLL,
            title=self.tr("WebSocket service"),
            content=self.tr("Configure WebSocket service"),
            parent=parent,
        )

        # 正向 Ws 服务开关
        self.wsServiceLabel = BodyLabel(self.tr("Enable WebSocket service"))
        self.wsServiceButton = SwitchButton(self, IndicatorPosition.RIGHT)

        # 正向 Ws 服务 端口
        self.wsPortLabel = BodyLabel(self.tr("Set WebSocket Port"))
        self.wsPortLineEdit = LineEdit()
        self.wsPortLineEdit.setPlaceholderText("3001")

        # 添加到设置卡
        self.add(self.wsServiceLabel, self.wsServiceButton)
        self.add(self.wsPortLabel, self.wsPortLineEdit)

    def getValue(self) -> dict:
        return {
            "enable": self.wsServiceButton.isChecked(),
            "port": self.wsPortLineEdit.text(),
        }

    def clear(self):
        self.wsServiceButton.setChecked(False),
        self.wsPortLineEdit.clear()
