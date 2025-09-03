# -*- coding: utf-8 -*-
from PySide6.QtCore import Slot, QObject

# 项目内模块导入
from src.ui.page.add_page.enum import ConnectType
from src.ui.page.add_page.msg_box import ChooseConfigTypeDialog as OldChooseConfigTypeDialog
from src.ui.page.bot_list_page.signal_bus import botListPageSignalBus


class ChooseConfigTypeDialog(OldChooseConfigTypeDialog):

    def __init__(self, parent: QObject) -> None:
        super().__init__(parent=parent)

        self.yesButton.clicked.disconnect()
        self.yesButton.clicked.connect(self._onYesButtonClicked)

    @Slot()
    def _onYesButtonClicked(self) -> None:
        """Yes 按钮槽函数"""

        if self.validate():
            self.accept()

        if (id := self.buttonGroup.checkedId()) != -1:
            botListPageSignalBus.chooseConnectType.emit(list(ConnectType)[id])
