# -*- coding: utf-8 -*-

# 标准库导入
from types import CodeType

# 第三方库导入
from annotated_types import T
from qfluentwidgets.common import FluentIcon, FluentIconBase, FluentStyleSheet, isDarkTheme
from qfluentwidgets.components import (
    Flyout,
    ComboBox,
    LineEdit,
    PushButton,
    TableWidget,
    SwitchButton,
    MessageBoxBase,
    IndicatorPosition,
    TransparentPushButton,
    TransparentToolButton,
)
from qfluentwidgets.components.settings import SettingCard
from qfluentwidgets.components.widgets.flyout import FlyoutView
from qfluentwidgets.components.settings.setting_card import SettingIconWidget
from PySide6.QtGui import QColor, QPainter
from PySide6.QtCore import Qt, QSize, QObject, QStandardPaths
from PySide6.QtWidgets import (
    QFrame,
    QLabel,
    QFileDialog,
    QHBoxLayout,
    QHeaderView,
    QSizePolicy,
    QVBoxLayout,
    QTableWidgetItem,
)

# 项目内模块导入
from src.Ui.common.code_editor import JsonEditor


class JsonTemplateEditConfigCard(QFrame):

    TEMPLATE_STRING = [
        {"name": "机器人名称", "string": "${bot_name}", "doc": "显示您在 NCD 中配置的机器人名称"},
        {"name": "机器人QQ号", "string": "${bot_qq_id}", "doc": "显示机器人的 QQ 号"},
        {"name": "当前时间", "string": "${disconnect_time}", "doc": "显示为当前的时间(发件时间)"},
    ]

    def __init__(self, icon: FluentIconBase, title: str, parent: QObject | None = None):
        super().__init__(parent)

        # 创建控件
        self.iconLabel = SettingIconWidget(icon, self)
        self.titleLabel = QLabel(title, self)
        self.jsonTextEdit = JsonEditor(self)
        self.editorFontSizeAddButton = TransparentToolButton(FluentIcon.ADD, self)
        self.editorFontSizeSubButton = TransparentToolButton(FluentIcon.REMOVE, self)
        self.checkButton = TransparentPushButton(FluentIcon.CODE, self.tr("校验 JSON"), self)
        self.insertTemplateButton = TransparentPushButton(FluentIcon.DICTIONARY_ADD, self.tr("插入模板"), self)
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
        self.insertTemplateButton.clicked.connect(self.showTemplateFlyout)

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
        self.hBoxLayout.addWidget(self.insertTemplateButton, 0, Qt.AlignmentFlag.AlignRight)
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

    def showTemplateFlyout(self) -> None:
        """展示模板弹出组件"""
        # 创建视图
        view = FlyoutView(
            title=self.tr("Json 模板"),
            content=self.tr(
                "这是自带的模板字符串以及模板,模板字符串将会被 NCD 自动解析成相应内容插入\n点击表格中的内容即视为执行插入"
            ),
            isClosable=True,
        )

        # 创建相关组件
        table = TableWidget()
        table.setBorderVisible(True)
        table.setBorderRadius(8)
        table.setWordWrap(False)
        table.setRowCount(len(self.TEMPLATE_STRING))
        table.setColumnCount(3)
        table.setHorizontalHeaderLabels(["模板名称", "模板字符串", "介绍"])
        table.verticalHeader().hide()

        # 修改列宽设置 - 前两列根据内容调整，第三列拉伸填充
        table.horizontalHeader().setSectionResizeMode(0, QHeaderView.ResizeMode.ResizeToContents)
        table.horizontalHeader().setSectionResizeMode(1, QHeaderView.ResizeMode.ResizeToContents)
        table.horizontalHeader().setSectionResizeMode(2, QHeaderView.ResizeMode.Stretch)

        table.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)
        table.setMinimumWidth(530)

        # 链接信号
        table.cellClicked.connect(self._on_table_cell_clicked)

        # 填充数据
        for row, item in enumerate(self.TEMPLATE_STRING):
            # 模板名称列
            name_item = QTableWidgetItem(item["name"])
            name_item.setTextAlignment(Qt.AlignCenter)
            table.setItem(row, 0, name_item)

            # 模板字符串列
            string_item = QTableWidgetItem(item["string"])
            string_item.setTextAlignment(Qt.AlignCenter)
            table.setItem(row, 1, string_item)

            # 介绍列
            doc_item = QTableWidgetItem(item["doc"])
            table.setItem(row, 2, doc_item)

        view.addWidget(table)
        view.setMinimumSize(QSize(600, 290))

        # 展示窗口
        widget = Flyout.make(view, self.insertTemplateButton, self)
        view.closed.connect(widget.close)

    def _on_table_cell_clicked(self, row: int, _: int) -> None:
        """列表点击事件处理"""
        cursor = self.jsonTextEdit.textCursor()
        cursor.insertText(self.TEMPLATE_STRING[row]["string"])
        self.jsonTextEdit.setTextCursor(cursor)


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
