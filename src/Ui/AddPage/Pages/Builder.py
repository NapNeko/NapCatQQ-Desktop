# -*- coding: utf-8 -*-
# 标准库导入
from typing import TYPE_CHECKING

# 第三方库导入
from qfluentwidgets import FluentIcon, TitleLabel
from PySide6.QtCore import Qt
from PySide6.QtWidgets import QWidget, QHBoxLayout, QVBoxLayout

# 项目内模块导入
from src.Ui.AddPage.Unit.Card import BuilderAppCard

if TYPE_CHECKING:
    # 项目内模块导入
    from src.Ui.AddPage import AddWidget


class Builder(QWidget):
    """
    ## 机器人生成器页面
        - 询问用户是自定义机器人还是导入已有配置
        - 选择自定义机器人后，进入自定义机器人页面
        - 选择导入已有配置后，进入导入已有配置页面

    ## TODO
        - 实现模板机器人配置
    """

    def __init__(self, parent: "AddWidget") -> None:
        super().__init__(parent=parent)

        # 创建控件
        self.hBoxLayout = QHBoxLayout()
        self.vBoxLayout = QVBoxLayout(self)
        self.titleLabel = TitleLabel("选择一个开始吧 ₍˄·͈༝·͈˄*₎੭", self)
        self.customCard = BuilderAppCard(FluentIcon.APPLICATION, "自定义机器人")
        self.importCard = BuilderAppCard(FluentIcon.DOWNLOAD, "导入已有配置")

        # 调整布局
        self.vBoxLayout.addWidget(self.titleLabel, alignment=Qt.AlignmentFlag.AlignHCenter)
        self.vBoxLayout.addLayout(self.hBoxLayout)
        self.vBoxLayout.setSpacing(10)
        self.vBoxLayout.setAlignment(Qt.AlignmentFlag.AlignCenter)

        self.hBoxLayout.addWidget(self.customCard)
        self.hBoxLayout.addWidget(self.importCard)
        self.hBoxLayout.setSpacing(20)
        self.hBoxLayout.setAlignment(Qt.AlignmentFlag.AlignCenter)

    def paintEvent(self, event):
        super().paintEvent(event)
        self.customCard.setFixedSize(int(self.width() // 3), int(self.height() // 2))
        self.importCard.setFixedSize(int(self.width() // 3), int(self.height() // 2))
        self.vBoxLayout.setContentsMargins(0, int(self.height() // 8), 0, int(self.height() // 8))

