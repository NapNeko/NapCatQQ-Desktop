# -*- coding: utf-8 -*-


# 第三方库导入
from qfluentwidgets import FluentIcon as FIcon
from qfluentwidgets import SearchLineEdit, MSFluentTitleBar, TransparentToolButton
from PySide6.QtGui import QPixmap
from PySide6.QtCore import Qt, QSize


class TitleBar(MSFluentTitleBar):
    """标题栏"""

    def __init__(self, parent=None) -> None:
        """构造函数"""
        super().__init__(parent)

        # 设置窗口标题以及图标
        self.setIcon(QPixmap(":Global/logo.png"))
        self.setTitle("NapCatQQ Desktop")

        # 添加返回按钮
        self.backButton = TransparentToolButton(FIcon.LEFT_ARROW)
        self.backButton.setIconSize(QSize(12, 12))
        self.hBoxLayout.insertWidget(5, self.backButton, 0, Qt.AlignmentFlag.AlignCenter)

        # 添加前进按钮
        self.forwardButton = TransparentToolButton(FIcon.RIGHT_ARROW)
        self.forwardButton.setIconSize(QSize(12, 12))
        self.forwardButton.setEnabled(False)
        self.hBoxLayout.insertWidget(6, self.forwardButton, 0, Qt.AlignmentFlag.AlignCenter)

        # 添加搜索框
        # TODO 这里只是暂时这样做个示例，后续会根据实际情况进行修改
        self.searchLineEdit = SearchLineEdit()
        self.searchLineEdit.setPlaceholderText(f"{' ' * 25} 🔎 NapCatQQ Doucment")
        self.searchLineEdit.setMinimumSize(400, 30)
        self.hBoxLayout.insertWidget(7, self.searchLineEdit, 0, Qt.AlignmentFlag.AlignCenter)

        # 插入弹簧
        self.hBoxLayout.insertStretch(4, 1)
        self.hBoxLayout.insertStretch(9, 3)
