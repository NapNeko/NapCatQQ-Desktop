# -*- coding: utf-8 -*-

from qfluentwidgets.common import FluentIcon
from qfluentwidgets.components import (
    SwitchButton,
    BodyLabel,
    LineEdit,
    IndicatorPosition,
)

from src.Ui.AddPage.Card.BaseClass import GroupCardBase


class HttpConfigCard(GroupCardBase):

    def __init__(self, parent=None) -> None:
        super().__init__(
            icon=FluentIcon.SCROLL,
            title=self.tr("HTTP service"),
            content=self.tr("Configure HTTP related services"),
            parent=parent,
        )
        # http服务开关
        self.httpServiceLabel = BodyLabel(self.tr("Enable HTTP service"))
        self.httpServiceButton = SwitchButton(self, IndicatorPosition.RIGHT)

        # http服务端口
        self.httpPortLabel = BodyLabel(self.tr("HTTP service port"))
        self.httpPortLineEdit = LineEdit()
        self.httpPortLineEdit.setPlaceholderText("8080")

        # 添加到设置卡
        self.add(self.httpServiceLabel, self.httpServiceButton)
        self.add(self.httpPortLabel, self.httpPortLineEdit)

    def getValue(self) -> dict:
        return {
            "enable": self.httpServiceButton.isChecked(),
            "port": self.httpPortLineEdit.text(),
        }

    def clear(self) -> None:
        self.httpServiceButton.setChecked(False)
        self.httpPortLineEdit.clear()
