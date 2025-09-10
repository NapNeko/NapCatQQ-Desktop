# -*- coding: utf-8 -*-
from PySide6.QtCore import QObject, Slot

# 项目内模块导入
from src.ui.page.add_page.enum import ConnectType
from src.ui.page.add_page.msg_box import ChooseConfigTypeDialog as OldChooseConfigTypeDialog
from src.ui.page.bot_list_page.signal_bus import bot_list_page_signal_bus


class ChooseConfigTypeDialog(OldChooseConfigTypeDialog):

    def __init__(self, parent: QObject) -> None:
        super().__init__(parent=parent)

    def accept(self) -> None:

        if self.validate():
            self.accept()

        if (id := self.button_group.checkedId()) != -1:
            bot_list_page_signal_bus.choose_connect_type_signal.emit(list(ConnectType)[id])
