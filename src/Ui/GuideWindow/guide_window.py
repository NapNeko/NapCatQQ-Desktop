# -*- coding: utf-8 -*-
# 第三方库导入
from qframelesswindow import FramelessWindow
from PySide6.QtWidgets import QApplication

# 项目内模块导入
from src.Core.Utils.singleton import singleton


@singleton
class GuideWindow(FramelessWindow):

    def __init__(self) -> None:
        super().__init__()

    def initialize(self) -> None:
        """初始化窗体设置"""
        self.show()

        # 设置窗体大小以及设置打开时居中
        self.setFixedSize(600, 450)
        desktop = QApplication.screens()[0].availableGeometry()
        width, height = desktop.width(), desktop.height()
        self.move(width // 2 - self.width() // 2, height // 2 - self.height() // 2)

        # 隐藏无用的按钮
        self.titleBar.maxBtn.hide()
        self.titleBar.minBtn.hide()
        self.titleBar.closeBtn.hide()

    def create_page(self) -> None: ...
