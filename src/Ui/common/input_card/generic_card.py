# -*- coding: utf-8 -*-

# 第三方库导入
from qfluentwidgets.common import FluentIcon, FluentIconBase, FluentStyleSheet, isDarkTheme
from qfluentwidgets.components import (
    ComboBox,
    LineEdit,
    PushButton,
    SwitchButton,
    MessageBoxBase,
    IndicatorPosition,
    TransparentPushButton,
    TransparentToolButton,
)
from qfluentwidgets.components.settings import SettingCard
from qfluentwidgets.components.settings.setting_card import SettingIconWidget
from PySide6.QtGui import QColor, QPainter
from PySide6.QtCore import Qt, QObject, QStandardPaths
from PySide6.QtWidgets import QFrame, QLabel, QFileDialog, QHBoxLayout, QVBoxLayout

# 项目内模块导入
from src.Ui.common.code_editor import JsonEditor


class TemplateEditConfigCard(QFrame):

    def __init__(self, icon: FluentIconBase, title: str, parent: QObject | None = None):
        super().__init__(parent)

        # 创建控件
        self.iconLabel = SettingIconWidget(icon, self)
        self.titleLabel = QLabel(title, self)
        self.jsonTextEdit = JsonEditor(self)
        self.editorFontSizeAddButton = TransparentToolButton(FluentIcon.ADD, self)
        self.editorFontSizeSubButton = TransparentToolButton(FluentIcon.REMOVE, self)
        self.checkButton = TransparentPushButton(FluentIcon.CODE, self.tr("校验 JSON"), self)
        self.vBoxLayout = QVBoxLayout(self)
        self.hBoxLayout = QHBoxLayout()

        # 设置属性
        self.setMinimumHeight(300)
        self.setMinimumWidth(400)
        self.iconLabel.setFixedSize(16, 16)
        self.jsonTextEdit.setReadOnly(False)

        # 信号与槽
        self.editorFontSizeAddButton.clicked.connect(
            lambda: self.jsonTextEdit.setFontSize(self.jsonTextEdit.font_size + 1)
        )
        self.editorFontSizeSubButton.clicked.connect(
            lambda: self.jsonTextEdit.setFontSize(self.jsonTextEdit.font_size - 1)
        )
        self.checkButton.clicked.connect(self.jsonTextEdit.checkJson)

        # 布局
        self.hBoxLayout.setSpacing(0)
        self.hBoxLayout.setContentsMargins(0, 0, 0, 0)
        self.hBoxLayout.setAlignment(Qt.AlignmentFlag.AlignVCenter)

        self.vBoxLayout.setSpacing(0)
        self.vBoxLayout.setContentsMargins(16, 16, 16, 16)
        self.vBoxLayout.setAlignment(Qt.AlignmentFlag.AlignTop)

        self.hBoxLayout.addWidget(self.iconLabel, 0, Qt.AlignmentFlag.AlignLeft)
        self.hBoxLayout.addSpacing(16)
        self.hBoxLayout.addWidget(self.titleLabel, 0, Qt.AlignmentFlag.AlignLeft)
        self.hBoxLayout.addSpacing(16)
        self.hBoxLayout.addStretch(1)
        self.hBoxLayout.addWidget(self.editorFontSizeSubButton, 0, Qt.AlignmentFlag.AlignRight)
        self.hBoxLayout.addWidget(self.editorFontSizeAddButton, 0, Qt.AlignmentFlag.AlignRight)
        self.hBoxLayout.addWidget(self.checkButton, 0, Qt.AlignmentFlag.AlignRight)
        self.hBoxLayout.addSpacing(8)

        self.vBoxLayout.addLayout(self.hBoxLayout)
        self.vBoxLayout.addSpacing(8)
        self.vBoxLayout.addWidget(self.jsonTextEdit)

        FluentStyleSheet.SETTING_CARD.apply(self)

    def paintEvent(self, e):
        painter = QPainter(self)
        painter.setRenderHints(QPainter.Antialiasing)

        if isDarkTheme():
            painter.setBrush(QColor(255, 255, 255, 13))
            painter.setPen(QColor(0, 0, 0, 50))
        else:
            painter.setBrush(QColor(255, 255, 255, 170))
            painter.setPen(QColor(0, 0, 0, 19))

        painter.drawRoundedRect(self.rect().adjusted(1, 1, -1, -1), 6, 6)

    def fillValue(self, value: str) -> None:
        self.jsonTextEdit.setJson(str(value))

    def getValue(self) -> str:
        return self.jsonTextEdit.getJson()

    def clear(self) -> None:
        self.jsonTextEdit.clear()


