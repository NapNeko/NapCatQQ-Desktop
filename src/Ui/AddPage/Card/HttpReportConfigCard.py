# -*- coding: utf-8 -*-
from typing import TYPE_CHECKING

from qfluentwidgets.common import FluentIcon
from qfluentwidgets.components import (
    SwitchButton,
    BodyLabel,
    LineEdit,
    IndicatorPosition,
)

from src.Ui.AddPage.Card.BaseClass import GroupCardBase

if TYPE_CHECKING:
    pass


# noinspection PyTypeChecker
class HttpReportConfigCard(GroupCardBase):

    def __init__(self, parent=None) -> None:
        super().__init__(
            icon=FluentIcon.SCROLL,
            title=self.tr("HTTP report"),
            content=self.tr("Configure HTTP reporting related services"),
            parent=parent,
        )

        # http 上报服务开关
        self.httpRpLabel = BodyLabel(self.tr("Enable HTTP reporting service"))
        self.httpRpButton = SwitchButton(self, IndicatorPosition.RIGHT)

        # http 上报心跳开关
        self.httpRpHeartLabel = BodyLabel(self.tr("Enable HTTP reporting heart"))
        self.httpRpHeartButton = SwitchButton(self, IndicatorPosition.RIGHT)

        # http 上报 token
        self.httpRpTokenLabel = BodyLabel(self.tr("Set HTTP reporting token"))
        self.httpRpTokenLineEdit = LineEdit()
        self.httpRpTokenLineEdit.setPlaceholderText(self.tr("Optional filling"))

        # 添加到设置卡
        self.add(self.httpRpLabel, self.httpRpButton)
        self.add(self.httpRpHeartLabel, self.httpRpHeartButton)
        self.add(self.httpRpTokenLabel, self.httpRpTokenLineEdit)

    def getValue(self) -> dict:
        return {
            "enable": self.httpRpButton.isChecked(),
            "enableHeart": self.httpRpHeartButton.isChecked(),
            "token": self.httpRpTokenLineEdit.text(),
        }

    def clear(self) -> None:
        self.httpRpButton.setChecked(False)
        self.httpRpHeartButton.setChecked(False)
        self.httpRpTokenLineEdit.clear()
