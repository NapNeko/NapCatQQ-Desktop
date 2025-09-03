# -*- coding: utf-8 -*-
# 标准库导入
from typing import TYPE_CHECKING, Self, Optional

# 项目内模块导入
from src.core.utils.singleton import singleton
from src.ui.common.style_sheet import StyleSheet
from src.ui.components.stacked_widget import TransparentStackedWidget
from src.ui.page.home_page.display_view import DisplayViewWidget

if TYPE_CHECKING:
    # 项目内模块导入
    from src.ui.window.main_window import main_window


@singleton
class HomeWidget(TransparentStackedWidget):
    displayView: Optional[DisplayViewWidget]

    def __init__(self) -> None:
        super().__init__()

    def initialize(self, parent: "main_window") -> Self:
        """
        初始化
        """
        # 创建控件
        self.displayView = DisplayViewWidget()

        # 设置控件
        self.setParent(parent)
        self.setObjectName("home_page")
        self.addWidget(self.displayView)
        self.setCurrentWidget(self.displayView)

        # 链接信号
        self.displayView.buttonGroup.goButton.clicked.connect(lambda: self.parent().setCurrentIndex(1))

        # 应用样式表
        StyleSheet.HOME_WIDGET.apply(self)

        return self
