# -*- coding: utf-8 -*-

from qfluentwidgets.common import FluentIcon
from qfluentwidgets.components import (
    TransparentPushButton,
)

from src.Ui.AddPage.Card.BaseClass import GroupCardBase


class HttpReportUrlCard(GroupCardBase):

    def __init__(self, parent=None) -> None:
        super().__init__(
            icon=FluentIcon.SCROLL,
            title=self.tr("Set the HTTP reporting URL"),
            content=self.tr("For example: http://127.0.0.1:8080/onebot/v11/http"),
            parent=parent,
        )
        self.addUrlButton = TransparentPushButton()
        self.addUrlButton.setText(self.tr("Add Url"))
        self.addUrlButton.setIcon(FluentIcon.ADD)
        self.card.hBoxLayout.insertWidget(5, self.addUrlButton)
        self.card.hBoxLayout.insertSpacing(6, 6)
        self.card.expandButton.setEnabled(False)

    def addUrlBtnSolt(self): ...
