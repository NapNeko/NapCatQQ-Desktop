# -*- coding: utf-8 -*-
# 第三方库导入
from qfluentwidgets import IndicatorPosition, LineEdit, SwitchButton
from PySide6.QtWidgets import QWidget

# 项目内模块导入
from src.ui.components.input_card.base import ItemBase


class SwitchItem(ItemBase):
    """开关Item"""

    def __init__(self, title: str, parent: QWidget | None = None) -> None:
        """初始化 Item

        Args:
            title (str): Item 内容
            parent (QWidget, optional): 父控件. Defaults to None.
        """
        super().__init__(title, parent=parent)
        self.button = SwitchButton(self, IndicatorPosition.RIGHT)

        self._set_layout(self.button)

    def fill_value(self, value: bool) -> None:
        self.button.setChecked(value)

    def get_value(self) -> bool:
        return self.button.isChecked()

    def clear(self) -> None:
        self.button.setChecked(False)


class LineEditItem(ItemBase):
    """单行输入框Item"""

    def __init__(self, title: str, placeholders: str, parent: QWidget | None = None) -> None:
        """初始化 Item

        Args:
            title (str): Item 内容
            placeholders (str): 输入框占位符
            parent (QWidget, optional): 父控件. Defaults to None.
        """
        super().__init__(title, parent=parent)
        self.lineEdit = LineEdit(self)
        self.lineEdit.setFixedWidth(200)
        self.lineEdit.setPlaceholderText(placeholders)
        self.lineEdit.setClearButtonEnabled(True)

        self._set_layout(self.lineEdit)

    def fill_value(self, value: str | int) -> None:
        self.lineEdit.setText(str(value))

    def get_value(self) -> str:
        return self.lineEdit.text()

    def clear(self) -> None:
        self.lineEdit.clear()
