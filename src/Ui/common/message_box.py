# -*- coding: utf-8 -*-
"""
## 弹出消息框创建
    本模块的目的是为了统一程序内消息框的创建, 从而保持代码的简洁

"""
from typing import TYPE_CHECKING
from PySide6.QtCore import Qt, QEvent, Signal
from PySide6.QtGui import QColor
from qfluentwidgets import MessageBoxBase, TitleLabel, CaptionLabel, LineEdit
from qfluentwidgets.components.dialog_box.dialog import MaskDialogBase, Ui_MessageBox

if TYPE_CHECKING:
    from src.Ui.MainWindow.Window import MainWindow


class TextInputBox(MessageBoxBase):
    """文本输入框"""

    def __init__(self, parent: "MainWindow", placeholder_text: str = "Enter...") -> None:
        """
        ## 初始化类, 创建必要控件

        ## 参数
            - placeholder_text LineEdit的占位符

        """
        super().__init__(parent=parent)
        # 创建控件
        self.titleLabel = TitleLabel(self.tr("请输入..."), self)
        self.urlLineEdit = LineEdit(self)

        # 设置属性
        self.urlLineEdit.setPlaceholderText(placeholder_text)
        self.urlLineEdit.setClearButtonEnabled(True)
        self.widget.setFixedSize(420, 230)

        # 将组件添加到布局
        self.viewLayout.addWidget(self.titleLabel)
        self.viewLayout.addWidget(self.urlLineEdit)


class AskBox(MessageBoxBase):
    """询问用户是否确认提示框"""

    yesSignal = Signal()
    cancelSignal = Signal()

    def __init__(self, parent: "MainWindow", title: str, content: str):
        """初始化类, 创建必要控件"""
        super().__init__(parent=parent)
        # 创建控件
        self.titleLabel = TitleLabel(title, self)
        self.contentLabel = CaptionLabel(content, self)

        # 设置属性
        self.widget.setFixedSize(420, 230)

        # 将组件添加到布局
        self.viewLayout.addWidget(self.titleLabel)
        self.viewLayout.addWidget(self.contentLabel)

        # 连接信号
        self.yesButton.clicked.connect(self.yesSignal.emit)
        self.cancelButton.clicked.connect(self.cancelSignal.emit)
