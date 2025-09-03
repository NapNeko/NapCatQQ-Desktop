# -*- coding: utf-8 -*-
# 标准库导入
from typing import TYPE_CHECKING, Self, Optional

# 项目内模块导入
from src.ui.common.style_sheet import StyleSheet
from src.core.utils.singleton import singleton
from src.ui.page.HomePage.DisplayView import DisplayViewWidget
from src.ui.components.stacked_widget import TransparentStackedWidget

if TYPE_CHECKING:
    # 项目内模块导入
    from src.ui.window.MainWindow import MainWindow


@singleton
class HomeWidget(TransparentStackedWidget):
    displayView: Optional[DisplayViewWidget]

    def __init__(self) -> None:
        super().__init__()

    def initialize(self, parent: "MainWindow") -> Self:
        """
        初始化
        """
        # 创建控件
        self.displayView = DisplayViewWidget()

        # 设置控件
        self.setParent(parent)
        self.setObjectName("HomePage")
        self.addWidget(self.displayView)
        self.setCurrentWidget(self.displayView)

        # 链接信号
        self.displayView.buttonGroup.goButton.clicked.connect(lambda: self.parent().setCurrentIndex(1))

        # 应用样式表
        StyleSheet.HOME_WIDGET.apply(self)

        return self
