# -*- coding: utf-8 -*-
from qfluentwidgets import LineEdit, SwitchButton, IndicatorPosition

from src.Ui.common.InputCard.BaseClass import ItemBase


class SwitchItem(ItemBase):
    """
    实现 开关Item
    """

    def __init__(self, title: str, parent=None) -> None:
        """
        ## 初始化 Item

        ### 参数
            - title: Item 内容
        """
        super().__init__(title, parent=parent)
        self.button = SwitchButton(self, IndicatorPosition.RIGHT)

        self._setLayout(self.button)

    def fillValue(self, value: bool) -> None:
        self.button.setChecked(value)

    def getValue(self) -> bool:
        return self.button.isChecked()

    def clear(self) -> None:
        self.button.setChecked(False)


class LineEditItem(ItemBase):
    """
    实现 单行输入框Item
    """

    def __init__(self, title: str, placeholders: str, parent=None) -> None:
        """
        ## 初始化 Item

        ### 参数
            - title: Item 内容
        """
        super().__init__(title, parent=parent)
        self.lineEdit = LineEdit(self)
        self.lineEdit.setFixedWidth(200)
        self.lineEdit.setPlaceholderText(placeholders)
        self.lineEdit.setClearButtonEnabled(True)

        self._setLayout(self.lineEdit)

    def fillValue(self, value: str | int) -> None:
        self.lineEdit.setText(value)

    def getValue(self) -> str:
        return self.lineEdit.text()

    def clear(self) -> None:
        self.lineEdit.clear()
