# -*- coding: utf-8 -*-

from qfluentwidgets.common import FluentIcon
from qfluentwidgets.components import (
    SwitchButton,
    BodyLabel,
    LineEdit,
    IndicatorPosition,
)

from src.Ui.AddPage.Card.BaseClass import GroupCardBase


class WsReverseConfigCard(GroupCardBase):

    def __init__(self, parent=None) -> None:
        super().__init__(
            icon=FluentIcon.SCROLL,
            title=self.tr("WebSocketReverse service"),
            content=self.tr("Configure WebSocketReverse service"),
            parent=parent,
        )

        # 反向 Ws 开关
        self.wsReServiceLabel = BodyLabel(self.tr("Enable WebSocketReverse service"))
        self.wsReServiceButton = SwitchButton(self, IndicatorPosition.RIGHT)

        # 反向 Ws Ip
        self.wsReIpLabel = BodyLabel(self.tr("Set WebSocketReverse Ip"))
        self.wsReIpLineEdit = LineEdit()
        self.wsReIpLineEdit.setPlaceholderText("127.0.0.1")

        # 反向 Ws 端口
        self.wsRePortLabel = BodyLabel(self.tr("Set WebSocketReverse port"))
        self.wsRePortLineEdit = LineEdit()
        self.wsRePortLineEdit.setPlaceholderText("8080")

        # 反向 Ws 地址
        self.wsRePathLabel = BodyLabel(self.tr("Set WebSocketReverse path"))
        self.wsRePathLineEdit = LineEdit()
        self.wsRePathLineEdit.setPlaceholderText("/onebot/v11/ws")

        # 添加到设置卡
        self.add(self.wsReServiceLabel, self.wsReServiceButton)
        self.add(self.wsReIpLabel, self.wsReIpLineEdit)
        self.add(self.wsRePortLabel, self.wsRePortLineEdit)
        self.add(self.wsRePathLabel, self.wsRePathLineEdit)

    def getValue(self):
        return {
            "enable": self.wsReServiceButton.isChecked(),
            "ip": self.wsReIpLineEdit.text(),
            "port": self.wsRePortLineEdit.text(),
            "path": self.wsRePathLineEdit.text(),
        }

    def clear(self) -> None:
        self.wsReServiceButton.setChecked(False)
        self.wsReIpLineEdit.clear()
        self.wsRePortLineEdit.clear()
        self.wsReIpLineEdit.clear()
