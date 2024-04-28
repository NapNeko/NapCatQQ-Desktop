# -*- coding: utf-8 -*-
from PySide6.QtCore import Qt, QStandardPaths
from PySide6.QtWidgets import QFileDialog
from qfluentwidgets.common import FluentIconBase, FluentIcon
from qfluentwidgets.components import (
    PushButton,
    SwitchButton,
    ComboBox,
    LineEdit,
    IndicatorPosition,
)
from qfluentwidgets.components.settings import SettingCard


class LineEditConfigCard(SettingCard):

    def __init__(
        self,
        icon: FluentIconBase,
        title: str,
        placeholder_text="",
        content=None,
        parent=None,
    ) -> None:
        super().__init__(icon, title, content, parent)
        self.lineEdit = LineEdit(self)
        self.lineEdit.setClearButtonEnabled(True)
        self.lineEdit.setPlaceholderText(placeholder_text)

        self.hBoxLayout.addWidget(self.lineEdit, 0, Qt.AlignmentFlag.AlignRight)
        self.hBoxLayout.addSpacing(16)

    def getValue(self) -> str:
        return self.lineEdit.text()

    def clear(self) -> None:
        self.lineEdit.clear()


class ComboBoxConfigCard(SettingCard):

    def __init__(
        self, icon: FluentIconBase, title: str, texts=None, content=None, parent=None
    ) -> None:
        super().__init__(icon, title, content, parent)
        self.texts = texts or []
        self.comboBox = ComboBox(self)
        self.comboBox.addItems(self.texts)
        self.hBoxLayout.addWidget(self.comboBox, 0, Qt.AlignmentFlag.AlignRight)
        self.hBoxLayout.addSpacing(16)

    def getValue(self) -> str:
        return self.comboBox.currentText()

    def clear(self) -> None:
        self.comboBox.setCurrentIndex(0)


class SwitchConfigCard(SettingCard):

    def __init__(
        self, icon: FluentIconBase, title: str, content=None, parent=None
    ) -> None:
        super().__init__(icon, title, content, parent)
        self.swichButton = SwitchButton(self, IndicatorPosition.RIGHT)

        self.hBoxLayout.addWidget(self.swichButton, 0, Qt.AlignmentFlag.AlignRight)
        self.hBoxLayout.addSpacing(16)

    def getValue(self) -> bool:
        return self.swichButton.isChecked()

    def clear(self) -> None:
        self.swichButton.setChecked(False)


class FolderConfigCard(SettingCard):

    def __init__(
        self, icon: FluentIconBase, title: str, content=None, parent=None
    ) -> None:
        super().__init__(icon, title, content, parent)
        self.default = content
        self.chooseFolderButton = PushButton(
            icon=FluentIcon.FOLDER, text=self.tr("Choose Folder")
        )
        self.chooseFolderButton.clicked.connect(self.chooseFolder)

        self.hBoxLayout.addWidget(
            self.chooseFolderButton, 0, Qt.AlignmentFlag.AlignRight
        )
        self.hBoxLayout.addSpacing(16)

    def chooseFolder(self) -> None:
        folder = QFileDialog.getExistingDirectory(
            parent=self,
            caption=self.tr("Choose folder"),
            dir=QStandardPaths.writableLocation(
                QStandardPaths.StandardLocation.DesktopLocation
            ),
        )
        if folder:
            self.contentLabel.setText(folder)

    def getValue(self) -> str:
        return self.contentLabel.text()

    def clear(self) -> None:
        self.contentLabel.setText(self.default)