class LineEditConfigCard(SettingCard):

    def __init__(self, icon: FluentIconBase, title: str, placeholder_text="", content=None, parent=None) -> None:
        super().__init__(icon, title, content, parent)
        self.lineEdit = LineEdit(self)
        self.lineEdit.setClearButtonEnabled(True)
        self.lineEdit.setPlaceholderText(placeholder_text)
        self.lineEdit.setFixedWidth(165)
        self.hBoxLayout.addWidget(self.lineEdit, 0, Qt.AlignmentFlag.AlignRight)
        self.hBoxLayout.addSpacing(16)

    def fillValue(self, value: str | int) -> None:
        self.lineEdit.setText(str(value))

    def getValue(self) -> str:
        return self.lineEdit.text()

    def clear(self) -> None:
        self.lineEdit.clear()


class ComboBoxConfigCard(SettingCard):

    def __init__(self, icon: FluentIconBase, title: str, texts=None, content=None, parent=None) -> None:
        super().__init__(icon, title, content, parent)
        self.texts = texts or []
        self.comboBox = ComboBox(self)
        self.comboBox.addItems(self.texts)
        self.comboBox.setFixedWidth(165)
        self.hBoxLayout.addWidget(self.comboBox, 0, Qt.AlignmentFlag.AlignRight)
        self.hBoxLayout.addSpacing(16)

    def fillValue(self, value: str) -> None:
        self.comboBox.setCurrentIndex(self.texts.index(value))

    def getValue(self) -> str:
        return self.comboBox.currentText()

    def clear(self) -> None:
        self.comboBox.setCurrentIndex(0)


class SwitchConfigCard(SettingCard):

    def __init__(self, icon: FluentIconBase, title: str, content: str = None, value: bool = False, parent=None) -> None:
        super().__init__(icon, title, content, parent)
        self.switchButton = SwitchButton(self, IndicatorPosition.RIGHT)

        self.fillValue(value)
        self.hBoxLayout.addWidget(self.switchButton, 0, Qt.AlignmentFlag.AlignRight)
        self.hBoxLayout.addSpacing(16)

    def fillValue(self, value: bool) -> None:
        self.switchButton.setChecked(value)

    def getValue(self) -> bool:
        return self.switchButton.isChecked()

    def clear(self) -> None:
        self.switchButton.setChecked(False)


class FolderConfigCard(SettingCard):

    def __init__(self, icon: FluentIconBase, title: str, content=None, parent=None) -> None:
        super().__init__(icon, title, content, parent)
        self.default = content
        self.chooseFolderButton = PushButton(icon=FluentIcon.FOLDER, text=self.tr("Choose Folder"))
        self.chooseFolderButton.clicked.connect(self.chooseFolder)

        self.hBoxLayout.addWidget(self.chooseFolderButton, 0, Qt.AlignmentFlag.AlignRight)
        self.hBoxLayout.addSpacing(16)

    def chooseFolder(self) -> None:
        folder = QFileDialog.getExistingDirectory(
            parent=self,
            caption=self.tr("Choose folder"),
            dir=QStandardPaths.writableLocation(QStandardPaths.StandardLocation.DesktopLocation),
        )
        if folder:
            self.setContent(folder)
            self.setFixedHeight(70)

    def fillValue(self, value: str) -> None:
        self.setContent(value)

    def getValue(self) -> str:
        return self.contentLabel.text()

    def clear(self) -> None:
        self.contentLabel.setText(self.default)


class ShowDialogCard(SettingCard):

    def __init__(self, dialog: MessageBoxBase, icon: FluentIconBase, title: str, content=None, parent=None) -> None:
        super().__init__(icon, title, content, parent)
        self._dialog = dialog
        self.button = TransparentPushButton(FluentIcon.SETTING, self.tr("点我配置"))

        self.button.clicked.connect(self.showDialog)

        self.hBoxLayout.addWidget(self.button, 0, Qt.AlignmentFlag.AlignRight)
        self.hBoxLayout.addSpacing(16)

    def showDialog(self) -> None:
        # 项目内模块导入
        from src.Ui.MainWindow import MainWindow

        self._dialog(MainWindow()).exec()
