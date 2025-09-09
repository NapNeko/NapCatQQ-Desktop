# -*- coding: utf-8 -*-
# 标准库导入
from typing import TYPE_CHECKING, Self

from PySide6.QtWidgets import QVBoxLayout, QWidget

# 项目内模块导入
from src.core.utils.singleton import singleton
from src.ui.common.style_sheet import StyleSheet
from src.ui.components.code_editor import CodeExibit
from src.ui.components.stacked_widget import TransparentStackedWidget
from src.ui.page.setup_page.general import General
from src.ui.page.setup_page.personalization import Personalization
from src.ui.page.setup_page.setup_top_card import SetupTopCard

if TYPE_CHECKING:
    # 项目内模块导入
    from src.ui.window.main_window import MainWindow


@singleton
class SetupWidget(QWidget):
    """设置页面"""

    top_card: SetupTopCard
    v_box_layout: QVBoxLayout
    infoWidget: CodeExibit

    view: TransparentStackedWidget
    personalization: Personalization

    def __init__(self):
        super().__init__()

    def initialize(self, parent: "MainWindow") -> Self:
        """初始化"""
        # 创建控件
        self.top_card = SetupTopCard(self)
        self.v_box_layout = QVBoxLayout()
        self._create_view()

        # 调整控件
        self.setParent(parent)
        self.setObjectName("setup_page")
        self.view.setObjectName("SetupView")

        # 设置布局
        self.v_box_layout.addWidget(self.top_card)
        self.v_box_layout.addWidget(self.view)
        self.v_box_layout.setContentsMargins(24, 20, 24, 10)
        self.setLayout(self.v_box_layout)

        # 应用样式表
        StyleSheet.SETUP_WIDGET.apply(self)
        return self

    def _create_view(self) -> None:
        """创建并配置 QStackedWidget"""
        # 创建控件
        self.view = TransparentStackedWidget(self)
        self.general = General(self)
        self.personalization = Personalization(self)

        # 设置控件
        self.view.addWidget(self.personalization)
        self.view.addWidget(self.general)

        self.top_card.pivot.addItem(
            routeKey=self.personalization.objectName(),
            text=self.tr("个性化"),
            onClick=lambda: self.view.setCurrentWidget(self.personalization),
        )
        self.top_card.pivot.addItem(
            routeKey=self.general.objectName(),
            text=self.tr("常规"),
            onClick=lambda: self.view.setCurrentWidget(self.general),
        )

        # 连接信号并初始化当前标签页
        self.view.setCurrentWidget(self.personalization)
        self.top_card.pivot.setCurrentItem(self.personalization.objectName())
