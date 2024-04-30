# -*- coding: utf-8 -*-
from typing import TYPE_CHECKING

from creart import it
from PySide6.QtCore import Qt
from PySide6.QtWidgets import QHBoxLayout, QVBoxLayout, QWidget
from qfluentwidgets.common import Action, FluentIcon
from qfluentwidgets.components import (
    CaptionLabel,
    InfoBar,
    InfoBarPosition,
    MessageBox,
    PrimarySplitPushButton,
    PushButton,
    RoundMenu,
    TitleLabel,
)

from src.Core.CreateScript import ScriptType

if TYPE_CHECKING:
    from src.Core.CreateScript import CreateScript
    from src.Ui.AddPage.Add import AddWidget


class ConfigTopCard(QWidget):

    def __init__(self, parent: "AddWidget") -> None:
        super().__init__(parent=parent)
        self.titleLabel = TitleLabel(self.tr("Add bot"))
        self.subtitleLabel = CaptionLabel(
            self.tr("Before adding a robot, you need to do some configuration")
        )
        self.clearConfigButton = PushButton(
            icon=FluentIcon.DELETE, text=self.tr("Clear config")
        )
        self.psPushButton = PrimarySplitPushButton(
            icon=FluentIcon.ADD, text=self.tr("Add to bot list")
        )
        self.menu = RoundMenu()
        self.menu.addAction(
            Action(
                icon=FluentIcon.COMMAND_PROMPT,
                text=self.tr("Create .ps1 script"),
                triggered=self._createPs1ScriptSlot,
            )
        )
        self.menu.addAction(
            Action(
                icon=FluentIcon.COMMAND_PROMPT,
                text=self.tr("Create .bat script"),
                triggered=self._createBatScriptSlot,
            )
        )
        self.menu.addAction(
            Action(
                icon=FluentIcon.COMMAND_PROMPT,
                text=self.tr("Create .sh script"),
                triggered=self._createShScriptSlot,
            )
        )
        self.psPushButton.setFlyout(self.menu)

        self.hBoxLayout = QHBoxLayout()
        self.labelLayout = QVBoxLayout()
        self.buttonLayout = QHBoxLayout()

        self.__setLayout()
        self.__connectSignal()

    def __setLayout(self) -> None:
        self.labelLayout.setSpacing(0)
        self.labelLayout.setContentsMargins(0, 0, 0, 0)
        self.labelLayout.addWidget(self.titleLabel)
        self.labelLayout.addSpacing(1)
        self.labelLayout.addWidget(self.subtitleLabel)

        self.buttonLayout.setSpacing(4)
        self.buttonLayout.setContentsMargins(0, 0, 0, 0)
        self.buttonLayout.addWidget(self.clearConfigButton),
        self.buttonLayout.addSpacing(2)
        self.buttonLayout.addWidget(self.psPushButton)
        self.buttonLayout.setAlignment(
            Qt.AlignmentFlag.AlignBottom | Qt.AlignmentFlag.AlignLeft
        )

        self.hBoxLayout.addLayout(self.labelLayout)
        self.hBoxLayout.addStretch(1)
        self.hBoxLayout.addLayout(self.buttonLayout)
        self.hBoxLayout.addSpacing(20)

        self.setLayout(self.hBoxLayout)

    def __connectSignal(self) -> None:
        self.clearConfigButton.clicked.connect(self.clearBtnSlot)

    def __initCreateScript(self, scriptType) -> "CreateScript":
        from src.Core.CreateScript import CreateScript
        from src.Ui.AddPage.Add import AddWidget

        return CreateScript(
            config=it(AddWidget).getConfig(),
            scriptType=scriptType,
            infoBarParent=self.parent().parent(),
        )

    def _createPs1ScriptSlot(self) -> None:
        create = self.__initCreateScript(ScriptType.PS1)
        create.createPs1Script()

    def _createBatScriptSlot(self) -> None:
        create = self.__initCreateScript(ScriptType.BAT)
        create.createBatScript()

    def _createShScriptSlot(self) -> None:
        create = self.__initCreateScript(ScriptType.SH)
        create.createShScript()

    def clearBtnSlot(self) -> None:
        from src.Ui.AddPage.Add import AddWidget

        msg = MessageBox(
            title=self.tr("Confirm clearing configuration"),
            content=self.tr(
                "After clearing, all configuration items on this page "
                "will be cleared, and this operation cannot be undone"
            ),
            parent=it(AddWidget),
        )

        if msg.exec():
            for card in it(AddWidget).cardList:
                card.clear()
